/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

#![allow(dead_code)]
use crate::time_system::DbTs;
use serde::{Deserialize, Serialize};

fn is_false(v: &bool) -> bool {
    !*v
}

fn is_zero_i32(v: &i32) -> bool {
    *v == 0
}

fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}

fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RequestLog {
    pub id: i64,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_id: Option<String>,
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_id: Option<i64>,
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    /// 缓存命中的 Token 数量（属于输入的子集）
    #[sqlx(default)]
    pub cached_tokens: i64,
    pub cost: f64,
    pub latency_ms: i64,
    pub status_code: i64,
    pub endpoint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_url: Option<String>,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_request_id: Option<String>,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_group_aid: Option<String>,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yid: Option<String>, // 读路径由 JOIN channel_configs.yid 填充，非 logs 列
    /// 读路径 JOIN models.mid（l.model 匹配 mid 或 model_id）
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mid: Option<String>,
    /// 请求当时是否走 HA 组（写路径快照，非 JOIN 当前渠道）
    #[sqlx(default)]
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub is_ha: i64,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_content: Option<String>,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_content: Option<String>,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_response: Option<String>,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_req_content: Option<String>,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_stream: Option<i64>,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_name: Option<String>,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_kid: Option<String>,
    /// 令牌当前是否开启高可用（JOIN `api_tokens.high_availability`，非请求快照）
    #[sqlx(default)]
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub token_ha: i64,
    /// 是否创作中心专用令牌（JOIN `only_playground` / `only_playground_2026`）
    #[sqlx(default)]
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub token_pg: i64,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_nickname: Option<String>,
    /// 用户管理员备注（仅管理端列表展示）
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_admin_remark: Option<String>,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_group: Option<String>,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_level_name: Option<String>,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_uid: Option<String>,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_name: Option<String>,
    /// 列表通常为 None；全文走 detail
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billing_detail: Option<String>,
    /// 列表轻量标记（不传 billing_detail 全文）
    #[sqlx(default)]
    #[serde(default, skip_serializing_if = "is_false")]
    pub billing_refunded: bool,
    #[sqlx(default)]
    #[serde(default, skip_serializing_if = "is_false")]
    pub billing_failed: bool,
    /// 列表用量数字（SQL 从 billing_detail 抽出）
    #[sqlx(default)]
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub billing_cache_creation: i64,
    #[sqlx(default)]
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub billing_cache_read: i64,
    #[sqlx(default)]
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub billing_web_search: f64,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billing_pid: Option<String>,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forward_eid: Option<String>,
    /// POST 阶段提取的计费特征快照 (JSON)，独立于 enable_log 开关
    #[sqlx(default)]
    #[serde(skip_serializing)]
    pub billing_features: Option<String>,
    /// 预扣费中从赠送余额扣除的金额，用于退款时精准归还
    #[sqlx(default)]
    #[serde(default)]
    pub pre_deduct_gift: f64,
    /// 插件标记 JSON（列表/仪表盘不返回；仅详情接口）
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_tag: Option<String>,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_type: Option<String>,
    /// 任务是否已终结(1=已完成,0=进行中或待结算)
    #[sqlx(default)]
    #[serde(default)]
    pub is_completed: i64,
    /// 视频模型输入是否包含视频(1=含视频, 0=无视频)
    #[sqlx(default)]
    #[serde(default)]
    pub has_video: i64,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_config_id: Option<i64>,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_channel_name: Option<String>,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub created_at: DbTs,
    /// 归档时间（仅 logs_archive 冷表包含）
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<DbTs>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LogQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub user_id: Option<String>,
    pub model: Option<String>,
    pub channel_id: Option<i64>,
    pub channel_group_aid: Option<String>,
    pub token_id: Option<i64>,
    pub status: Option<String>,
    /// 精确 HTTP 状态码；可与 status=success/fail 叠加
    pub status_code: Option<i64>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub user_group: Option<String>,
    pub uid: Option<String>,
    pub router_ep: Option<String>,
    pub action_type: Option<String>,
    pub log_id: Option<String>,
    pub token_kid: Option<String>,
    pub task_id: Option<String>,
    pub search_keyword: Option<String>,
    /// 视频模型输入形态过滤：1=含视频，0=无视频
    pub has_video: Option<i64>,
    /// CSV 导出字段（逗号分隔 key）；空则全部。仅 `/logs/export` 使用。
    pub export_fields: Option<String>,
    /// 客户端已知的总记录数（翻页时回传，避免重跑 COUNT）
    pub known_total: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct LogListResponse {
    pub data: Vec<RequestLog>,
    pub total: i64,
    pub allow_details: bool,
    /// 筛选范围内的汇总统计
    pub total_cost: f64,
    pub success_count: i64,
    pub fail_count: i64,
    /// 输入 + 输出 token 合计（不含独立加计缓存，缓存已含在输入中）
    pub total_tokens: i64,
    /// 输入 token 合计（prompt_tokens）
    pub total_prompt_tokens: i64,
    /// 输出 token 合计（completion_tokens）
    pub total_completion_tokens: i64,
    /// 输出 token 中含视频部分（with_video，>0 时返回；无视频部分即 total_completion_tokens - total_with_video_tokens）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_with_video_tokens: Option<i64>,
    pub total_system_cost: Option<f64>,
    pub total_gift_cost: Option<f64>,
}

/// 日志详情大字段（列表不返回，展开时按需拉取）
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct LogDetailContent {
    pub id: i64,
    pub request_content: Option<String>,
    pub response_content: Option<String>,
    pub post_response: Option<String>,
    pub upstream_req_content: Option<String>,
    pub billing_detail: Option<String>,
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_request_id: Option<String>,
    /// 列表不查；展开按需。用户端已白名单投影。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_tag: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct DashboardStats {
    pub total_requests: i64,
    pub total_tokens: i64,
    pub total_cost: f64,
    pub total_users: i64,
    pub total_channels: i64,
    pub total_api_tokens: i64,
    pub today_requests: i64,
    pub today_tokens: i64,
    pub today_cost: f64,
    pub today_active_tokens: i64,
    pub yesterday_requests: i64,
    pub yesterday_tokens: i64,
    pub yesterday_cost: f64,
    pub yesterday_active_tokens: i64,
    pub recent_logs: Vec<RequestLog>,
    pub model_stats: Vec<ModelStat>,
    #[serde(default)]
    pub daily_trends: Vec<DashboardDailyTrend>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ModelStat {
    pub model: String,
    pub count: i64,
    pub total_tokens: Option<i64>,
    pub total_cost: Option<f64>,
    pub last_three_days: Vec<DashboardModelDailyStatInfo>,
}

#[derive(Debug, Serialize, Clone)]
pub struct DashboardModelDailyStatInfo {
    pub date: String,
    pub count: i64,
    pub total_cost: f64,
}

#[derive(Debug, Serialize, Clone, sqlx::FromRow)]
pub struct DashboardDailyTrend {
    pub date: String,
    pub requests: i64,
    pub cost: f64,
}
