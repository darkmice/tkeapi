/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 高可用（HA）failover 策略：端点只碰 [`HaAttempt`] + 少量选渠/落库辅助。
//! 语义：仅当「插件启用 AND 令牌 high_availability≠0」时可选 HA 组并做子渠切换。
//!
//! 终态对齐规则：
//! - **全部上游失败**：客户端错误 + 日志的 status/error/`channel_config_id` 按规则 `err`（首败/末败）
//! - **某次成功**：日志 channel_id / `channel_config_id` / 上游 URL 使用**成功那一次**的子渠
//! - HA 中间失败不 UPDATE `logs`；环结束一次记账；插件表 `ha_usage_logs` 记全量子渠过程
//! - 展示用 YID：读路径 JOIN `channel_configs`，日志表不落 `yid` 列
//! - 展示用 HA：写路径快照 `logs.is_ha`（见 [`channel_is_ha_flag`]），禁止 JOIN 当前 `channels.provider_type`
//! - **墙钟预算**（仅 failover 开启）：备渠切换受规则 `budget` 约束，避免嵌套转发被入口 Nginx 切成 504 HTML
//! - **终态错误**：规则 `err=first` 对外首败；`err=last` 对外末次上游失败；`ha_usage_logs` 仍记全过程
//! - **停切原因**：写入末次 `attempts[].end`（无新列），解释未打满最大尝试的原因

use crate::error::AppError;
use crate::models::{ApiToken, Channel};
use crate::relay::ha_rule::{default_rule_arc, parse_rule_id, HaRule, HaRulesBundle};
use crate::AppState;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const FAILED_CHANNELS_SOFT_CAP: usize = 4096;
/// 备渠切换最少剩余预算；过短则收口首败
const MIN_RETRY_BUDGET: Duration = Duration::from_secs(5);
/// send 时剩余预算不足仍给极短超时，便于快速失败
const MIN_REQ_TIMEOUT: Duration = Duration::from_secs(1);

/// HA 整次墙钟预算秒数。`0`=自动 `min(900, 上游超时-60)`（留足多模态慢任务与重试回写裕量）。
fn resolve_budget_secs(configured: u64) -> u64 {
    if configured > 0 {
        configured
    } else {
        let upstream = crate::services::http_client::upstream_timeout_duration().as_secs();
        upstream.saturating_sub(60).min(900).max(60)
    }
}

/// spawn 内在真正 send 前重算超时（避免 transform 空耗导致超时过宽）
#[derive(Clone, Copy)]
pub struct HaTimeoutCtx {
    started_at: Instant,
    budget: Option<Duration>,
    ttfb: Option<Duration>,
}

impl HaTimeoutCtx {
    #[inline]
    fn remaining(self) -> Option<Duration> {
        self.budget
            .map(|b| b.saturating_sub(self.started_at.elapsed()))
    }

    /// 单次尝试超时（由规则 ttfb 配置，0 代表关闭）：
    /// 仅在 HA 模式且 ttfb > 0 时生效，同时受剩余总预算严格收敛
    #[inline]
    pub fn single_attempt_timeout(self) -> Option<Duration> {
        self.ttfb.map(|t| {
            self.remaining()
                .map(|rem| rem.min(t).max(MIN_REQ_TIMEOUT))
                .unwrap_or(t)
        })
    }

    /// 按当前墙钟解析本轮上游整请求兜底超时（针对非流式，无论首选或备选均受剩余预算约束）
    #[inline]
    pub fn resolve(self) -> Duration {
        let base = crate::services::http_client::upstream_timeout_duration();
        self.remaining()
            .filter(|&rem| rem < base)
            .map(|rem| rem.max(MIN_REQ_TIMEOUT))
            .unwrap_or(base)
    }

    /// 统一网络发送：
    /// - 若为 HA 组子渠且开启了单次超时（`ttfb > 0`）：
    ///   - 执行单次超时保护（超时切备渠防卡死）；
    ///   - 流式提示 TTFB 首包响应超时，非流式提示单次请求超时；
    /// - 若配置为 0（关闭）或普通单渠道：完全不设单次超时，沿用长等待原语。
    pub async fn send(
        self,
        builder: reqwest::RequestBuilder,
        is_stream: bool,
    ) -> Result<reqwest::Response, String> {
        if let Some(timeout) = self.single_attempt_timeout() {
            match tokio::time::timeout(timeout, builder.send()).await {
                Ok(Ok(resp)) => Ok(resp),
                Ok(Err(e)) => Err(e.to_string()),
                Err(_) => Err(if is_stream {
                    format!(
                        "上游首包响应超时 (TTFB 超过 {}s 未返回响应头)",
                        timeout.as_secs()
                    )
                } else {
                    format!("上游单次请求超时 (超过 {}s 未返回响应)", timeout.as_secs())
                }),
            }
        } else {
            builder.send().await.map_err(|e| e.to_string())
        }
    }
}

/// 日志展示用 yid（空 → `-`，与后台「上游 YID」对齐）
#[inline]
pub fn yid_label(yid: Option<&str>) -> &str {
    yid.map(str::trim).filter(|s| !s.is_empty()).unwrap_or("-")
}

