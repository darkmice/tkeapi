# TokensByte Relay 中枢开发规范

> **适用范围**: `backend/src/relay/` 目录下的所有模块。  
> **最后更新**: 2026-08-27  
> **目的**: 确保模型转发、计费、日志、异步任务、HA 逻辑一致，防止扩展时引入遗漏。

---

## 一、整体架构总览

```
┌──────────────────────── Relay 中枢 ────────────────────────┐
│  OpenAI 兼容层              多模态原生层 (Native)            │
│  chat.rs / image.rs         native.rs (gemini / volc / ark) │
│  video.rs / audio.rs                                        │
│  generic.rs                                                 │
│            │                         │                      │
│            ▼                         ▼                      │
│  共享: proxy.rs / forward.rs / router.rs / ha.rs            │
│        usage_extractor.rs / stream.rs / task.rs             │
│        asset_convert.rs / url_utils.rs / response_formatter │
│            ▼                                                │
│  mod.rs::compute_cost() / calculate_relay_cost()            │
└─────────────────────────────────────────────────────────────┘
```

| 模块 | 职责 |
|------|------|
| `chat.rs` | Chat Completions / Responses |
| `image.rs` / `video.rs` / `audio.rs` / `generic.rs` | 对应模态入口 |
| `native.rs` | Gemini / 火山原生路径 |
| `proxy.rs` | 鉴权、预扣费、PendingLog/BillRecord、记账 |
| `ha.rs` | HA 策略、`HaAttempt`、首次失败 reinstate |
| `stream.rs` | SSE 流式 + 流后结算 |
| `task.rs` | 异步任务查询与结算（含后台轮询） |

---

## 二、双轨同步原则

| 层级 | 文件 | 入口 |
|------|------|------|
| OpenAI 兼容 | `chat.rs`, `image.rs`, `video.rs`, `audio.rs` | `/v1/...` |
| Native | `native.rs` | `/v1beta/...`, `/api/v3/...` |
| 任务轮询 | `task.rs` | `/v1/tasks/{id}` + 后台定时 |

**凡改 Usage / 计费 / 特征 / 预扣费 / 日志 / 异步判定，必须同时审查兼容层与 Native 层。**

改因遵循全项目规范（agent.md / `fix-root-cause-not-fallback.mdc`）。例：logs 短路先 status 再级联。

---

## 三、一条日志原则 + 记账 API

1. 请求前：`proxy::record_pending_log(PendingLog { ... })` → `status_code=0`
2. 完成后：`proxy::record_and_bill_inner(BillRecord { ... })` 传入同一 `pending_log_id`（UPDATE，禁止再 INSERT）
3. HA 重试复用同一条 pending；**中间失败不 UPDATE logs**；环结束由 `ha.fail` / 成功路径一次落库；过程写入 `ha_usage_logs`

### PendingLog / BillRecord

命名字段传参，禁止位置参数。关键字段：

| 结构 | 要点 |
|------|------|
| `PendingLog` | `category`、`forward_eid`、`db_model`、`requested_log_id` |
| `BillRecord` | `hint_category`、`pending_log_id`、`pre_deducted` / `pre_deduct_gift`、`billing_detail` |

### 预扣费

```rust
proxy::pre_deduct_or_intercept(..., category).await
// amount≤0 跳过；超管/管理员与普通用户同等预扣；失败写 403 日志并返回 AppError
// 预扣与 pending 日志 cost/pre_deduct_gift 同事务落账，崩溃可由孤儿清理退款
```

禁止各端点手写 `pre_deduct` + 403 落库副本（除非有特殊不可复用路径）。

### 上游失败零费用结算（预扣前 / 失败退费）

判定仍分两入口（勿合并成单一 if）：

1. `send` 失败或 HTTP 非 2xx → `prefer_http_status = Some(status)`
2. HTTP 200 但 `check_upstream_post_error` → `prefer_http_status = None`

记账复用（禁止再抄一整段 `BillRecord { cost: 0, ... }`）：

| API | 用途 |
|-----|------|
| `record_zero_cost_fail` | **只记账**；HA 终态 / 业务侧停切；调用方自行 `upstream_fail` / `BadRequest` / `PaymentRequired` |
| `HaAttempt::park` + `FailBill::transport|http|biz` | 上游失败暂存（不写 logs）→ 外环 `ha.fail`；中间失败不记账 |
| `spawn_protected` + `join_protected` | 连接保护：独立 task 跑完素材转换/上游/预扣/落库；拼 Response 在 task 外 |

`pre_deducted`/`pre_deduct_gift`：尚未预扣传 `0.0`；预扣后失败退费传已扣金额。  
成功计费、异步冻结、流结束结算：**不要**走上述 API。

状态码一致性：`record_zero_cost_fail` 写入的 `status_code` 与 `upstream_fail` / HA `first_fail` 均经 `norm_status`（仅保留 400–599，其余→502）。HTTP 200 业务失败走 body 推断后再规范化。分账与对外错误须共用同一码，禁止「日志推断码 + `upstream_fail(HTTP 200/硬编码 502)`」分叉。

---

## 四、HA（`ha.rs`）

```rust
let req_start = std::time::Instant::now();
let mut ha = HaAttempt::begin(&state, token.high_availability, req_start).await;
while ha.cont() {
    let attempt_start = std::time::Instant::now();
    // select → ha.on_select_err / access → ha.on_access_err
    // spawn 内：HaAttempt::park(&buf, FailBill::transport|http|biz(...), headers)
    let ctx = HaBillCtx::new(&state, &token, model, &ep).category("聊天").db(db_model.as_ref());
    if ha.fail(&ctx, &channel, e, Some(&url)).await {
        ha.bump(); continue;
    }
    break;
}
Err(ha.finish(&HaBillCtx::new(&state, &token, model, entry_path).category("聊天")).await)
```