/// 落库用 0/1（列类型 BIGINT / INT8）
#[inline]
pub fn channel_is_ha_flag(channel: &Channel) -> i64 {
    let is_ha = channel.provider_type == "high_availability_group"
        || channel.group_aid.as_deref().is_some_and(is_ha_aid);
    if is_ha { 1 } else { 0 }
}

/// 落库用子配 id：运行时 aid/preset 优先，否则用内存 `Channel.yid` 反查
pub async fn resolve_log_config_id(state: &AppState, channel: &Channel) -> Option<i64> {
    if let Some(id) = resolve_config_id(channel) {
        return Some(id);
    }
    let y = channel
        .yid
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    sqlx::query_scalar::<_, i64>(
        &state
            .db
            .format_query("SELECT id FROM channel_configs WHERE yid = ? LIMIT 1"),
    )
    .bind(y)
    .fetch_optional(&state.db.pool)
    .await
    .ok()
    .flatten()
}

/// HA 子渠道熔断键：`ha_group_{渠道id}_config_{上游配置id}`
#[inline]
pub fn ha_sub_channel_key(group_id: i64, config_id: i64) -> String {
    format!("ha_group_{}_config_{}", group_id, config_id)
}

/// 是否仍在熔断窗口内；已过期则立即移除该键
#[inline]
pub fn is_melted_down(state: &AppState, aid: &str) -> bool {
    let Some(entry) = state.failed_channels.get(aid) else {
        return false;
    };
    if *entry.value() > Instant::now() {
        true
    } else {
        drop(entry);
        state.failed_channels.remove(aid);
        false
    }
}

/// 批量清除过期熔断；超软上限则整清
pub fn scrub_failed_channels(state: &AppState) {
    let now = Instant::now();
    state.failed_channels.retain(|_, until| *until > now);
    let n = state.failed_channels.len();
    if n > FAILED_CHANNELS_SOFT_CAP {
        tracing::warn!(
            "[HA] 熔断表数量={} 超过软上限={}，整表清空",
            n,
            FAILED_CHANNELS_SOFT_CAP
        );
        state.failed_channels.clear();
    }
}

/// spawn 内上游失败暂存（不写 logs）；外环 [`HaAttempt::fail`] 终态再记账
#[derive(Debug, Default)]
pub struct FailBill {
    pub latency_ms: u32,
    pub response_body: String,
    pub response_content: Option<String>,
    pub upstream_req: Option<String>,
    pub billing_detail: Option<String>,
    pub prefer_status: Option<u16>,
    pub client_msg: Option<String>,
    pub pre_deducted: f64,
    pub pre_deduct_gift: f64,
    pub is_stream: i64,
    pub request_content: String,
    pub upstream_request_id: Option<String>,
}

impl FailBill {
    /// 传输/连接失败（默认 502；`response_content` 默认同 body）
    #[inline]
    pub fn transport(
        latency_ms: u32,
        msg: impl Into<String>,
        request: impl Into<String>,
        upstream: impl Into<String>,
    ) -> Self {
        let msg = msg.into();
        Self {
            latency_ms,
            response_body: msg.clone(),
            response_content: Some(msg),
            upstream_req: Some(upstream.into()),
            prefer_status: Some(502),
            request_content: request.into(),
            ..Default::default()
        }
    }

    /// HTTP 非 2xx（body 同时写入 response_body / response_content）
    /// 若网关/代理层返回 5xx 笼统码但响应体明确携带业务 4xx code（如内容风控 400），优先采纳业务状态码
    #[inline]
    pub fn http(
        latency_ms: u32,
        status: u16,
        body: impl Into<String>,
        request: impl Into<String>,
        upstream: impl Into<String>,
    ) -> Self {
        let body = body.into();
        let prefer_status = if (500..600).contains(&status) && !body.is_empty() {
            let biz_status = super::proxy::infer_error_status_code_from_str(&body);
            if (400..500).contains(&biz_status) {
                biz_status
            } else {
                status
            }
        } else {
            status
        };
        Self {
            latency_ms,
            response_body: body.clone(),
            response_content: Some(body),
            upstream_req: Some(upstream.into()),
            prefer_status: Some(prefer_status),
            request_content: request.into(),
            ..Default::default()
        }
    }

    /// HTTP 200 业务失败（直接从响应体提取上游业务 code 作为 prefer_status，避免置空退化为 502）
    #[inline]
    pub fn biz(
        latency_ms: u32,
        body: impl Into<String>,
        client_msg: impl Into<String>,
        request: impl Into<String>,
        upstream: impl Into<String>,
    ) -> Self {
        let body = body.into();
        let prefer_status = Some(super::proxy::infer_error_status_code_from_str(&body));
        Self {
            latency_ms,
            response_body: body.clone(),
            response_content: Some(body),
            upstream_req: Some(upstream.into()),
            prefer_status,
            client_msg: Some(client_msg.into()),
            request_content: request.into(),
            ..Default::default()
        }
    }

    #[inline]
    pub fn stream(mut self, s: i64) -> Self {
        self.is_stream = s;
        self
    }

    #[inline]
    pub fn detail(mut self, d: impl Into<String>) -> Self {
        self.billing_detail = Some(d.into());
        self
    }

    #[inline]
    pub fn detail_opt(mut self, d: Option<String>) -> Self {
        self.billing_detail = d;
        self
    }

    #[inline]
    pub fn pre(mut self, pre: f64, gift: f64) -> Self {
        self.pre_deducted = pre;
        self.pre_deduct_gift = gift;
        self
    }

    #[inline]
    pub fn content(mut self, c: Option<String>) -> Self {
        self.response_content = c;
        self
    }

    #[inline]
    pub fn body(mut self, b: impl Into<String>) -> Self {
        self.response_body = b.into();
        self
    }

    #[inline]
    pub fn client(mut self, m: impl Into<String>) -> Self {
        self.client_msg = Some(m.into());
        self
    }

    #[inline]
    pub fn upstream_request_id(mut self, req_id: Option<String>) -> Self {
        self.upstream_request_id = req_id;
        self
    }
}

/// [`HaAttempt::fail`] / [`HaAttempt::finish`] 共用记账上下文
pub struct HaBillCtx<'a> {
    pub state: &'a Arc<AppState>,
    pub token: &'a ApiToken,
    pub model: &'a str,
    pub ep: &'a str,
    pub hint_category: Option<&'a str>,
    pub db_model: Option<&'a crate::models::Model>,
}

impl<'a> HaBillCtx<'a> {
    #[inline]
    pub fn new(state: &'a Arc<AppState>, token: &'a ApiToken, model: &'a str, ep: &'a str) -> Self {
        Self {
            state,
            token,
            model,
            ep,
            hint_category: None,
            db_model: None,
        }
    }

    #[inline]
    pub fn category(mut self, c: &'a str) -> Self {
        self.hint_category = Some(c);
        self
    }

    #[inline]
    pub fn db(mut self, m: Option<&'a crate::models::Model>) -> Self {
        self.db_model = m;
        self
    }
}

fn u16_zero(v: &u16) -> bool {
    *v == 0
}

/// HA 子渠池快照（过滤项为 0 不落库）
#[derive(Debug, Clone, Default, Serialize)]
pub struct HaPoolSnap {
    pub bind: u16,
    pub live: u16,
    #[serde(skip_serializing_if = "u16_zero")]
    pub off: u16,
    #[serde(skip_serializing_if = "u16_zero")]
    pub melt: u16,
    #[serde(skip_serializing_if = "u16_zero")]
    pub quota: u16,
    #[serde(skip_serializing_if = "u16_zero")]
    pub excl: u16,
    #[serde(skip_serializing_if = "u16_zero")]
    pub gone: u16,
}

/// 插件 attempts 精简字段（末次可带 `end`=停切原因，无新表字段）
#[derive(Debug, Clone, Serialize)]
struct HaSnap {
    n: u16,
    #[serde(skip_serializing_if = "String::is_empty")]
    yid: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    name: String,
    status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    /// 上游返回的错误码（如 InvalidParameter），命中黑名单时可溯源
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    url: String,
    ms: u32,
    ok: u8,
    /// 停切原因（仅写在收口那次；运营解释「为何未打满最大尝试」）
    #[serde(skip_serializing_if = "Option::is_none")]
    end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pool: Option<HaPoolSnap>,
}

/// 落库快照：`err=first` 只记首次；`err=last` 每次覆盖
struct FirstFail {
    channel: Channel,
    status: u16,
    message: String,
    headers: Option<axum::http::HeaderMap>,
    bill: Option<FailBill>,
    endpoint: String,
}

/// HA 外环状态：各端点共用。上游失败走 [`HaAttempt::fail`]；业务侧 [`HaAttempt::on_access_err`]。
pub struct HaAttempt {
    pub exclude_aids: Vec<String>,
    pub attempt: usize,
    pub max_attempts: usize,
    pub failover_on: bool,
    pub pending_log_id: Option<i64>,
    pub had_upstream: bool,
    pub last_err: AppError,
    started_at: Instant,
    budget: Option<Duration>,
    fail_buf: Arc<Mutex<Option<FailBill>>>,
    snaps: Vec<HaSnap>,
    first: Option<FirstFail>,
    group_aid: Option<String>,
    billed: bool,
    saved: bool,
    bundle: Option<Arc<HaRulesBundle>>,
    rule: Arc<HaRule>,
    pool: Option<HaPoolSnap>,
    is_ha: bool,
}

impl HaAttempt {
    /// 开环：解析 HA 策略，初始化排除列表与终态错误占位（`started_at` 为请求入口全局总计时起点）
    pub async fn begin(state: &AppState, token_ha: i64, started_at: Instant) -> Self {
        let plugin_on = super::relay_settings::get_cached_ha_enabled(&state.db).await;
        let failover_on = plugin_on && token_ha != 0;
        let (bundle, rule, max_attempts, budget) = if failover_on {
            let bundle = super::relay_settings::get_cached_ha_rules(&state.db).await;
            let rule = bundle.resolve_arc(None);
            let max_attempts = rule.retries as usize;
            let budget_secs = resolve_budget_secs(rule.budget);
            crate::relay_debug!(
                "[HA] 开始 规则={} 最大尝试={} 令牌HA={} 墙钟预算={}s",
                rule.id,
                max_attempts,
                token_ha,
                budget_secs
            );
            (
                Some(bundle),
                rule,
                max_attempts,
                Some(Duration::from_secs(budget_secs)),
            )
        } else {
            (None, default_rule_arc(), 1, None)
        };
        Self {
            exclude_aids: vec![],
            attempt: 0,
            max_attempts,
            failover_on,
            pending_log_id: None,
            had_upstream: false,
            last_err: AppError::UpstreamError("No available models".into()),
            started_at,
            budget,
            fail_buf: Arc::new(Mutex::new(None)),
            snaps: Vec::new(),
            first: None,
            group_aid: None,
            billed: false,
            saved: false,
            bundle,
            rule,
            pool: None,
            is_ha: false,
        }
    }