- `FailBill::transport` / `http` / `biz`：三类上游失败账单；可选 `.stream` / `.detail` / `.pre` / `.content` / `.client`
- `spawn_protected(fut)` + `join_protected`：连接保护（素材转换/上游/预扣/落库，oneshot 回传）；拼 Response 在 Ok 分支；流泵仍用普通 `tokio::spawn`
- `park`：spawn 暂存 `FailBill` + 对外错误（不写 logs）
- `fail`：记 snap / 首败；续试则退预扣并清零首败预扣；末次强制停切后 `settle_first`+`save`
- `settle_first`：首败渠 + 首败 `endpoint` + `FailBill`；category 用当前 `HaBillCtx`
- `finish`：pending 仍处理中则补记；返回首败/`last_err`；再 `save`
- 业务侧：`record_zero_cost_*` + `on_access_err`（预扣失败勿 `?` 跳出）
- `ok`/`save`：仅真实 HA 渠写 `ha_usage_logs`
- `set_pending`：禁止 `None` 覆盖已有 id
- 对外：`HaAttempt` + `HaBillCtx::new` + `FailBill` + `spawn_protected` / `join_protected` + `policy` / …

---

## 五、计费流水线

```
get_user_context → check_access_with_model → select_channel → resolve_forward_rule
  → transform_request_body → 上游
  → [同步] usage → calculate_relay_cost → BillRecord
  → [异步] POST 冻结 pre_deduction → GET/轮询结算
```

- 唯一计费入口：`mod.rs` 的 `compute_cost` / `calculate_relay_cost`
- 折扣：`proxy::resolve_discount`（禁止各 handler 自算）
- 流式结算：`stream.rs::settle_after_stream`（各 handler 只保留 usage/features 提取）

### ExtractedFeatures 契约

调用计费前须填齐：`has_video` / `has_audio` / `duration_seconds` / `resolution` / `image_count` / `service_tier` 等。  
`has_video` 必须检测请求体实际引用，不能只靠类别名。

### 预扣费生命周期

POST 冻结（`billing_detail` 含「冻结」）→ GET 成功结算 / 失败退还。  
不含「冻结」= 已结算，禁止重复扣费（`already_billed`）。

---

## 六、Usage / 转发 / 素材（摘要）

- Usage：`usage_extractor::parse_usage`（OpenAI / Gemini / 火山 / SSE）
- 转发：`forward.rs`（`ResolvedForward`、`target_type`、白名单透传）
- 素材：`asset_convert.rs`（仅 `asset_convert==true`；失败不阻塞主请求）
- 异步任务：`poll_task_result` → `PollOutcome`（`Succeeded` / `Failed{timed_out}`）+ `PollTaskOpts`（查询前 5→1s，可重试达 `POLL_FAIL_LIMIT=40`，不可重试立即失败并保留上游文案）；级联裁剪/抽帧经插件 `crop_volc_cascade_video` / `extract_volc_last_frame`（腾讯为 `crop_tencent_cascade_video` / `extract_tencent_last_frame`）；增强状态仍由 GET/TaskPoller；后台周期见 `RelaySettings.poll_tick_secs`（缓存）
- 级联增强：S2 成功走 `cascade_on_s2_succeeded`（usage×res_mul + 按需抽帧写 stage2）；对外/用户端经 `cascade_s1_with_s2_url` 叠尾帧；落库 stage1 保持原尾帧
- 级联出片不变量：S2 完成前禁止展示 S1 成片（剥 content/data/video_url 等）；失败勿回退空 URL；logs 短路先 status 再级联；处理中无 `{id,status}` 空壳兜底
- 结算：`cascade_stage2_submit` 立刻返回、失败只落库；退费由下次 `prepare_poll` / task `settle_failure` 结案
- 级联裁剪：`crop_480p`（缺省 true）控制 720p←480 是否 MediaKit 裁剪；其它分辨率忽略
- 宽日志查询（>16 列）：用 `TaskRelayLogRow` + `FromRow` 一次查出，禁止拆成二次 query / 超长元组；轮询 SELECT 列与结构体字段对齐（`action_type AS category`，不查无用 `endpoint`）

---

## 七、新功能 Checklist（精简）

- [ ] 兼容层 + Native 对等审查
- [ ] PendingLog/BillRecord 命名字段 + 一条日志
- [ ] HA 用 `HaAttempt`，上游失败走 `fail`（中间不写 logs）
- [ ] 预扣费用 `pre_deduct_or_intercept`
- [ ] features / usage / 异步冻结结算完整
- [ ] HA 重载（轮询/取消/列表）用 `fetch_channel(..., channel_config_id)`
- [ ] 更新本文档日期与相关表

---

## 八、搜索速查

| 需求 | 关键词 |
|------|--------|
| 计费 | `calculate_relay_cost`, `compute_cost` |
| 日志 | `PendingLog`, `BillRecord`, `record_pending_log` |
| HA | `FailBill::transport|http|biz`, `park` / `fail` / `ok` / `finish`, `spawn_protected` / `join_protected` |
| 预扣费 | `pre_deduct_or_intercept` |
| Usage / 特征 | `parse_usage`, `extract_request_features` |
| 异步 | `already_billed`, `"冻结"`, `TaskRelayLogRow` |