    /// 全链路端到端总消耗时间（自方法入口 started_at 至今的毫秒数）
    #[inline]
    pub fn total_latency_ms(&self) -> u32 {
        self.started_at.elapsed().as_millis() as u32
    }

    /// 选渠侧写入的子渠池快照（仅 HA 组有值）
    #[inline]
    pub fn note_pool(&mut self, pool: Option<HaPoolSnap>) {
        if let Some(p) = pool {
            self.pool = Some(p);
        }
    }

    /// 选中 HA 组后绑定该组规则（未选 rule 则用默认）
    pub fn on_channel(&mut self, channel: &Channel) {
        let is_ha = self.failover_on && channel_is_ha_flag(channel) == 1;
        self.is_ha = is_ha;
        if !is_ha {
            return;
        }
        let Some(bundle) = &self.bundle else {
            return;
        };
        let Some(aid) = channel.group_aid.as_deref() else {
            return;
        };
        if !is_ha_aid(aid) {
            return;
        }
        self.group_aid = Some(group_key(channel));
        let rid = parse_rule_id(&channel.config);
        self.apply_rule(bundle.resolve_arc(rid.as_deref()));
    }

    fn apply_rule(&mut self, rule: Arc<HaRule>) {
        if self.rule.id == rule.id {
            return;
        }
        self.max_attempts = rule.retries as usize;
        self.budget = Some(Duration::from_secs(resolve_budget_secs(rule.budget)));
        crate::relay_debug!(
            "[HA] 绑定规则={} 最大尝试={} 墙钟预算={}s",
            rule.id,
            self.max_attempts,
            self.budget_secs()
        );
        self.rule = rule;
    }

    /// 供 spawn 克隆，写入 [`FailBill`]
    #[inline]
    pub fn buf(&self) -> Arc<Mutex<Option<FailBill>>> {
        Arc::clone(&self.fail_buf)
    }

    /// spawn 内：暂存账单并构造对外错误（不写 logs）
    #[inline]
    pub fn park(
        buf: &Arc<Mutex<Option<FailBill>>>,
        mut bill: FailBill,
        headers: Option<axum::http::HeaderMap>,
    ) -> AppError {
        if bill.upstream_request_id.is_none() {
            if let Some(ref h) = headers {
                bill.upstream_request_id =
                    crate::relay::upstream_headers::extract_upstream_request_id(h);
            }
        }
        let err = err_of(&bill, headers);
        if let Ok(mut g) = buf.lock() {
            *g = Some(bill);
        }
        err
    }

    #[inline]
    fn take_bill(&self) -> Option<FailBill> {
        self.fail_buf.lock().ok().and_then(|mut g| g.take())
    }

    #[inline]
    fn remaining_budget(&self) -> Option<Duration> {
        self.timeout_ctx().remaining()
    }

    #[inline]
    fn budget_secs(&self) -> u64 {
        self.budget.map(|d| d.as_secs()).unwrap_or(0)
    }

    #[inline]
    fn has_budget_for_retry(&self) -> bool {
        self.remaining_budget()
            .is_none_or(|rem| rem >= MIN_RETRY_BUDGET)
    }

    /// 快照供 spawn 在 send 前 [`HaTimeoutCtx::resolve`] 与 [`HaTimeoutCtx::send`]
    #[inline]
    pub fn timeout_ctx(&self) -> HaTimeoutCtx {
        HaTimeoutCtx {
            started_at: self.started_at,
            budget: self.budget,
            ttfb: if self.is_ha && self.rule.ttfb > 0 {
                Some(Duration::from_secs(self.rule.ttfb))
            } else {
                None
            },
        }
    }

    #[inline]
    pub fn cont(&self) -> bool {
        if self.attempt >= self.max_attempts {
            return false;
        }
        // 首次尝试不受墙钟预算限制；备渠需剩余预算
        if self.attempt > 0 && !self.has_budget_for_retry() {
            crate::relay_debug!(
                "[HA] 墙钟预算耗尽 已耗时={}s 预算={}s 尝试={}/{}，收口首败",
                self.started_at.elapsed().as_secs(),
                self.budget_secs(),
                self.attempt,
                self.max_attempts
            );
            return false;
        }
        true
    }

    /// 末次 snap 记停切原因（无新列）
    #[inline]
    fn mark_end(&mut self, reason: impl Into<String>) {
        if let Some(s) = self.snaps.last_mut() {
            s.end = Some(reason.into());
        }
    }

    /// 选渠失败：尚无上游交互时用选渠错误作为对外文案
    pub fn on_select_err(&mut self, e: AppError) {
        if !self.had_upstream {
            self.last_err = e;
        } else {
            self.mark_end("无可用子渠（已排除或熔断）");
        }
    }

    /// 权限/余额等不可 failover 错误
    pub fn on_access_err(&mut self, e: AppError) {
        self.mark_end("业务侧停止（权限/余额等）");
        self.last_err = e;
    }

    /// 上游失败：push snap；HA 续试则不写 logs；停环则按终态快照一次记账并 [`save`]。
    /// 返回 true → `bump(); continue`。
    pub async fn fail(
        &mut self,
        ctx: &HaBillCtx<'_>,
        channel: &Channel,
        err: AppError,
        url: Option<&str>,
    ) -> bool {
        if is_access_side_err(&err) {
            let _ = self.take_bill();
            crate::relay_debug!(
                "[HA] 业务侧停止 状态码={} 上游YID={} 尝试={}/{}",
                err.http_status(),
                yid_label(channel.yid.as_deref()),
                self.attempt + 1,
                self.max_attempts
            );
            self.on_access_err(err);
            return false;
        }

        let bill = self.take_bill();
        let (fail_status, msg) = match &bill {
            Some(b) => fail_status_msg(b),
            None => status_msg(&err),
        };
        let latency_ms = bill.as_ref().map(|b| b.latency_ms).unwrap_or(0);
        let masked = url.map(|u| super::forward::mask_key_in_string(u, &channel.api_key));

        self.had_upstream = true;
        let upstream_headers = match &err {
            AppError::UpstreamHttpError(_, _, h) => h.clone(),
            _ => None,
        };
        self.last_err = err;
        // err_code 在 bill.take() 之前提取，否则首败 bill 被 move 后拿不到
        let err_code = bill
            .as_ref()
            .and_then(|b| extract_err_code(&b.response_body));
        let mut bill = bill;
        if self.first.is_none() || self.rule.err_last() {
            self.first = Some(FirstFail {
                channel: channel.clone(),
                status: fail_status,
                message: msg.clone(),
                headers: upstream_headers,
                bill: bill.take(),
                endpoint: ctx.ep.to_string(),
            });
        }

        self.push_snap(
            channel,
            fail_status,
            Some(super::proxy::extract_error_message(&msg)),
            err_code.clone(),
            masked.as_deref().unwrap_or(""),
            latency_ms,
            false,
        );

        if self.try_switch(ctx.state, channel, fail_status, &msg, err_code.as_deref()) {
            self.refund_continue(ctx, bill.as_ref()).await;
            return true;
        }

        // err=first：保留首败账单，仅将末败预扣额并入一次落库
        if let Some(last) = bill {
            if let Some(f) = self.first.as_mut() {
                if let Some(b) = f.bill.as_mut() {
                    b.pre_deducted = last.pre_deducted;
                    b.pre_deduct_gift = last.pre_deduct_gift;
                } else if last.pre_deducted > 0.0 || last.pre_deduct_gift > 0.0 {
                    f.bill = Some(FailBill {
                        pre_deducted: last.pre_deducted,
                        pre_deduct_gift: last.pre_deduct_gift,
                        ..Default::default()
                    });
                }
            }
        }
        self.settle_first(ctx).await;
        self.save(ctx.state).await;
        false
    }

    /// 续试：有预扣时按日志行退款，并清零首败预扣字段防 finish 双退
    async fn refund_continue(&mut self, ctx: &HaBillCtx<'_>, last: Option<&FailBill>) {
        let has_pre = last
            .map(|b| b.pre_deducted > 0.0 || b.pre_deduct_gift > 0.0)
            .or_else(|| {
                self.first
                    .as_ref()
                    .and_then(|f| f.bill.as_ref())
                    .map(|b| b.pre_deducted > 0.0 || b.pre_deduct_gift > 0.0)
            })
            .unwrap_or(false);
        if has_pre {
            if let Some(log_id) = self.pending_log_id {
                super::proxy::refund_pending(ctx.state, log_id).await;
            }
        }
        if let Some(fb) = self.first.as_mut().and_then(|f| f.bill.as_mut()) {
            fb.pre_deducted = 0.0;
            fb.pre_deduct_gift = 0.0;
        }
    }

    /// 成功子渠：记 snap + 写插件表（主站 logs 已由外部 record_and_bill）
    pub async fn ok(&mut self, state: &AppState, channel: &Channel, url: &str, ms: u32) {
        self.billed = true;
        let masked = super::forward::mask_key_in_string(url, &channel.api_key);
        self.push_snap(channel, 200, None, None, &masked, ms, true);
        self.save(state).await;
    }

    /// 写入 `ha_usage_logs`（仅有 HA 子渠 snap 时）
    pub async fn save(&mut self, state: &AppState) {
        if self.saved || self.snaps.is_empty() {
            return;
        }
        // 精简优化：只记录包含错误/失败子渠尝试的日志。
        // 如果全部尝试都成功（通常意味着一次调用即成功，无任何错误），则不重复记录
        if self.snaps.iter().all(|s| s.ok == 1) {
            return;
        }
        let Some(log_id) = self.pending_log_id else {
            return;
        };
        if let Some(s) = self.snaps.last_mut() {
            s.pool = self.pool.clone();
        }
        let attempts = match serde_json::to_value(&self.snaps) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("[HA] attempts 序列化失败: {}", e);
                return;
            }
        };
        let (final_ok, final_status) = self
            .snaps
            .iter()
            .rev()
            .find(|s| s.ok == 1)
            .map(|s| (true, s.status as i32))
            .unwrap_or_else(|| {
                (
                    false,
                    self.first.as_ref().map(|f| f.status as i32).unwrap_or(0),
                )
            });
        let group_aid = self.group_aid.clone().unwrap_or_default();
        let n = self.snaps.len() as i64;
        if let Err(e) = sqlx::query(
            &state.db.format_query(
                "INSERT INTO ha_usage_logs (log_id, group_aid, attempt_count, final_ok, final_status_code, attempts) \
                 VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT (log_id) DO NOTHING",
            ),
        )
        .bind(log_id)
        .bind(&group_aid)
        .bind(n)
        .bind(if final_ok { 1i64 } else { 0i64 })
        .bind(final_status as i64)
        .bind(sqlx::types::Json(attempts))
        .execute(&state.db.pool)
        .await
        {
            tracing::warn!("[HA] 写 ha_usage_logs 失败 日志id={}: {:?}", log_id, e);
            return;
        }
        self.saved = true;
    }

    #[inline]
    pub fn bump(&mut self) {
        self.attempt += 1;
    }

    /// 写入 pending id；禁止用 None 覆盖已有 id（防 HA 重试丢 id → 双日志）
    #[inline]
    pub fn set_pending(&mut self, id: Option<i64>) {
        if id.is_some() {
            self.pending_log_id = id;
        }
    }

    /// 环结束：补记仍为处理中的首败 → 写插件表 → 返回对外错误
    pub async fn finish(mut self, ctx: &HaBillCtx<'_>) -> AppError {
        crate::relay_debug!(
            "[HA] 结束 已尝试={} 最大={} 已耗时={}s 预算={}s 已排除={:?}",
            self.attempt,
            self.max_attempts,
            self.started_at.elapsed().as_secs(),
            self.budget_secs(),
            self.exclude_aids
        );
        if !self.billed && self.first.is_some() {
            if let Some(id) = self.pending_log_id {
                if pending_open(ctx.state, id).await {
                    self.settle_first(ctx).await;
                }
            }
        }
        self.save(ctx.state).await;

        if is_access_side_err(&self.last_err) {
            return self.last_err;
        }
        match self.first {
            Some(f) => super::proxy::upstream_fail(
                f.status,
                &f.message,
                f.headers,
                ctx.ep,
            ),
            None => self.last_err,
        }
    }

    /// 按首败一次记账（仅 pending 复用；无 pending 则放弃以防双日志）
    async fn settle_first(&mut self, ctx: &HaBillCtx<'_>) {
        if self.billed {
            return;
        }
        if self.pending_log_id.is_none() {
            tracing::warn!(
                "[HA] 终态记账缺少 pending_log_id，放弃落库以防双日志 上游YID={}",
                self.first
                    .as_ref()
                    .map(|f| yid_label(f.channel.yid.as_deref()))
                    .unwrap_or("-")
            );
            return;
        }
        let total_latency_ms = self.total_latency_ms();
        let pending_log_id = self.pending_log_id;
        let Some(first) = self.first.as_mut() else {
            return;
        };
        let status = first.status;
        let fallback = first.message.clone();
        let channel = first.channel.clone();
        let endpoint = if first.endpoint.is_empty() {
            ctx.ep
        } else {
            first.endpoint.as_str()
        };

        let bill = first.bill.take();
        let prefer_http_status = match &bill {
            Some(b) => b.prefer_status.or(Some(status)),
            None => Some(status),
        };
        let (
            pre,
            gift,
            is_stream,
            req,
            resp_c,
            up_req,
            detail,
            _,
            response_body,
            client,
            up_req_id,
        ) = match bill {
            Some(b) => {
                let body = if b.response_body.is_empty() {
                    fallback.clone()
                } else {
                    b.response_body
                };
                let client = b
                    .client_msg
                    .unwrap_or_else(|| super::proxy::extract_error_message(&body));
                (
                    b.pre_deducted,
                    b.pre_deduct_gift,
                    b.is_stream,
                    b.request_content,
                    b.response_content,
                    b.upstream_req,
                    b.billing_detail,
                    b.latency_ms,
                    body,
                    client,
                    b.upstream_request_id,
                )
            }
            None => (
                0.0,
                0.0,
                0,
                String::new(),
                None,
                None,
                None,
                0,
                fallback.clone(),
                super::proxy::extract_error_message(&fallback),
                None,
            ),
        };
        let response_content =
            resp_c.or_else(|| (!response_body.is_empty()).then(|| response_body.clone()));

        let _ = super::proxy::record_zero_cost_fail(super::proxy::ZeroCostUpstreamFail {
            state: ctx.state,
            token: ctx.token,
            channel: &channel,
            model: ctx.model,
            prefer_http_status,
            endpoint,
            latency_ms: total_latency_ms,
            is_stream,
            request_content: req,
            response_body,
            response_content,
            upstream_req_content: up_req,
            billing_detail: detail,
            hint_category: ctx.hint_category,
            pending_log_id,
            db_model: ctx.db_model,
            client_msg: Some(&client),
            pre_deducted: pre,
            pre_deduct_gift: gift,
            upstream_request_id: up_req_id,
        })
        .await;
        self.billed = true;
    }

    fn push_snap(
        &mut self,
        channel: &Channel,
        status: u16,
        error: Option<String>,
        code: Option<String>,
        url: &str,
        ms: u32,
        ok: bool,
    ) {
        if channel_is_ha_flag(channel) == 0 {
            return;
        }
        if self.group_aid.is_none() {
            self.group_aid = Some(group_key(channel));
        }
        let yid = channel
            .yid
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("")
            .to_string();
        let n = (self.snaps.len() as u16).saturating_add(1);
        self.snaps.push(HaSnap {
            n,
            yid,
            name: channel.name.clone(),
            status,
            error,
            code,
            url: url.to_string(),
            ms,
            ok: u8::from(ok),
            end: None,
            pool: None,
        });
    }

    /// 熔断 / exclude；返回是否续试（停切时写入末次 snap.end）
    fn try_switch(
        &mut self,
        state: &Arc<AppState>,
        channel: &Channel,
        fail_status: u16,
        msg: &str,
        err_code: Option<&str>,
    ) -> bool {
        let aid = channel.group_aid.as_deref().unwrap_or("-");
        let yid = yid_label(channel.yid.as_deref());
        let n = self.attempt + 1;

        if let Some(reason) = try_failover(
            state,
            self.failover_on,
            channel.group_aid.as_deref(),
            channel.yid.as_deref(),
            fail_status,
            msg,
            err_code,
            &mut self.exclude_aids,
            &self.rule,
        ) {
            self.mark_end(reason);
            crate::relay_debug!(
                "[HA] 停止无切换 状态码={} 上游YID={} 子渠标识={} 尝试={}/{}",
                fail_status,
                yid,
                aid,
                n,
                self.max_attempts
            );
            return false;
        }
        if !self.has_budget_for_retry() {
            let elapsed_s = self.started_at.elapsed().as_secs();
            let budget_s = self.budget_secs();
            crate::relay_debug!(
                "[HA] 切换取消(预算不足) 状态码={} 上游YID={} 子渠标识={} 已耗时={}s 预算={}s",
                fail_status,
                yid,
                aid,
                elapsed_s,
                budget_s
            );
            self.mark_end(format!("墙钟预算不足（已用{elapsed_s}s/{budget_s}s）"));
            return false;
        }
        if n >= self.max_attempts {
            crate::relay_debug!(
                "[HA] 切换取消(已达最大尝试) 状态码={} 上游YID={} 子渠标识={} 尝试={}/{}",
                fail_status,
                yid,
                aid,
                n,
                self.max_attempts
            );
            self.mark_end(format!("已达最大尝试 {n}/{}", self.max_attempts));
            return false;
        }

        crate::relay_debug!(
            "[HA] 切换 状态码={} 上游YID={} 子渠标识={} 尝试={}/{} 已排除={} 剩余预算={}s",
            fail_status,
            yid,
            aid,
            n,
            self.max_attempts,
            self.exclude_aids.len(),
            self.remaining_budget().map(|d| d.as_secs()).unwrap_or(0)
        );
        true
    }
}

// ── 私有实现 ──────────────────────────────────────────────────

#[inline]
fn is_ha_aid(aid: &str) -> bool {
    aid.starts_with("ha_group_")
}

/// pending 行仍为处理中（status_code=0）才允许补记终态，避免覆盖已落库行
async fn pending_open(state: &AppState, log_id: i64) -> bool {
    let code: Option<i64> = sqlx::query_scalar(
        &state
            .db
            .format_query("SELECT status_code FROM logs WHERE id = ?"),
    )
    .bind(log_id)
    .fetch_optional(&state.db.pool)
    .await
    .ok()
    .flatten();
    matches!(code, Some(0))
}

fn group_key(channel: &Channel) -> String {
    if let Some(aid) = channel.group_aid.as_deref() {
        if let Some(pos) = aid.find("_config_") {
            return aid[..pos].to_string();
        }
        if is_ha_aid(aid) {
            return aid.to_string();
        }
    }
    format!("ha_group_{}", channel.id)
}

#[inline]
pub(crate) fn ha_config_id_from_aid(channel: &Channel) -> Option<i64> {
    let aid = channel.group_aid.as_deref()?;
    if !is_ha_aid(aid) {
        return None;
    }
    let pos = aid.rfind("_config_")?;
    aid[pos + "_config_".len()..].parse().ok()
}

#[inline]
fn resolve_config_id(channel: &Channel) -> Option<i64> {
    ha_config_id_from_aid(channel).or(channel.preset_id)
}

#[inline]
fn status_msg(err: &AppError) -> (u16, String) {
    match err {
        AppError::UpstreamHttpError(s, m, _) => (*s, m.clone()),
        AppError::UpstreamError(m) => (502, m.clone()),
        other => (other.http_status(), other.message()),
    }
}

fn fail_status_msg(b: &FailBill) -> (u16, String) {
    let raw_status = b
        .prefer_status
        .unwrap_or_else(|| super::proxy::infer_error_status_code_from_str(&b.response_body));
    let status = super::proxy::normalize_error_http_status(raw_status);
    let msg = match b.prefer_status {
        Some(_) => super::proxy::upstream_error_text(status, &b.response_body),
        None => super::proxy::extract_error_message(&b.response_body),
    };
    (status, msg)
}

/// 由暂存账单构造上游错误（不写库，保留原始错误体，由 finish 按入口路径统一决策是否转 OpenAI）
fn err_of(bill: &FailBill, headers: Option<axum::http::HeaderMap>) -> AppError {
    let (status, msg) = fail_status_msg(bill);
    let raw = bill.client_msg.as_deref().unwrap_or(&msg);
    AppError::UpstreamHttpError(status, raw.to_string(), headers)
}

/// 业务侧错误：禁止 HA failover
#[inline]
fn is_access_side_err(err: &AppError) -> bool {
    match err {
        AppError::PaymentRequired(_)
        | AppError::Forbidden(_)
        | AppError::BadRequest(_)
        | AppError::Unauthorized
        | AppError::AuthFailed(_) => true,
        AppError::Internal(m) if m.contains("预扣费") => true,
        _ => false,
    }
}

/// 仅 HA 子渠且策略开启时 exclude（并可能熔断）；黑名单命中则停止切换；未配置熔断码只切换不熔断
/// `None`=可续试；`Some(reason)`=停切原因
fn try_failover(
    state: &Arc<AppState>,
    failover_on: bool,
    group_aid: Option<&str>,
    yid: Option<&str>,
    status: u16,
    err_msg: &str,
    err_code: Option<&str>,
    exclude_aids: &mut Vec<String>,
    rule: &HaRule,
) -> Option<String> {
    if !failover_on {
        return Some("未启用切换".into());
    }
    let Some(aid) = group_aid.filter(|a| is_ha_aid(a)) else {
        return Some("非高可用子渠".into());
    };
    let yid_disp = yid_label(yid);

    if let Some(pattern) = match_err_keywords(err_msg, err_code, &rule.deny) {
        crate::relay_debug!(
            "[HA] 黑名单停止切换 状态码={} 上游YID={} 子渠标识={} 关键词={}",
            status,
            yid_disp,
            aid,
            pattern
        );
        return Some(format!("命中错误黑名单「{pattern}」"));
    }

    trigger_ha_meltdown(state, aid, status, err_msg, err_code, yid, rule);
    if !exclude_aids.iter().any(|a| a == aid) {
        exclude_aids.push(aid.to_string());
    }
    None
}

fn match_err_keywords(
    error_message: &str,
    err_code: Option<&str>,
    list: &[String],
) -> Option<String> {
    if list.is_empty() {
        return None;
    }

    // 1. 优先上游结构化错误码精准匹配（大小写无关）
    if let Some(code) = err_code.map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(matched) = list.iter().find(|p| p.trim().eq_ignore_ascii_case(code)) {
            return Some(matched.clone());
        }
    }

    // 2. 兜底错误文本子串模糊匹配
    if !error_message.is_empty() {
        let err_lower = error_message.to_lowercase();
        return list
            .iter()
            .find(|p| {
                let p_trimmed = p.trim();
                !p_trimmed.is_empty() && err_lower.contains(&p_trimmed.to_lowercase())
            })
            .cloned();
    }

    None
}

fn trigger_ha_meltdown(
    state: &AppState,
    group_aid: &str,
    status_code: u16,
    error_message: &str,
    err_code: Option<&str>,
    yid: Option<&str>,
    rule: &HaRule,
) {
    let yid = yid_label(yid);

    if let Some(pattern) = match_err_keywords(error_message, err_code, &rule.allow) {
        crate::relay_debug!(
            "[HA] 白名单跳过熔断 上游YID={} 子渠标识={} 关键词={}",
            yid,
            group_aid,
            pattern
        );
        return;
    }

    let cooldown = rule.melt_secs(status_code);
    if cooldown > 0 {
        let block_until = Instant::now() + Duration::from_secs(cooldown as u64);
        state
            .failed_channels
            .insert(group_aid.to_string(), block_until);
        crate::relay_debug!(
            "[HA] 熔断 上游YID={} 子渠标识={} 状态码={} 冷却={}秒",
            yid,
            group_aid,
            status_code,
            cooldown
        );
    }

    static CLEANUP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = CLEANUP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if n % 32 == 0 || state.failed_channels.len() > 2048 {
        scrub_failed_channels(state);
    }
}

/// 从上游响应体 JSON 中提取 error.code（如 `InvalidParameter`），用于 snap 溯源。
/// 复用 `response_formatter::extract_error_code_from_value`，无匹配返回 `None`。
fn extract_err_code(body: &str) -> Option<String> {
    let raw = body.find('{').map(|i| &body[i..]).unwrap_or(body);
    let v = serde_json::from_str::<serde_json::Value>(raw).ok()?;
    super::response_formatter::extract_error_code_from_value(&v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ha_attempt_started_at_latency() {
        let past = Instant::now() - Duration::from_millis(50);
        let ha = HaAttempt {
            exclude_aids: vec![],
            attempt: 0,
            max_attempts: 1,
            failover_on: false,
            pending_log_id: None,
            had_upstream: false,
            last_err: AppError::UpstreamError("test".into()),
            started_at: past,
            budget: None,
            fail_buf: Arc::new(Mutex::new(None)),
            snaps: Vec::new(),
            first: None,
            group_aid: None,
            billed: false,
            saved: false,
            bundle: None,
            rule: default_rule_arc(),
            pool: None,
            is_ha: false,
        };
        let ms = ha.total_latency_ms();
        assert!(ms >= 50);
    }
}
