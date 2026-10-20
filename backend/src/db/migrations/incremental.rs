/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 拆分后新增迁移只追加到本文件末尾。
//! 历史迁移已归档沉淀至 ledger/ 模块。
//! 系统更新不得改写 `plugins.is_enabled`，也不得改写已有 MediaKit 预置模型的 `is_active`。
//! 新插件只 `INSERT ... ON CONFLICT (name) DO NOTHING`。

use sqlx::PgPool;
use std::collections::HashSet;

#[allow(unused_imports)]
use crate::db::migrations::{once_heavy_migration, once_migration};

pub async fn run(pool: &PgPool, done: &mut HashSet<String>) -> anyhow::Result<()> {
    // 拆分后最新迁移追加入口（单表轻量增量在此追加）
    once_migration!(
        pool,
        done,
        "marketing_membership_v1",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS coupon_deduct DOUBLE PRECISION NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN logs.coupon_deduct IS '预扣中由营销卡券面值承担的金额；退款须退回卡券，不能退成现金'",
        r#"CREATE TABLE IF NOT EXISTS mkt_member_plans (
            id BIGSERIAL PRIMARY KEY,
            name TEXT NOT NULL DEFAULT '',
            subtitle TEXT NOT NULL DEFAULT '',
            description TEXT NOT NULL DEFAULT '',
            price DOUBLE PRECISION NOT NULL DEFAULT 0,
            sort_order BIGINT NOT NULL DEFAULT 0,
            is_enabled BIGINT NOT NULL DEFAULT 1,
            card_role TEXT NOT NULL DEFAULT 'plan',
            accent TEXT NOT NULL DEFAULT 'plain',
            badge TEXT NOT NULL DEFAULT '',
            contact_url TEXT NOT NULL DEFAULT '',
            features_json TEXT NOT NULL DEFAULT '[]',
            tier_enabled BIGINT NOT NULL DEFAULT 0,
            target_group_key TEXT NOT NULL DEFAULT '',
            tier_duration_mode TEXT NOT NULL DEFAULT 'permanent',
            tier_duration_days BIGINT NOT NULL DEFAULT 0,
            upgrade_group_keys TEXT NOT NULL DEFAULT '[]',
            keep_group_keys TEXT NOT NULL DEFAULT '[]',
            credit_enabled BIGINT NOT NULL DEFAULT 0,
            credit_amount DOUBLE PRECISION NOT NULL DEFAULT 0,
            credit_duration_mode TEXT NOT NULL DEFAULT 'permanent',
            credit_duration_days BIGINT NOT NULL DEFAULT 0,
            model_ids TEXT NOT NULL DEFAULT '[]',
            limit_mode TEXT NOT NULL DEFAULT 'none',
            limit_count BIGINT NOT NULL DEFAULT 0,
            limit_window_days BIGINT NOT NULL DEFAULT 0,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "COMMENT ON TABLE mkt_member_plans IS '营销卡券会员可购买方案'",
        r#"CREATE TABLE IF NOT EXISTS mkt_member_orders (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            plan_id BIGINT NOT NULL,
            plan_name TEXT NOT NULL DEFAULT '',
            price_paid DOUBLE PRECISION NOT NULL DEFAULT 0,
            tier_applied BIGINT NOT NULL DEFAULT 0,
            tier_from TEXT NOT NULL DEFAULT '',
            tier_to TEXT NOT NULL DEFAULT '',
            tier_expires_at TIMESTAMPTZ,
            tier_reverted BIGINT NOT NULL DEFAULT 0,
            tier_note TEXT NOT NULL DEFAULT '',
            credit_id BIGINT,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_mkt_orders_user_plan ON mkt_member_orders (user_id, plan_id, created_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_mkt_orders_tier_expiry ON mkt_member_orders (tier_expires_at) WHERE tier_applied = 1 AND tier_reverted = 0",
        r#"CREATE TABLE IF NOT EXISTS mkt_member_credits (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            order_id BIGINT NOT NULL,
            plan_id BIGINT NOT NULL,
            face_value DOUBLE PRECISION NOT NULL DEFAULT 0,
            remaining DOUBLE PRECISION NOT NULL DEFAULT 0,
            model_ids TEXT NOT NULL DEFAULT '[]',
            expires_at TIMESTAMPTZ,
            status TEXT NOT NULL DEFAULT 'active',
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_mkt_credits_user_status ON mkt_member_credits (user_id, status)",
        r#"CREATE TABLE IF NOT EXISTS mkt_coupon_holds (
            id BIGSERIAL PRIMARY KEY,
            credit_id BIGINT NOT NULL,
            log_id BIGINT NOT NULL,
            user_id TEXT NOT NULL,
            amount DOUBLE PRECISION NOT NULL DEFAULT 0,
            restored DOUBLE PRECISION NOT NULL DEFAULT 0,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_mkt_holds_log ON mkt_coupon_holds (log_id)",
        r#"CREATE TABLE IF NOT EXISTS mkt_coupon_ledger (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            credit_id BIGINT,
            order_id BIGINT,
            kind TEXT NOT NULL DEFAULT '',
            amount DOUBLE PRECISION NOT NULL DEFAULT 0,
            model_name TEXT NOT NULL DEFAULT '',
            log_id BIGINT,
            remark TEXT NOT NULL DEFAULT '',
            expires_at TIMESTAMPTZ,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_mkt_ledger_user ON mkt_coupon_ledger (user_id, id DESC)",
        r#"INSERT INTO plugins (
            name, title, description, is_enabled, allowed_levels, category,
            show_in_admin_menu, admin_menu_sort, admin_menu_title, admin_menu_default_tab
        ) VALUES (
            'marketing_membership', '营销卡券会员',
            '购买卡券可以享受快速变更站点等级，模型优惠等。',
            0, 'all', 'user',
            0, 50, '', 'plans'
        ) ON CONFLICT (name) DO NOTHING"#
    );
    once_migration!(
        pool,
        done,
        "model_voucher_redemption_v1",
        "ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS code_type TEXT NOT NULL DEFAULT 'balance'",
        "ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS allowed_models TEXT NOT NULL DEFAULT '[]'",
        "ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS valid_days BIGINT NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN redemptions.code_type IS 'balance=余额兑换码, model_voucher=模型卡券兑换码'",
        "COMMENT ON COLUMN redemptions.allowed_models IS '模型卡券可消耗的模型 ID JSON 数组'",
        "COMMENT ON COLUMN redemptions.valid_days IS '卡券兑换后可消耗天数，0=长期有效'",
        r#"CREATE TABLE IF NOT EXISTS user_model_vouchers (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            redemption_id BIGINT NOT NULL,
            name TEXT NOT NULL DEFAULT '',
            face_value DOUBLE PRECISION NOT NULL,
            remaining DOUBLE PRECISION NOT NULL,
            allowed_models TEXT NOT NULL DEFAULT '[]',
            redeemed_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            expires_at TIMESTAMPTZ,
            status BIGINT NOT NULL DEFAULT 1,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_user_model_vouchers_user_fifo ON user_model_vouchers (user_id, redeemed_at, id)",
        r#"CREATE TABLE IF NOT EXISTS user_model_voucher_holds (
            id BIGSERIAL PRIMARY KEY,
            voucher_id BIGINT NOT NULL,
            log_id BIGINT NOT NULL,
            amount DOUBLE PRECISION NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_user_model_voucher_holds_log ON user_model_voucher_holds (log_id)",
        "ALTER TABLE logs ADD COLUMN IF NOT EXISTS pre_deduct_voucher DOUBLE PRECISION NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN logs.pre_deduct_voucher IS '本笔最终由模型卡券承担的金额，退款退回卡券，不退进余额'",
        "ALTER TABLE usage_daily_stats ADD COLUMN IF NOT EXISTS total_pre_deduct_voucher DOUBLE PRECISION NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN usage_daily_stats.total_pre_deduct_voucher IS '模型卡券计入调用的金额，系统消费统计需扣除，避免与兑换核销重复'"
    );
    once_migration!(
        pool,
        done,
        "redemption_activity_id_v1",
        "ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS activity_id TEXT NOT NULL DEFAULT ''",
        "COMMENT ON COLUMN redemptions.activity_id IS '活动ID，4位数字，同一活动的兑换码共用，不同活动不可重复'"
    );
    once_migration!(
        pool,
        done,
        "redemption_activity_id_backfill_v1",
        r#"DO $$
DECLARE
  rec RECORD;
  new_id TEXT;
  tries INT;
BEGIN
  UPDATE redemptions r
  SET activity_id = src.activity_id
  FROM (
    SELECT name, MAX(activity_id) AS activity_id
    FROM redemptions
    WHERE activity_id <> ''
    GROUP BY name
  ) src
  WHERE r.name = src.name AND r.activity_id = '';

  FOR rec IN
    SELECT DISTINCT name FROM redemptions WHERE activity_id = ''
  LOOP
    tries := 0;
    LOOP
      tries := tries + 1;
      new_id := lpad((floor(random() * 10000))::int::text, 4, '0');
      EXIT WHEN NOT EXISTS (SELECT 1 FROM redemptions WHERE activity_id = new_id);
      IF tries > 200 THEN
        RAISE EXCEPTION 'redemption activity id exhausted';
      END IF;
    END LOOP;
    UPDATE redemptions SET activity_id = new_id WHERE name = rec.name AND activity_id = '';
  END LOOP;
END $$;"#
    );
    if !done.contains("playground_2026_claude_fable_chat_scheme_v1") {
        match crate::db::migrations::helpers::claude_fable_chat::seed_playground_2026_claude_fable_chat_scheme(pool).await {
            Ok(_) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_claude_fable_chat_scheme_v1')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_claude_fable_chat_scheme_v1".into());
            }
            Err(e) => {
                tracing::warn!(
                    "playground_2026_claude_fable_chat_scheme_v1 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    if !done.contains("playground_2026_error_mappings_v1") {
        match crate::db::migrations::helpers::error_mappings::seed_playground_2026_error_mappings(pool).await {
            Ok(_) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_error_mappings_v1')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_error_mappings_v1".into());
            }
            Err(e) => {
                tracing::warn!(
                    "playground_2026_error_mappings_v1 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    if !done.contains("playground_2026_error_mappings_v2") {
        match crate::db::migrations::helpers::error_mappings::reset_playground_2026_error_mappings(pool).await {
            Ok(_) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_error_mappings_v2')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_error_mappings_v2".into());
            }
            Err(e) => {
                tracing::warn!(
                    "playground_2026_error_mappings_v2 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    // ── 火山素材转换规则、插件规则与级联视频规则：启用 fast_response 优先快返 ──
    once_migration!(pool, done, "forward_rules_enable_fast_response_for_asset_rules_v1",
        r#"UPDATE forward_rules
           SET config_json = (config_json::jsonb || '{"fast_response": true}'::jsonb)::text
           WHERE is_active = 1
             AND (
               config_json::jsonb ->> 'asset_convert' = 'true'
               OR config_json::jsonb ->> 'upstream_asset_convert' = 'true'
               OR config_json::jsonb ->> 'is_cascade' IN ('true', '1')
             )
             AND NOT (config_json::jsonb ? 'fast_response')"#
    );

    // ── Vidu 官方旗舰视频生成内置转发规则 ──
    once_migration!(pool, done, "system_preset_vidu_forward_rule_v1",
        r#"INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system, eid)
           SELECT 'Vidu 官方旗舰视频生成', 'vidu', '供 Vidu 官方 Q 系列视频模型（viduq4-preview / viduq3-pro 等）调用的专属转发规则，支持 OpenAI 与 Vidu 官方 2 种方式调用，支持文生视频、首帧图生视频、首尾帧生视频与多主体参考生视频自适应选路，支持音视频直出与异步任务轮询。', '{"target_type":"vidu","path_rewrite":{"old":"/v1/video/generations","new":"/ent/v2/img2video"},"auth_type":"token","poll_path":"/ent/v2/tasks/${task_id}/creations"}', '视频', 1, '1' || lpad((floor(random() * 10000)::int)::text, 4, '0')
           WHERE NOT EXISTS (SELECT 1 FROM forward_rules WHERE name = 'Vidu 官方旗舰视频生成')"#,
        r#"UPDATE forward_rules SET eid = CASE WHEN eid ~ '^[0-9]+$' THEN eid ELSE '1' || lpad((floor(random() * 10000)::int)::text, 4, '0') END, config_json = '{"target_type":"vidu","path_rewrite":{"old":"/v1/video/generations","new":"/ent/v2/img2video"},"auth_type":"token","poll_path":"/ent/v2/tasks/${task_id}/creations"}', description = '供 Vidu 官方 Q 系列视频模型（viduq4-preview / viduq3-pro 等）调用的专属转发规则，支持 OpenAI 与 Vidu 官方 2 种方式调用，支持文生视频、首帧图生视频、首尾帧生视频与多主体参考生视频自适应选路，支持音视频直出与异步任务轮询。' WHERE name = 'Vidu 官方旗舰视频生成'"#
    );

    once_migration!(
        pool,
        done,
        "redemption_starts_at_v1",
        "ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS starts_at TIMESTAMPTZ",
        "COMMENT ON COLUMN redemptions.starts_at IS '兑换码开始生效时间，NULL=生成后立即生效'"
    );
    once_migration!(
        pool,
        done,
        "mkt_member_order_no_v1",
        "ALTER TABLE mkt_member_orders ADD COLUMN IF NOT EXISTS order_no TEXT NOT NULL DEFAULT ''",
        "COMMENT ON COLUMN mkt_member_orders.order_no IS '卡券购买订单号，C 开头；历史单后缀 L，新单后缀 R'",
        r#"UPDATE mkt_member_orders
           SET order_no = 'C' || to_char(created_at AT TIME ZONE 'Asia/Shanghai', 'YYYYMMDDHH24MISS') || 'L' || lpad(id::text, 8, '0')
           WHERE order_no = ''"#,
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_mkt_orders_order_no ON mkt_member_orders (order_no) WHERE order_no <> ''",
        r#"INSERT INTO recharge_records (user_id, amount, recharge_type, remark, wallet_type, order_no, created_at)
           SELECT o.user_id, -o.price_paid, 'coupon_purchase', '购买卡券 ' || o.plan_name, 'system', o.order_no, o.created_at
           FROM mkt_member_orders o
           JOIN users u ON u.id = o.user_id
           WHERE o.price_paid > 0
             AND o.order_no <> ''
             AND NOT EXISTS (
               SELECT 1 FROM recharge_records rr
               WHERE rr.order_no = o.order_no AND rr.recharge_type = 'coupon_purchase'
             )"#
    );
    once_migration!(
        pool,
        done,
        "mkt_member_order_pay_status_v1",
        "ALTER TABLE mkt_member_orders ADD COLUMN IF NOT EXISTS pay_status TEXT NOT NULL DEFAULT 'paid'",
        "ALTER TABLE mkt_member_orders ADD COLUMN IF NOT EXISTS pay_method TEXT NOT NULL DEFAULT 'balance'",
        "ALTER TABLE mkt_member_orders ADD COLUMN IF NOT EXISTS paid_at TIMESTAMPTZ",
        "ALTER TABLE mkt_member_orders ADD COLUMN IF NOT EXISTS payment_no TEXT NOT NULL DEFAULT ''",
        "COMMENT ON COLUMN mkt_member_orders.pay_status IS 'unpaid=未付款, paid=已付款, closed=已关闭'",
        "COMMENT ON COLUMN mkt_member_orders.pay_method IS '余额为 balance，在线支付为渠道名；未付款为空'",
        "COMMENT ON COLUMN mkt_member_orders.payment_no IS '在线支付对应 orders.out_trade_no，余额支付为空'",
        "UPDATE mkt_member_orders SET paid_at = created_at WHERE pay_status = 'paid' AND paid_at IS NULL",
        "CREATE INDEX IF NOT EXISTS idx_mkt_orders_user_unpaid ON mkt_member_orders (user_id, plan_id) WHERE pay_status = 'unpaid'"
    );
    once_migration!(
        pool,
        done,
        "mkt_member_order_pay_expire_v1",
        "ALTER TABLE mkt_member_orders ADD COLUMN IF NOT EXISTS pay_expires_at TIMESTAMPTZ",
        "COMMENT ON COLUMN mkt_member_orders.pay_expires_at IS '未付款订单的支付截止时间，超时自动取消'",
        "UPDATE mkt_member_orders SET pay_expires_at = created_at + INTERVAL '24 hours' WHERE pay_status = 'unpaid' AND pay_expires_at IS NULL",
        "CREATE INDEX IF NOT EXISTS idx_mkt_orders_unpaid_expiry ON mkt_member_orders (pay_expires_at) WHERE pay_status = 'unpaid'"
    );
    once_migration!(
        pool,
        done,
        "mkt_member_order_balance_hold_v1",
        "ALTER TABLE mkt_member_orders ADD COLUMN IF NOT EXISTS balance_hold DOUBLE PRECISION NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN mkt_member_orders.balance_hold IS '组合支付时已从现金余额占用、待在线付清差额的金额；取消或超时退回'"
    );
    once_migration!(
        pool,
        done,
        "mkt_member_plan_window_hours_v1",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS limit_window_hours BIGINT NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN mkt_member_plans.limit_window_hours IS '限购窗口小时数；大于 0 时按小时，为 0 时仍用 limit_window_days'"
    );
    once_migration!(
        pool,
        done,
        "support_tickets_v1",
        r#"CREATE TABLE IF NOT EXISTS support_ticket_settings (
            id BIGINT PRIMARY KEY,
            auto_close_days BIGINT NOT NULL DEFAULT 7,
            max_open BIGINT NOT NULL DEFAULT 10,
            allow_images BIGINT NOT NULL DEFAULT 1,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "COMMENT ON COLUMN support_ticket_settings.auto_close_days IS '已解决超过这么多天后自动关闭，0=不自动关闭'",
        "COMMENT ON COLUMN support_ticket_settings.max_open IS '单用户 new/open/pending 工单上限'",
        "COMMENT ON COLUMN support_ticket_settings.allow_images IS '1=允许上传图片'",
        "INSERT INTO support_ticket_settings (id) VALUES (1) ON CONFLICT (id) DO NOTHING",
        r#"CREATE TABLE IF NOT EXISTS support_ticket_categories (
            id BIGSERIAL PRIMARY KEY,
            cat_key TEXT NOT NULL UNIQUE,
            label TEXT NOT NULL,
            sort_order BIGINT NOT NULL DEFAULT 0,
            is_enabled BIGINT NOT NULL DEFAULT 1,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        r#"INSERT INTO support_ticket_categories (cat_key, label, sort_order, is_enabled) VALUES
            ('account', '账户与登录', 10, 1),
            ('billing', '充值与账单', 20, 1),
            ('api', 'API 与调用', 30, 1),
            ('idea', '功能建议', 40, 1),
            ('other', '其他', 50, 1)
        ON CONFLICT (cat_key) DO NOTHING"#,
        r#"CREATE TABLE IF NOT EXISTS support_tickets (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            subject TEXT NOT NULL,
            category_key TEXT NOT NULL DEFAULT '',
            category_label TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'new',
            priority BIGINT NOT NULL DEFAULT 1,
            assignee_id TEXT,
            solved_at TIMESTAMPTZ,
            closed_at TIMESTAMPTZ,
            user_read_event_id BIGINT NOT NULL DEFAULT 0,
            admin_read_event_id BIGINT NOT NULL DEFAULT 0,
            last_public_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "COMMENT ON COLUMN support_tickets.status IS 'new/open/pending/solved/closed'",
        "COMMENT ON COLUMN support_tickets.priority IS '0低 1普通 2高 3紧急，仅管理员设置'",
        "COMMENT ON COLUMN support_tickets.user_read_event_id IS '用户已读到的事件 id，只比较管理员公开回复'",
        "CREATE INDEX IF NOT EXISTS idx_support_tickets_user_public ON support_tickets (user_id, last_public_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_support_tickets_status_updated ON support_tickets (status, updated_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_support_tickets_assignee ON support_tickets (assignee_id, status)",
        "CREATE INDEX IF NOT EXISTS idx_support_tickets_solved ON support_tickets (solved_at) WHERE status = 'solved'",
        r#"CREATE TABLE IF NOT EXISTS support_ticket_events (
            id BIGSERIAL PRIMARY KEY,
            ticket_id BIGINT NOT NULL,
            kind TEXT NOT NULL,
            actor_id TEXT NOT NULL DEFAULT '',
            actor_role TEXT NOT NULL DEFAULT '',
            actor_name TEXT NOT NULL DEFAULT '',
            body TEXT NOT NULL DEFAULT '',
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "COMMENT ON COLUMN support_ticket_events.kind IS 'public=双方可见, internal=仅管理员, system=状态与指派'",
        "CREATE INDEX IF NOT EXISTS idx_support_ticket_events_ticket ON support_ticket_events (ticket_id, id)",
        r#"CREATE TABLE IF NOT EXISTS support_ticket_files (
            id BIGSERIAL PRIMARY KEY,
            ticket_id BIGINT NOT NULL,
            event_id BIGINT NOT NULL,
            file_name TEXT NOT NULL,
            mime TEXT NOT NULL,
            size_bytes BIGINT NOT NULL,
            storage_name TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_support_ticket_files_event ON support_ticket_files (event_id)",
        "CREATE INDEX IF NOT EXISTS idx_support_ticket_files_ticket ON support_ticket_files (ticket_id)",
        r#"INSERT INTO plugins (
            name, title, description, is_enabled, allowed_levels, category,
            show_in_admin_menu, admin_menu_sort, admin_menu_title, admin_menu_default_tab
        ) VALUES (
            'support_tickets', '工单',
            '用户在顶栏提交工单，管理员在后台回复。公开回复与内部备注分开，解决后可以回访，超时自动关闭。',
            0, 'all', 'user',
            0, 60, '', 'inbox'
        ) ON CONFLICT (name) DO NOTHING"#
    );
    once_migration!(
        pool,
        done,
        "mkt_member_plan_space_v1",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS space_enabled BIGINT NOT NULL DEFAULT 0",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS space_mb BIGINT NOT NULL DEFAULT 0",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS space_duration_mode TEXT NOT NULL DEFAULT 'permanent'",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS space_duration_days BIGINT NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN mkt_member_plans.space_mb IS '购买后在创作中心2026基础上增加的存储空间，单位 MB'",
        "COMMENT ON COLUMN mkt_member_plans.space_duration_mode IS 'permanent=长期增加，days=固定天数后收回增加的部分'",
        r#"CREATE TABLE IF NOT EXISTS mkt_member_space_grants (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            order_id BIGINT NOT NULL,
            plan_id BIGINT NOT NULL,
            space_mb BIGINT NOT NULL DEFAULT 0,
            expires_at TIMESTAMPTZ,
            status TEXT NOT NULL DEFAULT 'active',
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "COMMENT ON TABLE mkt_member_space_grants IS '已付款卡券发放的创作中心2026空间。多次购买各记一行，配额按未过期行相加'",
        "CREATE INDEX IF NOT EXISTS idx_mkt_space_user_active ON mkt_member_space_grants (user_id) WHERE status = 'active'"
    );
    once_migration!(
        pool,
        done,
        "mkt_member_plan_duration_hours_v1",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS tier_duration_hours BIGINT NOT NULL DEFAULT 0",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS credit_duration_hours BIGINT NOT NULL DEFAULT 0",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS space_duration_hours BIGINT NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN mkt_member_plans.tier_duration_hours IS '会员等级有效小时。mode=hours 时使用，此时 tier_duration_days 为 0'",
        "COMMENT ON COLUMN mkt_member_plans.credit_duration_hours IS '消费额度有效小时。mode=hours 时使用，此时 credit_duration_days 为 0'",
        "COMMENT ON COLUMN mkt_member_plans.space_duration_hours IS '创作空间有效小时。mode=hours 时使用，此时 space_duration_days 为 0'"
    );
    once_migration!(
        pool,
        done,
        "mkt_campaigns_v1",
        r#"CREATE TABLE IF NOT EXISTS mkt_campaigns (
            id BIGSERIAL PRIMARY KEY,
            name TEXT NOT NULL DEFAULT '',
            headline TEXT NOT NULL DEFAULT '',
            subtitle TEXT NOT NULL DEFAULT '',
            is_enabled BIGINT NOT NULL DEFAULT 1,
            template_key TEXT NOT NULL DEFAULT 'cards',
            placement TEXT NOT NULL DEFAULT 'playground_2026_hot',
            plan_ids TEXT NOT NULL DEFAULT '[]',
            sort_order BIGINT NOT NULL DEFAULT 0,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "COMMENT ON TABLE mkt_campaigns IS '营销卡券活动。引用已有会员方案，投放到指定页面入口'",
        "COMMENT ON COLUMN mkt_campaigns.template_key IS '展示模板。cards=卡片列表'",
        "COMMENT ON COLUMN mkt_campaigns.placement IS '展示位置。playground_2026_hot=创作中心2026顶部热门活动'",
        "COMMENT ON COLUMN mkt_campaigns.plan_ids IS '参与活动的会员方案 id，JSON 数组，顺序即展示顺序'",
        "CREATE INDEX IF NOT EXISTS idx_mkt_campaigns_placement ON mkt_campaigns (placement, is_enabled, sort_order, id)"
    );
    once_migration!(
        pool,
        done,
        "mkt_campaigns_h_site_template_v1",
        "COMMENT ON COLUMN mkt_campaigns.template_key IS '展示模板。cards=卡片列表，h_site=H站风格'"
    );
    once_migration!(
        pool,
        done,
        "mkt_campaigns_i_site_template_v1",
        "COMMENT ON COLUMN mkt_campaigns.template_key IS '展示模板。cards=卡片列表，h_site=H站风格，i_site=I站风格'"
    );
    once_migration!(
        pool,
        done,
        "commission_lots_v1",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS commissioned_recharge_left DOUBLE PRECISION NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN users.commissioned_recharge_left IS '已发过充值返佣、尚未被系统钱包支出吃掉的本金'",
        "ALTER TABLE recharge_records ADD COLUMN IF NOT EXISTS principal_exempt DOUBLE PRECISION NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN recharge_records.principal_exempt IS '这笔系统钱包支出吃掉的已返充值本金，退款时加回'",
        r#"CREATE TABLE IF NOT EXISTS commission_lots (
            id BIGSERIAL PRIMARY KEY,
            source_key TEXT NOT NULL,
            inviter_id TEXT NOT NULL,
            from_user_id TEXT NOT NULL,
            base_amount DOUBLE PRECISION NOT NULL DEFAULT 0,
            awarded DOUBLE PRECISION NOT NULL DEFAULT 0,
            clawed DOUBLE PRECISION NOT NULL DEFAULT 0,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "COMMENT ON TABLE commission_lots IS '每笔已入账佣金。回冲按这里的邀请人和金额，不看当前等级'",
        "CREATE UNIQUE INDEX IF NOT EXISTS uidx_commission_lots_source ON commission_lots (source_key)",
        "CREATE INDEX IF NOT EXISTS idx_commission_lots_from ON commission_lots (from_user_id, id DESC)",
        r#"INSERT INTO commission_lots (source_key, inviter_id, from_user_id, base_amount, awarded, clawed)
           SELECT c.source_key, c.user_id, c.from_user_id,
                  CASE WHEN c.ratio > 0 THEN c.amount / c.ratio ELSE c.amount END,
                  c.amount, 0
           FROM commissions c
           WHERE c.amount > 0 AND c.source_key IS NOT NULL AND c.source_key <> ''
           ON CONFLICT (source_key) DO NOTHING"#,
        r#"UPDATE users u SET commissioned_recharge_left = GREATEST(0, LEAST(
               GREATEST(u.balance, 0),
               COALESCE((
                   SELECT SUM(rr.amount) FROM recharge_records rr
                   JOIN commissions c ON c.source_key = 'recharge:' || rr.id::text AND c.amount > 0
                   WHERE rr.user_id = u.id AND rr.amount > 0
               ), 0)
               - COALESCE((
                   SELECT SUM(GREATEST(-rr.amount, 0)) FROM recharge_records rr
                   WHERE rr.user_id = u.id AND COALESCE(rr.wallet_type, 'system') = 'system' AND rr.amount < 0
                     AND rr.recharge_type IN ('coupon_purchase', 'manual', 'ark_video_consume')
               ), 0)
               - COALESCE((
                   SELECT SUM(GREATEST(l.cost - COALESCE(l.pre_deduct_gift, 0) - COALESCE(l.coupon_deduct, 0), 0))
                   FROM logs l
                   WHERE l.user_id = u.id AND l.status_code = 200 AND l.cost > 0
               ), 0)
           ))
           WHERE EXISTS (
               SELECT 1 FROM commissions c
               JOIN recharge_records rr ON c.source_key = 'recharge:' || rr.id::text
               WHERE rr.user_id = u.id AND c.amount > 0
           )"#
    );
    once_migration!(
        pool,
        done,
        "commission_refund_split_v1",
        "ALTER TABLE recharge_records ADD COLUMN IF NOT EXISTS real_cash DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE recharge_records ADD COLUMN IF NOT EXISTS system_refunded DOUBLE PRECISION NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN recharge_records.real_cash IS '方舟系统扣款里扣之前余额盖住的现金，信控垫付不在这里'",
        "COMMENT ON COLUMN recharge_records.system_refunded IS '这笔方舟系统扣款已经退回的金额，含信控垫付'",
        "ALTER TABLE mkt_member_orders ADD COLUMN IF NOT EXISTS principal_hold DOUBLE PRECISION NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN mkt_member_orders.principal_hold IS '占用余额时吃掉的已返充值本金，释放占用时只加回这个数'"
    );
    once_migration!(
        pool,
        done,
        "redemption_commission_exempt_v1",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS redemption_commission_exempt DOUBLE PRECISION NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN users.redemption_commission_exempt IS '余额兑换码入账后尚未花掉的金额。这段系统余额不参与充值返佣和消费返佣'",
        "ALTER TABLE recharge_records ADD COLUMN IF NOT EXISTS redemption_exempt DOUBLE PRECISION NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN recharge_records.redemption_exempt IS '这笔支出吃掉的余额兑换码本金，退款时加回'",
        "ALTER TABLE mkt_member_orders ADD COLUMN IF NOT EXISTS redemption_hold DOUBLE PRECISION NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN mkt_member_orders.redemption_hold IS '占用余额时吃掉的余额兑换码本金，释放占用时加回'",
        r#"UPDATE users u SET redemption_commission_exempt = GREATEST(0, LEAST(
               GREATEST(u.balance, 0),
               COALESCE((
                   SELECT SUM(rr.amount) FROM recharge_records rr
                   WHERE rr.user_id = u.id AND rr.recharge_type = 'redemption' AND rr.amount > 0
               ), 0)
               - COALESCE((
                   SELECT SUM(-rr.amount) FROM recharge_records rr
                   WHERE rr.user_id = u.id AND rr.recharge_type = 'redemption_refund' AND rr.amount < 0
               ), 0)
           ))
           WHERE EXISTS (
               SELECT 1 FROM recharge_records rr
               WHERE rr.user_id = u.id AND rr.recharge_type = 'redemption' AND rr.amount > 0
           )"#
    );

    // ── Google Gemini 转发规则收敛：合二为一，仅保留 1 条图片和 1 条聊天，淘汰多余旧规则并平滑迁移关联模型 ──
    let mig_key = "system_preset_google_forward_rules_consolidation_v1";
    if !done.contains(mig_key) {
        let mut tx = pool.begin().await?;

        // 1. 确保主图片规则与主聊天规则存在且配置最新（优先复用已有 ID，避免破坏现有绑定）
        let rules_def = [
            ("Google Gemini 原生生图", "图片",
             r#"{"target_type":"gemini_image","path_rewrites":[{"old":"/v1/images/generations","new":"/v1beta/models/${model}:generateContent"},{"old":"/v1/models/${model}:generateContent","new":"/v1/models/${model}:generateContent"},{"old":"/v1beta/models/${model}:generateContent","new":"/v1beta/models/${model}:generateContent"},{"old":"/v1beta/interactions","new":"/v1beta/interactions"},{"old":"/v1/interactions","new":"/v1/interactions"}],"auth_type":"x-goog-api-key"}"#,
             "标准生图重定向到 Gemini contents 接口，支持 OpenAI、新官方 /v1/、老官方 /v1beta/ 与 Interactions 多套路由自适应分发"),
            ("Google Gemini 格式转换 (聊天)", "聊天",
             r#"{"target_type":"gemini","path_rewrites":[{"old":"/v1/chat/completions","new":"/v1beta/models/${model}:generateContent"},{"old":"/v1/responses","new":"/v1beta/models/${model}:generateContent"},{"old":"/v1/models/${model}:generateContent","new":"/v1/models/${model}:generateContent"},{"old":"/v1/models/${model}:streamGenerateContent","new":"/v1/models/${model}:streamGenerateContent"},{"old":"/v1beta/models/${model}:generateContent","new":"/v1beta/models/${model}:generateContent"},{"old":"/v1beta/models/${model}:streamGenerateContent","new":"/v1beta/models/${model}:streamGenerateContent"},{"old":"/v1beta/interactions","new":"/v1beta/interactions"},{"old":"/v1/interactions","new":"/v1/interactions"}],"auth_type":"x-goog-api-key"}"#,
             "标准对话重定向到 Gemini contents，支持 OpenAI、Responses、新官方 /v1/、老官方 /v1beta/ 与 Interactions 流式/非流式多套路由"),
        ];

        let mut target_ids = Vec::with_capacity(2);
        for (name, category, config, desc) in rules_def {
            let existing_id: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM forward_rules WHERE (name = $1 OR (name = 'Google Gemini 流式转换 (聊天)' AND $1 = 'Google Gemini 格式转换 (聊天)')) AND category = $2 LIMIT 1"
            )
            .bind(name).bind(category).fetch_optional(&mut *tx).await?;

            let id = match existing_id {
                Some(id) => {
                    sqlx::query("UPDATE forward_rules SET name = $1, config_json = $2, description = $3 WHERE id = $4")
                        .bind(name).bind(config).bind(desc).bind(id).execute(&mut *tx).await?;
                    id
                }
                None => {
                    sqlx::query_scalar(
                        r#"INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system, eid)
                           VALUES ($1, 'gemini', $2, $3, $4, 1, '1' || lpad((floor(random() * 10000)::int)::text, 4, '0'))
                           RETURNING id"#
                    )
                    .bind(name).bind(desc).bind(config).bind(category).fetch_one(&mut *tx).await?
                }
            };
            target_ids.push(id);
        }
        let (target_img_id, target_chat_id) = (target_ids[0], target_ids[1]);

        // 2. 查出需淘汰的旧系统预设 Google 图片/聊天规则 ID (带 is_system = 1 防御，绝不误删用户自定义规则)
        let old_img_ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM forward_rules WHERE id != $1 AND category = '图片' AND is_system = 1 AND (rule_type IN ('gemini','google') OR name ILIKE '%gemini%' OR name ILIKE '%google%' OR config_json ILIKE '%gemini%')"
        ).bind(target_img_id).fetch_all(&mut *tx).await.unwrap_or_default();

        let old_chat_ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM forward_rules WHERE id != $1 AND category = '聊天' AND is_system = 1 AND (rule_type IN ('gemini','google') OR name ILIKE '%gemini%' OR name ILIKE '%google%' OR config_json ILIKE '%gemini%')"
        ).bind(target_chat_id).fetch_all(&mut *tx).await.unwrap_or_default();

        // 3. 关联模型平滑迁移：替换旧规则 ID 并去重更新
        if !old_img_ids.is_empty() || !old_chat_ids.is_empty() {
            let models: Vec<(i64, Option<String>)> = sqlx::query_as(
                "SELECT id, forward_rule_ids FROM models WHERE forward_rule_ids IS NOT NULL AND forward_rule_ids != '[]'"
            ).fetch_all(&mut *tx).await.unwrap_or_default();

            let mut update_models: Vec<(i64, String)> = Vec::new();
            for (id, ids_json) in models {
                if let Ok(mut ids) = serde_json::from_str::<Vec<i64>>(&ids_json.unwrap_or_default()) {
                    let mut changed = false;
                    for r_id in &mut ids {
                        if old_img_ids.contains(r_id) { *r_id = target_img_id; changed = true; }
                        else if old_chat_ids.contains(r_id) { *r_id = target_chat_id; changed = true; }
                    }
                    if changed {
                        let mut seen = HashSet::new();
                        ids.retain(|x| seen.insert(*x));
                        update_models.push((id, serde_json::to_string(&ids).unwrap_or_default()));
                    }
                }
            }

            if !update_models.is_empty() {
                let (pks, new_jsons): (Vec<i64>, Vec<String>) = update_models.into_iter().unzip();
                sqlx::query(
                    r#"UPDATE models AS m SET forward_rule_ids = u.new_ids
                       FROM unnest($1::bigint[], $2::text[]) AS u(id, new_ids) WHERE m.id = u.id"#
                )
                .bind(&pks).bind(&new_jsons).execute(&mut *tx).await?;
            }

            // 4. 批量删除被淘汰的旧规则
            let mut to_delete = old_img_ids;
            to_delete.extend(old_chat_ids);
            sqlx::query("DELETE FROM forward_rules WHERE id = ANY($1::bigint[])")
                .bind(&to_delete).execute(&mut *tx).await?;
        }

        // 5. 记录完成并提交事务
        sqlx::query("INSERT INTO sys_migration_history (id) VALUES ($1) ON CONFLICT (id) DO NOTHING")
            .bind(mig_key).execute(&mut *tx).await?;
        tx.commit().await?;
        done.insert(mig_key.to_string());
    }

    // ── 清理老旧可灵视频转发规则，自动重定向已绑定的模型与模型库并安全下线旧规则 ──
    once_migration!(
        pool,
        done,
        "cleanup_legacy_kling_video_forward_rules_v1",
        // 1. 将绑定了老规则「可灵 视频生成 (文/图/多图)」的模型重定向至「可灵视频 3.0（文/图·推荐）」
        r#"UPDATE models m
        SET forward_rule_ids = (
            SELECT json_agg(DISTINCT CASE WHEN elem::text::bigint = del.id THEN n.id ELSE elem::text::bigint END)::text
            FROM jsonb_array_elements_text(m.forward_rule_ids::jsonb) AS elem
        ), updated_at = CURRENT_TIMESTAMP
        FROM (SELECT id FROM forward_rules WHERE name = '可灵视频 3.0（文/图·推荐）' LIMIT 1) n,
             (SELECT id FROM forward_rules WHERE name = '可灵 视频生成 (文/图/多图)' LIMIT 1) del
        WHERE m.forward_rule_ids IS NOT NULL
          AND m.forward_rule_ids LIKE '[%]'
          AND m.forward_rule_ids::jsonb @> to_jsonb(del.id)"#,
        // 2. 将绑定了老规则「可灵 Omni 视频 (kling-v3-omni/video-o1)」的模型重定向至「可灵 Omni 视频 3.0（推荐）」
        r#"UPDATE models m
        SET forward_rule_ids = (
            SELECT json_agg(DISTINCT CASE WHEN elem::text::bigint = del.id THEN n.id ELSE elem::text::bigint END)::text
            FROM jsonb_array_elements_text(m.forward_rule_ids::jsonb) AS elem
        ), updated_at = CURRENT_TIMESTAMP
        FROM (SELECT id FROM forward_rules WHERE name = '可灵 Omni 视频 3.0（推荐）' LIMIT 1) n,
             (SELECT id FROM forward_rules WHERE name = '可灵 Omni 视频 (kling-v3-omni/video-o1)' LIMIT 1) del
        WHERE m.forward_rule_ids IS NOT NULL
          AND m.forward_rule_ids LIKE '[%]'
          AND m.forward_rule_ids::jsonb @> to_jsonb(del.id)"#,
        // 3. 将 model_library 模型库同理重定向
        r#"UPDATE model_library ml
        SET forward_rule_ids = (
            SELECT json_agg(DISTINCT CASE WHEN elem::text::bigint = del.id THEN n.id ELSE elem::text::bigint END)::text
            FROM jsonb_array_elements_text(ml.forward_rule_ids::jsonb) AS elem
        ), updated_at = CURRENT_TIMESTAMP
        FROM (SELECT id FROM forward_rules WHERE name = '可灵视频 3.0（文/图·推荐）' LIMIT 1) n,
             (SELECT id FROM forward_rules WHERE name = '可灵 视频生成 (文/图/多图)' LIMIT 1) del
        WHERE ml.forward_rule_ids IS NOT NULL
          AND ml.forward_rule_ids LIKE '[%]'
          AND ml.forward_rule_ids::jsonb @> to_jsonb(del.id)"#,
        r#"UPDATE model_library ml
        SET forward_rule_ids = (
            SELECT json_agg(DISTINCT CASE WHEN elem::text::bigint = del.id THEN n.id ELSE elem::text::bigint END)::text
            FROM jsonb_array_elements_text(ml.forward_rule_ids::jsonb) AS elem
        ), updated_at = CURRENT_TIMESTAMP
        FROM (SELECT id FROM forward_rules WHERE name = '可灵 Omni 视频 3.0（推荐）' LIMIT 1) n,
             (SELECT id FROM forward_rules WHERE name = '可灵 Omni 视频 (kling-v3-omni/video-o1)' LIMIT 1) del
        WHERE ml.forward_rule_ids IS NOT NULL
          AND ml.forward_rule_ids LIKE '[%]'
          AND ml.forward_rule_ids::jsonb @> to_jsonb(del.id)"#,
        // 4. 彻底删除老旧可灵视频转发规则
        r#"DELETE FROM forward_rules
        WHERE name IN ('可灵 视频生成 (文/图/多图)', '可灵 Omni 视频 (kling-v3-omni/video-o1)')
           OR (category = '视频' AND (config_json::jsonb->>'target_type') = 'kling' AND (config_json::jsonb #>> '{path_rewrite,new}') LIKE '%/v1/videos/%')"#,
        // 5. 将可灵视频推荐规则统一归类：rule_type 设为 kling（与可灵图片规则归在一起便于管理），target_type 保持为 kling_video 协议
        r#"UPDATE forward_rules
        SET rule_type = 'kling',
            config_json = replace(config_json, '"target_type":"kling"', '"target_type":"kling_video"')
        WHERE name IN ('可灵视频 3.0（文/图·推荐）', '可灵 Omni 视频 3.0（推荐）')
           OR rule_type = 'kling_video'"#
    );

    if !done.contains("playground_2026_grok_4_6_chat_scheme_v1") {
        match crate::db::migrations::helpers::grok_4_6_chat::seed_playground_2026_grok_4_6_chat_scheme(pool).await {
            Ok(_) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_grok_4_6_chat_scheme_v1')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_grok_4_6_chat_scheme_v1".into());
            }
            Err(e) => {
                tracing::warn!(
                    "playground_2026_grok_4_6_chat_scheme_v1 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    if !done.contains("playground_2026_drop_generic_chat_schemes_v1") {
        match crate::db::migrations::helpers::glm_deepseek_chat::drop_playground_2026_generic_chat_schemes(pool).await {
            Ok(_) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_drop_generic_chat_schemes_v1')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_drop_generic_chat_schemes_v1".into());
            }
            Err(e) => {
                tracing::warn!(
                    "playground_2026_drop_generic_chat_schemes_v1 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    // ── 腾讯云 VOD Vidu 视频生成专属转发规则（支持 OpenAI 与 Vidu 官方路由） ──
    once_migration!(
        pool,
        done,
        "tencent_vod_vidu_video_forward_rule_v1",
        r#"INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system, eid)
        SELECT '腾讯云 VOD Vidu 视频生成', 'tencent', '将 OpenAI 与 Vidu 官方视频请求转换为腾讯云点播 AIGC CreateAigcVideoTask 接口。密钥格式：SecretId:SecretKey:SubAppId，模型格式：ModelName@ModelVersion', '{"target_type":"tencent_vod_video","path_rewrite":{"old":"/v1/video/generations","new":"/"},"path_accept":["/ent/v2/text2video","/ent/v2/img2video","/ent/v2/start-end2video","/ent/v2/reference2video"],"auth_type":"tencent_vod"}', '视频', 1, '1' || lpad((floor(random() * 10000)::int)::text, 4, '0')
        WHERE NOT EXISTS (SELECT 1 FROM forward_rules WHERE name = '腾讯云 VOD Vidu 视频生成')"#,
        r#"UPDATE forward_rules
        SET eid = CASE WHEN eid ~ '^[0-9]+$' THEN eid ELSE '1' || lpad((floor(random() * 10000)::int)::text, 4, '0') END,
            config_json = '{"target_type":"tencent_vod_video","path_rewrite":{"old":"/v1/video/generations","new":"/"},"path_accept":["/ent/v2/text2video","/ent/v2/img2video","/ent/v2/start-end2video","/ent/v2/reference2video"],"auth_type":"tencent_vod"}',
            description = '将 OpenAI 与 Vidu 官方视频请求转换为腾讯云点播 AIGC CreateAigcVideoTask 接口。密钥格式：SecretId:SecretKey:SubAppId，模型格式：ModelName@ModelVersion'
        WHERE name = '腾讯云 VOD Vidu 视频生成'"#
    );
    once_migration!(
        pool,
        done,
        "mkt_offer_kind_v1",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS offer_kind TEXT NOT NULL DEFAULT 'pack'",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS billing_cycle TEXT NOT NULL DEFAULT ''",
        "COMMENT ON COLUMN mkt_member_plans.offer_kind IS 'pack=一次性加油包，subscription=按周期可续费订阅'",
        "COMMENT ON COLUMN mkt_member_plans.billing_cycle IS '订阅周期。month=30天，quarter=90天，year=365天。加油包为空'",
        "ALTER TABLE mkt_campaigns ADD COLUMN IF NOT EXISTS offer_kind TEXT NOT NULL DEFAULT 'pack'",
        "COMMENT ON COLUMN mkt_campaigns.offer_kind IS '活动类型，必须和引用的方案一致。pack=一次性加油包，subscription=订阅'",
        r#"CREATE TABLE IF NOT EXISTS mkt_member_subscriptions (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            plan_id BIGINT NOT NULL,
            billing_cycle TEXT NOT NULL DEFAULT 'month',
            expires_at TIMESTAMPTZ,
            last_order_id BIGINT NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'active',
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "COMMENT ON TABLE mkt_member_subscriptions IS '订阅到期锚点。续费从这里未过期的到期时间往后加一个周期'",
        "CREATE UNIQUE INDEX IF NOT EXISTS uidx_mkt_sub_user_plan ON mkt_member_subscriptions (user_id, plan_id)"
    );

    once_migration!(
        pool,
        done,
        "mkt_credit_grant_mode_v1",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS credit_grant_mode TEXT NOT NULL DEFAULT 'per_cycle'",
        "COMMENT ON COLUMN mkt_member_plans.credit_grant_mode IS '订阅消费额度。per_cycle=每期发放，once=新开通时发放一次。加油包忽略'"
    );

    once_migration!(
        pool,
        done,
        "mkt_cycle_prices_v1",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS price_month DOUBLE PRECISION",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS price_quarter DOUBLE PRECISION",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS price_year DOUBLE PRECISION",
        "COMMENT ON COLUMN mkt_member_plans.price_month IS '订阅按月价格。空表示不卖月付。0 表示这个周期免费'",
        "COMMENT ON COLUMN mkt_member_plans.price_quarter IS '订阅按季价格。空表示不卖季付'",
        "COMMENT ON COLUMN mkt_member_plans.price_year IS '订阅按年价格。空表示不卖年付'",
        "UPDATE mkt_member_plans SET price_month = price WHERE offer_kind = 'subscription' AND billing_cycle = 'month' AND price_month IS NULL",
        "UPDATE mkt_member_plans SET price_quarter = price WHERE offer_kind = 'subscription' AND billing_cycle = 'quarter' AND price_quarter IS NULL",
        "UPDATE mkt_member_plans SET price_year = price WHERE offer_kind = 'subscription' AND billing_cycle = 'year' AND price_year IS NULL",
        "UPDATE mkt_member_plans SET price_month = price WHERE offer_kind = 'subscription' AND price_month IS NULL AND price_quarter IS NULL AND price_year IS NULL",
        "ALTER TABLE mkt_member_orders ADD COLUMN IF NOT EXISTS billing_cycle TEXT NOT NULL DEFAULT ''",
        "COMMENT ON COLUMN mkt_member_orders.billing_cycle IS '这一笔购买的周期。month/quarter/year。加油包为空'",
        "UPDATE mkt_member_orders o SET billing_cycle = p.billing_cycle FROM mkt_member_plans p WHERE o.plan_id = p.id AND o.billing_cycle = '' AND p.offer_kind = 'subscription' AND p.billing_cycle IN ('month', 'quarter', 'year')"
    );

    once_migration!(
        pool,
        done,
        "user_level_purchasable_v1",
        "ALTER TABLE user_levels ADD COLUMN IF NOT EXISTS purchasable BIGINT NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN user_levels.purchasable IS '1=可以在会员卡券方案里被选为升级目标。0=方案里选不到'"
    );

    once_migration!(
        pool,
        done,
        "mkt_list_price_v1",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS list_price DOUBLE PRECISION",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS list_price_month DOUBLE PRECISION",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS list_price_quarter DOUBLE PRECISION",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS list_price_year DOUBLE PRECISION",
        "COMMENT ON COLUMN mkt_member_plans.list_price IS '加油包原价。空表示不展示划线价。扣款用 price'",
        "COMMENT ON COLUMN mkt_member_plans.list_price_month IS '订阅按月原价。空表示这个周期不展示划线价。扣款用 price_month'",
        "COMMENT ON COLUMN mkt_member_plans.list_price_quarter IS '订阅按季原价。扣款用 price_quarter'",
        "COMMENT ON COLUMN mkt_member_plans.list_price_year IS '订阅按年原价。扣款用 price_year'",
        "ALTER TABLE mkt_member_orders ADD COLUMN IF NOT EXISTS list_price DOUBLE PRECISION",
        "COMMENT ON COLUMN mkt_member_orders.list_price IS '下单时的原价，只用于展示。扣款用 price_paid'"
    );

    once_migration!(
        pool,
        done,
        "mkt_plan_placements_v1",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS placements_json TEXT NOT NULL DEFAULT '[]'",
        "COMMENT ON COLUMN mkt_member_plans.placements_json IS '方案勾选的用户前台位置。playground_2026_hot=创作中心2026顶部热门活动'",
        "UPDATE mkt_member_plans p SET placements_json = '[\"playground_2026_hot\"]' WHERE placements_json = '[]' AND EXISTS (SELECT 1 FROM mkt_campaigns c WHERE c.placement = 'playground_2026_hot' AND c.plan_ids::jsonb @> to_jsonb(p.id))"
    );

    once_migration!(
        pool,
        done,
        "mkt_renew_price_v1",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS renew_price_month DOUBLE PRECISION",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS renew_price_quarter DOUBLE PRECISION",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS renew_price_year DOUBLE PRECISION",
        "COMMENT ON COLUMN mkt_member_plans.renew_price_month IS '订阅未过期时的按月续费价。空表示按 price_month。0 表示这个周期续费免费'",
        "COMMENT ON COLUMN mkt_member_plans.renew_price_quarter IS '订阅未过期时的按季续费价。空表示按 price_quarter'",
        "COMMENT ON COLUMN mkt_member_plans.renew_price_year IS '订阅未过期时的按年续费价。空表示按 price_year'"
    );

    once_migration!(
        pool,
        done,
        "mkt_campaign_plan_select_v1",
        "ALTER TABLE mkt_campaigns ADD COLUMN IF NOT EXISTS plan_select_mode TEXT NOT NULL DEFAULT 'manual'",
        "COMMENT ON COLUMN mkt_campaigns.plan_select_mode IS 'all_enabled=所有启用中的会员方案都参与。manual=只展示 plan_ids 里选中的方案。已有活动保持 manual'"
    );

    once_migration!(
        pool,
        done,
        "mkt_campaign_placements_v1",
        "ALTER TABLE mkt_campaigns ADD COLUMN IF NOT EXISTS placements_json TEXT NOT NULL DEFAULT '[]'",
        "COMMENT ON COLUMN mkt_campaigns.placements_json IS '活动展示位置，可多选。playground_2026_hot=创作中心2026热门活动，console_nav_coupon=控制台顶部卡券订阅'",
        "UPDATE mkt_campaigns SET placements_json = json_build_array(placement)::text WHERE placements_json = '[]' AND placement <> ''"
    );

    once_migration!(
        pool,
        done,
        "mkt_member_gift_space_v1",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS gift_space_mb BIGINT NOT NULL DEFAULT 0",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS gift_space_duration_mode TEXT NOT NULL DEFAULT 'permanent'",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS gift_space_duration_days BIGINT NOT NULL DEFAULT 0",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS gift_space_duration_hours BIGINT NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN mkt_member_plans.gift_space_mb IS '赠送的创作中心2026空间，单位 MB。0 表示不赠送'",
        "COMMENT ON COLUMN mkt_member_plans.gift_space_duration_mode IS '赠送空间有效期。permanent=长期，days=按天，hours=按小时'",
        "ALTER TABLE mkt_member_space_grants ADD COLUMN IF NOT EXISTS kind TEXT NOT NULL DEFAULT 'space'",
        "COMMENT ON COLUMN mkt_member_space_grants.kind IS 'space=扩容空间，gift=赠送扩容空间。订阅续费只改 space'"
    );

    once_migration!(
        pool,
        done,
        "mkt_coupon_fund_type_v1",
        r#"UPDATE recharge_records AS rr
           SET recharge_type = CASE
               WHEN o.billing_cycle IN ('month', 'quarter', 'year') OR p.offer_kind = 'subscription' THEN 'coupon_subscription'
               ELSE 'coupon_pack'
           END
           FROM mkt_member_orders AS o
           LEFT JOIN mkt_member_plans AS p ON p.id = o.plan_id
           WHERE rr.recharge_type = 'coupon_purchase'
             AND rr.order_no <> ''
             AND rr.order_no = o.order_no"#
    );

    once_migration!(
        pool,
        done,
        "channel_configs_category_ids_v1",
        "ALTER TABLE channel_configs ADD COLUMN IF NOT EXISTS category_ids TEXT NOT NULL DEFAULT '[]'",
        "COMMENT ON COLUMN channel_configs.category_ids IS '上游分类ID列表(JSON数组字符串，支持多选)'",
        "UPDATE channel_configs SET category_ids = json_build_array(category_id)::text WHERE (category_ids = '[]' OR category_ids IS NULL) AND category_id IS NOT NULL"
    );

    once_migration!(
        pool,
        done,
        "mkt_plan_offer_snapshot_v1",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS content_rev BIGINT NOT NULL DEFAULT 1",
        "COMMENT ON COLUMN mkt_member_plans.content_rev IS '售价或权益变化时加一。停用、排序、角标不加'",
        "ALTER TABLE mkt_member_orders ADD COLUMN IF NOT EXISTS benefit_snapshot TEXT NOT NULL DEFAULT ''",
        "COMMENT ON COLUMN mkt_member_orders.benefit_snapshot IS '下单时的权益快照。付款按这份发放，不跟后来改过的方案'",
        "ALTER TABLE mkt_member_space_grants ADD COLUMN IF NOT EXISTS next_space_mb BIGINT",
        "ALTER TABLE mkt_member_space_grants ADD COLUMN IF NOT EXISTS next_space_at TIMESTAMPTZ",
        "ALTER TABLE mkt_member_space_grants ADD COLUMN IF NOT EXISTS held_order_id BIGINT",
        "COMMENT ON COLUMN mkt_member_space_grants.next_space_mb IS '续费后要换成的空间 MB。到 next_space_at 才生效，已付周期仍用 space_mb'",
        "COMMENT ON COLUMN mkt_member_space_grants.next_space_at IS '新空间开始生效的时间，等于续费前的订阅到期时间'",
        "COMMENT ON COLUMN mkt_member_space_grants.held_order_id IS '当前 space_mb 是哪一笔订单付的。作废续费订单时把空间退回到这一期'",
        "ALTER TABLE mkt_member_credits ADD COLUMN IF NOT EXISTS starts_at TIMESTAMPTZ",
        "COMMENT ON COLUMN mkt_member_credits.starts_at IS '下一期额度的开始时间。到点前 status=scheduled，不计入当前周期'",
        "ALTER TABLE mkt_member_orders ADD COLUMN IF NOT EXISTS tier_starts_at TIMESTAMPTZ",
        "COMMENT ON COLUMN mkt_member_orders.tier_starts_at IS '下一期会员等级的开始时间。到点才改用户等级，当前周期的等级保持不变'"
    );

    once_migration!(
        pool,
        done,
        "redemption_logs_refunded_v1",
        "ALTER TABLE redemption_logs ADD COLUMN IF NOT EXISTS refunded BIGINT NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN redemption_logs.refunded IS '0=这一次兑换尚未退换，1=已退换这一次。整码退换时一并标成 1'",
        "UPDATE redemption_logs AS l SET refunded = 1 FROM redemptions AS r WHERE l.redemption_id = r.id AND r.status = -2 AND l.refunded = 0"
    );

    if !done.contains("playground_2026_gemini_nano_banana_21_scheme_v1") {
        match crate::db::migrations::helpers::gemini_nano_banana_21::seed_playground_2026_gemini_nano_banana_21_scheme(pool).await {
            Ok(_) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_gemini_nano_banana_21_scheme_v1')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_gemini_nano_banana_21_scheme_v1".into());
            }
            Err(e) => {
                tracing::warn!(
                    "playground_2026_gemini_nano_banana_21_scheme_v1 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    // v1 已在像素尺寸版本上记过完成。这里再跑一遍，把仍是 WxH 的尺寸表改成 1K/2K/4K。
    if !done.contains("playground_2026_gemini_nano_banana_21_scheme_v2") {
        match crate::db::migrations::helpers::gemini_nano_banana_21::seed_playground_2026_gemini_nano_banana_21_scheme(pool).await {
            Ok(_) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_gemini_nano_banana_21_scheme_v2')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_gemini_nano_banana_21_scheme_v2".into());
            }
            Err(e) => {
                tracing::warn!(
                    "playground_2026_gemini_nano_banana_21_scheme_v2 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    if !done.contains("playground_2026_error_mappings_v3") {
        match crate::db::migrations::helpers::error_mappings::ensure_playground_2026_error_mapping_codes(
            pool,
            &["need_image"],
        )
        .await
        {
            Ok(_) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_error_mappings_v3')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_error_mappings_v3".into());
            }
            Err(e) => {
                tracing::warn!(
                    "playground_2026_error_mappings_v3 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    if !done.contains("playground_2026_error_mappings_v4") {
        match crate::db::migrations::helpers::error_mappings::sync_playground_2026_error_mapping_copy(
            pool,
            &["need_image"],
        )
        .await
        {
            Ok(_) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_error_mappings_v4')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_error_mappings_v4".into());
            }
            Err(e) => {
                tracing::warn!(
                    "playground_2026_error_mappings_v4 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    if !done.contains("playground_2026_viduq4_preview_scheme_v1") {
        match crate::db::migrations::helpers::viduq4_preview::seed_playground_2026_viduq4_preview_scheme(pool).await {
            Ok(_) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_viduq4_preview_scheme_v1')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_viduq4_preview_scheme_v1".into());
            }
            Err(e) => {
                tracing::warn!(
                    "playground_2026_viduq4_preview_scheme_v1 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    // v1 写入时分辨率含计费未开通的 540p，且未标记禁止文生。再跑一遍。
    if !done.contains("playground_2026_viduq4_preview_scheme_v2") {
        match crate::db::migrations::helpers::viduq4_preview::seed_playground_2026_viduq4_preview_scheme(pool).await {
            Ok(_) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_viduq4_preview_scheme_v2')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_viduq4_preview_scheme_v2".into());
            }
            Err(e) => {
                tracing::warn!(
                    "playground_2026_viduq4_preview_scheme_v2 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    if !done.contains("playground_2026_viduq3_schemes_v1") {
        match crate::db::migrations::helpers::viduq3::seed_playground_2026_viduq3_schemes(pool).await {
            Ok(_) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_viduq3_schemes_v1')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_viduq3_schemes_v1".into());
            }
            Err(e) => {
                tracing::warn!(
                    "playground_2026_viduq3_schemes_v1 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    // v2 把分辨率改成了计费表里的 480p。官方是 540p/720p/1080p/2K/4K，这里改回。
    if !done.contains("playground_2026_viduq4_preview_scheme_v3") {
        match crate::db::migrations::helpers::viduq4_preview::seed_playground_2026_viduq4_preview_scheme(pool).await {
            Ok(_) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('playground_2026_viduq4_preview_scheme_v3')",
                )
                .execute(pool)
                .await;
                done.insert("playground_2026_viduq4_preview_scheme_v3".into());
            }
            Err(e) => {
                tracing::warn!(
                    "playground_2026_viduq4_preview_scheme_v3 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    // ── 大表慢任务收拢至更新最末尾：保证前面主线程所有表的 once_migration 全部串行成功落地后，才在后台无锁构建 ──
    // 1. 请求日志大表 logs 谓词与核心用量覆盖索引 后台并发无锁重建（零启动等待、1024MB 内存、零锁表、全并发读写）
    once_heavy_migration!(
        pool,
        done,
        "logs_table_pending_indexes_rebuild_v2",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_is_completed_pending ON logs (id DESC) WHERE is_completed = 0",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_pending_poll ON logs (id ASC) WHERE is_completed = 0 AND status_code = 200",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_inflight_user_action ON logs (user_id, action_type) WHERE is_completed = 0 AND status_code IN (0, 200)",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_orphan_scan ON logs (created_at) WHERE is_completed = 0 AND status_code = 0",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_stat_covering ON logs (created_at DESC) INCLUDE (model, prompt_tokens, completion_tokens, has_video, cost, status_code, is_completed)",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_user_dashboard_covering_v2 ON logs (user_id, created_at DESC) INCLUDE (prompt_tokens, completion_tokens, cost, token_id)",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_action_created_stats ON logs (action_type, created_at DESC) INCLUDE (cost, status_code, pre_deduct_gift)"
    );

    // 2. 冷归档日志大表 logs_archive 全面升级整型字段为 BIGINT / INT8 (i64)，独立单语句动态重写（1024MB 内存）
    once_heavy_migration!(
        pool,
        done,
        "logs_archive_table_to_bigint_v1",
        r#"DO $$
DECLARE
    alter_clauses text;
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'logs_archive') THEN
        ALTER TABLE logs_archive ADD COLUMN IF NOT EXISTS has_video BIGINT NOT NULL DEFAULT 0;

        SELECT string_agg('ALTER COLUMN ' || quote_ident(column_name) || ' TYPE BIGINT', ', ')
        INTO alter_clauses
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'logs_archive'
          AND column_name IN (
              'prompt_tokens', 'completion_tokens', 'cached_tokens', 'latency_ms',
              'status_code', 'is_stream', 'is_ha', 'is_completed', 'has_video',
              'channel_config_id', 'commission_posted'
          )
          AND data_type IN ('integer', 'smallint');

        IF alter_clauses IS NOT NULL AND alter_clauses <> '' THEN
            PERFORM set_config('maintenance_work_mem', '1024MB', true);
            PERFORM set_config('max_parallel_maintenance_workers', '2', true);
            EXECUTE 'ALTER TABLE logs_archive ' || alter_clauses;
        END IF;
    END IF;
END $$;"#
    );

    // 3. 请求日志与统计大表索引终极优化与瘦身清理（解决 3.5s/44s 首屏聚合慢查询与 32s 僵死轮询，剔除冗余索引消除写放大）
    once_heavy_migration!(
        pool,
        done,
        "logs_and_stats_indexes_optimize_and_prune_v3",
        // ── 【加法：终极覆盖索引】 ──
        // 管理端日志大盘统计与首屏分页全覆盖（覆盖全部聚合计算列与 id，0 回表 Index Only Scan）
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_stat_covering_v3 ON logs (created_at DESC) INCLUDE (id, cost, pre_deduct_gift, status_code, is_completed, prompt_tokens, completion_tokens, has_video, action_type, model)",
        // 用户端个人中心日志大盘统计与首屏分页全覆盖（一网打尽所有按 user_id + created_at 范围的查询）
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_user_stat_covering_v3 ON logs (user_id, created_at DESC) INCLUDE (id, cost, pre_deduct_gift, status_code, is_completed, prompt_tokens, completion_tokens, has_video, action_type, model)",
        // 异步任务轮询与超期退款终极二分部分索引（将 created_at 纳入索引前缀，超期扫描 0ms 二分命中）
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_pending_poll_v3 ON logs (created_at ASC, id ASC) WHERE is_completed = 0 AND status_code = 200",
        // Playground 画布 loading 状态部分索引（消除对大文本 TOAST 字段的 LIKE 全表解压扫描）
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_playground_projects_loading ON playground_projects (id) WHERE is_deleted = 0 AND canvas_data LIKE '%\"status\":\"loading\"%'",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_playground_2026_workflows_loading ON playground_2026_workflows (id) WHERE is_deleted = 0 AND canvas_data LIKE '%\"status\":\"loading\"%'",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_playground_2026_projects_loading ON playground_2026_projects (id) WHERE is_deleted = 0 AND canvas_data LIKE '%\"status\":\"loading\"%'",
        // 日用量统计表全覆盖索引（消除个人累计消耗与模型大盘统计的回表）
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_usage_daily_stats_user_covering_v2 ON usage_daily_stats (user_id, stat_date) INCLUDE (total_cost, total_pre_deduct_gift, total_requests, success_count)",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_usage_daily_stats_date_model_covering ON usage_daily_stats (stat_date, model) INCLUDE (total_requests, total_tokens, total_cost)",
        // ── 【减法：安全无锁卸载多余废弃/残缺旧索引，降低 50% 写入放大】 ──
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_stat_covering",
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_user_dashboard_covering_v2",
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_user_dashboard_covering",
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_user_action_created",
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_user_vision_created_v2",
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_vision_created_at_v2",
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_pending_poll",
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_is_completed_pending",
        // 清理本地与存量环境的历史单列/冗余残留索引
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_user_id",
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_user_created",
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_created_at",
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_created_at_agg",
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_channel_created",
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_model_created",
        "DROP INDEX CONCURRENTLY IF EXISTS idx_logs_token_created",
        // ── 【刷新统计信息】 ──
        "ANALYZE logs",
        "ANALYZE usage_daily_stats",
        "ANALYZE playground_projects",
    );

    // ── 腾讯云 MPS AI 换装规则收敛与火山方舟语音合成参考谷歌多路由整理 ──
    once_migration!(
        pool,
        done,
        "consolidate_tencent_tryon_and_volc_tts_forward_rules_v1",
        // 1. 火山方舟语音合成升级为多路由自适应规则（参考 Google 范式注入 path_rewrites）
        r#"UPDATE forward_rules
        SET name = '火山方舟 语音合成', category = '音频', rule_type = 'volcengine',
            description = 'OpenAI 语音合成（/v1/audio/speech）转火山方舟 TTS V3 SSE，支持官方原生 /api/v3/tts/unidirectional(sse) 自适应透传。渠道地址: openspeech.bytedance.com，密钥为 X-Api-Key，模型ID通过 X-Api-Resource-Id 传递',
            config_json = '{"target_type":"volcengine_tts","auth_type":"volcengine_tts","path_rewrites":[{"old":"/v1/audio/speech","new":"/api/v3/tts/unidirectional/sse"},{"old":"/api/v3/tts/unidirectional/sse","new":"/api/v3/tts/unidirectional/sse"},{"old":"/api/v3/tts/unidirectional","new":"/api/v3/tts/unidirectional"}]}'
        WHERE name IN ('火山方舟 语音合成', '火山方舟 语音合成 (TTS V3)') AND category = '音频' AND is_system = 1"#,
        r#"INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system, eid)
        SELECT '火山方舟 语音合成', 'volcengine',
               'OpenAI 语音合成（/v1/audio/speech）转火山方舟 TTS V3 SSE，支持官方原生 /api/v3/tts/unidirectional(sse) 自适应透传。渠道地址: openspeech.bytedance.com，密钥为 X-Api-Key，模型ID通过 X-Api-Resource-Id 传递',
               '{"target_type":"volcengine_tts","auth_type":"volcengine_tts","path_rewrites":[{"old":"/v1/audio/speech","new":"/api/v3/tts/unidirectional/sse"},{"old":"/api/v3/tts/unidirectional/sse","new":"/api/v3/tts/unidirectional/sse"},{"old":"/api/v3/tts/unidirectional","new":"/api/v3/tts/unidirectional"}]}',
               '音频', 1, '1' || lpad((floor(random() * 10000)::int)::text, 4, '0')
        WHERE NOT EXISTS (SELECT 1 FROM forward_rules WHERE name = '火山方舟 语音合成' AND category = '音频')"#,
        // 2. 关联模型重定向（将旧换装与旧 Chunked 规则平滑指向新规则）
        r#"UPDATE models m
        SET forward_rule_ids = (
            SELECT json_agg(DISTINCT CASE WHEN elem::text::bigint = del.id THEN n.id ELSE elem::text::bigint END)::text
            FROM jsonb_array_elements_text(m.forward_rule_ids::jsonb) AS elem
        ), updated_at = CURRENT_TIMESTAMP
        FROM (SELECT id FROM forward_rules WHERE name = '腾讯云 MPS AI 换装 (同步轮询)' LIMIT 1) n,
             (SELECT id FROM forward_rules WHERE name = '腾讯云 MPS AI 换装' LIMIT 1) del
        WHERE m.forward_rule_ids IS NOT NULL AND m.forward_rule_ids LIKE '[%]' AND m.forward_rule_ids::jsonb @> to_jsonb(del.id)"#,
        r#"UPDATE models m
        SET forward_rule_ids = (
            SELECT json_agg(DISTINCT CASE WHEN elem::text::bigint = del.id THEN n.id ELSE elem::text::bigint END)::text
            FROM jsonb_array_elements_text(m.forward_rule_ids::jsonb) AS elem
        ), updated_at = CURRENT_TIMESTAMP
        FROM (SELECT id FROM forward_rules WHERE name = '火山方舟 语音合成' AND category = '音频' LIMIT 1) n,
             (SELECT id FROM forward_rules WHERE name = '火山方舟 语音合成 (TTS V3 Chunked)' LIMIT 1) del
        WHERE m.forward_rule_ids IS NOT NULL AND m.forward_rule_ids LIKE '[%]' AND m.forward_rule_ids::jsonb @> to_jsonb(del.id)"#,
        // 3. 模型库重定向（model_library 同步对齐）
        r#"UPDATE model_library ml
        SET forward_rule_ids = (
            SELECT json_agg(DISTINCT CASE WHEN elem::text::bigint = del.id THEN n.id ELSE elem::text::bigint END)::text
            FROM jsonb_array_elements_text(ml.forward_rule_ids::jsonb) AS elem
        ), updated_at = CURRENT_TIMESTAMP
        FROM (SELECT id FROM forward_rules WHERE name = '腾讯云 MPS AI 换装 (同步轮询)' LIMIT 1) n,
             (SELECT id FROM forward_rules WHERE name = '腾讯云 MPS AI 换装' LIMIT 1) del
        WHERE ml.forward_rule_ids IS NOT NULL AND ml.forward_rule_ids LIKE '[%]' AND ml.forward_rule_ids::jsonb @> to_jsonb(del.id)"#,
        r#"UPDATE model_library ml
        SET forward_rule_ids = (
            SELECT json_agg(DISTINCT CASE WHEN elem::text::bigint = del.id THEN n.id ELSE elem::text::bigint END)::text
            FROM jsonb_array_elements_text(ml.forward_rule_ids::jsonb) AS elem
        ), updated_at = CURRENT_TIMESTAMP
        FROM (SELECT id FROM forward_rules WHERE name = '火山方舟 语音合成' AND category = '音频' LIMIT 1) n,
             (SELECT id FROM forward_rules WHERE name = '火山方舟 语音合成 (TTS V3 Chunked)' LIMIT 1) del
        WHERE ml.forward_rule_ids IS NOT NULL AND ml.forward_rule_ids LIKE '[%]' AND ml.forward_rule_ids::jsonb @> to_jsonb(del.id)"#,
        // 4. 彻底删除已淘汰的老旧系统规则
        "DELETE FROM forward_rules WHERE name IN ('腾讯云 MPS AI 换装', '火山方舟 语音合成 (TTS V3 Chunked)') AND is_system = 1"
    );

    // ── 腾讯云 TokenHub 混元生图 3.0 与 3.5 转发规则 ──
    once_migration!(
        pool,
        done,
        "tencent_tokenhub_hunyuan_image_v3_and_v35_forward_rules_v1",
        r#"INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system, eid)
        SELECT '腾讯云 TokenHub 混元生图 3.0', 'tencent',
               '腾讯云 TokenHub 混元生图 3.0（hy-image-v3）同步文生图/参考生图原生通道。渠道地址: tokenhub.tencentmaas.com，鉴权头: Authorization: Bearer YOUR_API_KEY',
               '{"target_type":"tencent_tokenhub_image","auth_type":"bearer","path_rewrites":[{"old":"/v1/images/generations","new":"/v1/wand/hunyuan-image/v3-generation"},{"old":"/v1/wand/hunyuan-image/v3-generation","new":"/v1/wand/hunyuan-image/v3-generation"}]}',
               '图片', 1, '1' || lpad((floor(random() * 10000)::int)::text, 4, '0')
        WHERE NOT EXISTS (SELECT 1 FROM forward_rules WHERE name = '腾讯云 TokenHub 混元生图 3.0' AND category = '图片')"#,
        r#"INSERT INTO forward_rules (name, rule_type, description, config_json, category, is_system, eid)
        SELECT '腾讯云 TokenHub 混元生图 3.5', 'tencent',
               '腾讯云 TokenHub 混元生图 3.5（hy-image-v3.5-preview）同步生图/多轮编辑/4K原生通道。渠道地址: tokenhub.tencentmaas.com，鉴权头: Authorization: Bearer YOUR_API_KEY',
               '{"target_type":"tencent_tokenhub_v35_image","auth_type":"bearer","path_rewrites":[{"old":"/v1/images/generations","new":"/v1/wand/hunyuan-image/v35-generation"},{"old":"/v1/wand/hunyuan-image/v35-generation","new":"/v1/wand/hunyuan-image/v35-generation"}]}',
               '图片', 1, '1' || lpad((floor(random() * 10000)::int)::text, 4, '0')
        WHERE NOT EXISTS (SELECT 1 FROM forward_rules WHERE name = '腾讯云 TokenHub 混元生图 3.5' AND category = '图片')"#
    );

    // ── 归档日志冷表字段全量对齐、排查索引补充与系统内置插件注册 ──
    once_heavy_migration!(
        pool,
        done,
        "logs_archive_plugin_and_indexes_v1",
        // 1. 字段对齐（在建索引之前）：补齐 logs 历史追加缺失字段
        "ALTER TABLE logs_archive ADD COLUMN IF NOT EXISTS commission_posted BIGINT NOT NULL DEFAULT 0",
        "ALTER TABLE logs_archive ADD COLUMN IF NOT EXISTS coupon_deduct DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE logs_archive ADD COLUMN IF NOT EXISTS pre_deduct_voucher DOUBLE PRECISION NOT NULL DEFAULT 0",
        // 2. 补建冷库高频排查索引（无锁并发建立，消除全表扫描）
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_archive_log_id ON logs_archive (log_id) WHERE log_id IS NOT NULL AND log_id <> ''",
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_logs_archive_task_id ON logs_archive (task_id) WHERE task_id IS NOT NULL AND task_id <> ''",
        // 3. 注册 logs_archive 系统内置插件
        r#"INSERT INTO plugins (name, title, description, is_enabled, allowed_levels, category, show_in_admin_menu, admin_menu_sort, admin_menu_title, admin_menu_default_tab, created_at, updated_at)
        VALUES (
            'logs_archive',
            '归档日志',
            '系统冷归档日志管理中心。专为历史超期请求日志（logs_archive）提供高性能查询、多维过滤、明细审计与对账分析，保护热表性能的同时杜绝历史数据断层。',
            1,
            'admin',
            'system_builtin',
            1,
            88,
            '归档日志',
            'archive_list',
            CURRENT_TIMESTAMP,
            CURRENT_TIMESTAMP
        )
        ON CONFLICT (name) DO UPDATE SET
            category = 'system_builtin',
            title = EXCLUDED.title,
            description = EXCLUDED.description;"#
    );

    // ── 火山 MediaKit 转发规则合并、多路由兼容与所属分类统一为「画质增强」 ──
    if !done.contains("volc_mediakit_forward_rules_unify_v1") {
        match crate::db::migrations::helpers::volc_mediakit_image::sync_mediakit_forward_rules(pool)
            .await
        {
            Ok(_) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('volc_mediakit_forward_rules_unify_v1') ON CONFLICT DO NOTHING",
                )
                .execute(pool)
                .await;
                done.insert("volc_mediakit_forward_rules_unify_v1".into());
            }
            Err(e) => {
                tracing::warn!(
                    "volc_mediakit_forward_rules_unify_v1 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    // ── Google Gemini 预设转发规则幂等追加 /v1beta/interactions 与 /v1/interactions 直达映射 ──
    let mig_key = "google_gemini_interactions_forward_rules_v1";
    if !done.contains(mig_key) {
        let rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT id, config_json FROM forward_rules WHERE name IN ('Google Gemini 原生生图', 'Google Gemini 格式转换 (聊天)')",
        )
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        for (id, cfg_str) in rows {
            if let Ok(mut cfg) = serde_json::from_str::<serde_json::Value>(&cfg_str) {
                if let Some(arr) = cfg.get_mut("path_rewrites").and_then(|v| v.as_array_mut()) {
                    let mut changed = false;
                    for path in ["/v1beta/interactions", "/v1/interactions"] {
                        if !arr
                            .iter()
                            .any(|r| r.get("old").and_then(|v| v.as_str()) == Some(path))
                        {
                            arr.push(serde_json::json!({ "old": path, "new": path }));
                            changed = true;
                        }
                    }
                    if changed {
                        if let Ok(new_cfg) = serde_json::to_string(&cfg) {
                            let _ = sqlx::query(
                                "UPDATE forward_rules SET config_json = $1 WHERE id = $2",
                            )
                            .bind(new_cfg)
                            .bind(id)
                            .execute(pool)
                            .await;
                        }
                    }
                }
            }
        }
        let _ = sqlx::query(
            "INSERT INTO sys_migration_history (id) VALUES ($1) ON CONFLICT DO NOTHING",
        )
        .bind(mig_key)
        .execute(pool)
        .await;
        done.insert(mig_key.into());
    }

    once_migration!(
        pool,
        done,
        "playground_2026_space_grants_v1",
        r#"CREATE TABLE IF NOT EXISTS playground_2026_space_grants (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            redemption_id BIGINT NOT NULL REFERENCES redemptions(id) ON DELETE RESTRICT,
            redemption_log_id BIGINT NOT NULL REFERENCES redemption_logs(id) ON DELETE RESTRICT,
            space_mb BIGINT NOT NULL,
            expires_at TIMESTAMPTZ,
            status BIGINT NOT NULL DEFAULT 1,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "COMMENT ON TABLE playground_2026_space_grants IS '创作空间兑换码发放的创作中心2026存储。单位 MB。status 1=生效，0=已收回。到期看 expires_at'",
        "COMMENT ON COLUMN playground_2026_space_grants.space_mb IS '这一次兑换增加的存储空间，单位 MB'",
        "COMMENT ON COLUMN playground_2026_space_grants.status IS '1=计入配额，0=已收回。过期仍保持 1，由 expires_at 判断'",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_pg2026_space_grants_log ON playground_2026_space_grants (redemption_log_id)",
        "CREATE INDEX IF NOT EXISTS idx_pg2026_space_grants_redemption ON playground_2026_space_grants (redemption_id)",
        "CREATE INDEX IF NOT EXISTS idx_pg2026_space_grants_user_active ON playground_2026_space_grants (user_id) WHERE status = 1"
    );

    once_migration!(
        pool,
        done,
        "redemption_eligibility_v1",
        "ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS condition_match BIGINT NOT NULL DEFAULT 0",
        "ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS cond_min_balance DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS cond_month_spend DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS cond_total_spend DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS cond_month_recharge DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS cond_total_recharge DOUBLE PRECISION NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN redemptions.condition_match IS '0=已启用的兑换条件全部满足，1=满足其中一项。金额为 0 的条件不启用'",
        "COMMENT ON COLUMN redemptions.cond_min_balance IS '兑换前系统余额需达到的金额。0=不要求。不含赠送余额'",
        "COMMENT ON COLUMN redemptions.cond_month_spend IS '站点时区当月已结算消费需达到的金额。0=不要求'",
        "COMMENT ON COLUMN redemptions.cond_total_spend IS '累计已结算消费需达到的金额。0=不要求'",
        "COMMENT ON COLUMN redemptions.cond_month_recharge IS '站点时区当月充值需达到的金额。0=不要求。只算在线支付和管理员充入系统余额'",
        "COMMENT ON COLUMN redemptions.cond_total_recharge IS '累计充值需达到的金额。0=不要求。口径与当月充值相同'"
    );

    once_migration!(
        pool,
        done,
        "user_model_vouchers_log_id_v1",
        "ALTER TABLE user_model_vouchers ADD COLUMN IF NOT EXISTS redemption_log_id BIGINT",
        "COMMENT ON COLUMN user_model_vouchers.redemption_log_id IS '关联 redemption_logs.id，用于按单次兑换作废与核销明细状态同步'",
        r#"WITH v_ranked AS (
            SELECT id, redemption_id, user_id,
                   ROW_NUMBER() OVER (PARTITION BY redemption_id, user_id ORDER BY id ASC) AS rn
            FROM user_model_vouchers
            WHERE redemption_log_id IS NULL
        ),
        l_ranked AS (
            SELECT id, redemption_id, user_id,
                   ROW_NUMBER() OVER (PARTITION BY redemption_id, user_id ORDER BY id ASC) AS rn
            FROM redemption_logs
        )
        UPDATE user_model_vouchers AS v
        SET redemption_log_id = l.id
        FROM v_ranked vr
        JOIN l_ranked l ON l.redemption_id = vr.redemption_id AND l.user_id = vr.user_id AND l.rn = vr.rn
        WHERE v.id = vr.id"#,
        "UPDATE redemption_logs AS l SET refunded = 1 FROM user_model_vouchers AS v WHERE v.redemption_log_id = l.id AND v.status <> 1 AND l.refunded = 0",
        "CREATE INDEX IF NOT EXISTS idx_user_model_vouchers_log_id ON user_model_vouchers (redemption_log_id)",
        "CREATE INDEX IF NOT EXISTS idx_user_model_vouchers_redemption_id ON user_model_vouchers (redemption_id)"
    );

    once_migration!(
        pool,
        done,
        "redemptions_desc_and_remark_v1",
        "ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS description TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS remark TEXT NOT NULL DEFAULT ''",
        "COMMENT ON COLUMN redemptions.description IS '活动简介/广告语，发放与用户兑换时展示说明'",
        "COMMENT ON COLUMN redemptions.remark IS '管理员备注说明，仅后台管理者可见，支持在兑换管理列表快速编辑'"
    );

    if !done.contains("volc_mediakit_encapsulation_unify_v1") {
        match crate::db::migrations::helpers::volc_mediakit_image::seed(pool).await {
            Ok(_) => {
                let _ = sqlx::query(
                    "INSERT INTO sys_migration_history (id) VALUES ('volc_mediakit_encapsulation_unify_v1') ON CONFLICT DO NOTHING",
                )
                .execute(pool)
                .await;
                done.insert("volc_mediakit_encapsulation_unify_v1".into());
            }
            Err(e) => {
                tracing::warn!(
                    "volc_mediakit_encapsulation_unify_v1 失败，未标记完成以便重试: {e}"
                );
            }
        }
    }

    once_migration!(
        pool,
        done,
        "mkt_plan_offer_snapshot_v2",
        "ALTER TABLE mkt_member_credits ADD COLUMN IF NOT EXISTS starts_at TIMESTAMPTZ",
        "COMMENT ON COLUMN mkt_member_credits.starts_at IS '下一期额度的开始时间。到点前 status=scheduled，不计入当前周期'",
        "ALTER TABLE mkt_member_orders ADD COLUMN IF NOT EXISTS tier_starts_at TIMESTAMPTZ",
        "COMMENT ON COLUMN mkt_member_orders.tier_starts_at IS '下一期会员等级的开始时间。到点才改用户等级，当前周期的等级保持不变'"
    );

    once_migration!(
        pool,
        done,
        "marketing_games_plugin_v1",
        r#"INSERT INTO plugins (
            name, title, description, is_enabled, allowed_levels, category,
            show_in_admin_menu, admin_menu_sort, admin_menu_title, admin_menu_default_tab
        ) VALUES (
            'marketing_games', '营销小游戏',
            '配置3D大转盘等互动抽奖游戏活动，支持多页面延时弹窗、360°立体旋转视角、卡券与余额奖品自用或分享好友兑换。',
            0, 'all', 'user',
            0, 48, '', 'campaigns'
        ) ON CONFLICT (name) DO NOTHING"#,
        r#"CREATE TABLE IF NOT EXISTS mkt_game_campaigns (
            id BIGSERIAL PRIMARY KEY,
            name TEXT NOT NULL DEFAULT '',
            game_type TEXT NOT NULL DEFAULT 'lucky_wheel',
            theme_style TEXT NOT NULL DEFAULT 'cartoon_vibrant',
            headline TEXT NOT NULL DEFAULT '',
            subtitle TEXT NOT NULL DEFAULT '',
            rules_text TEXT NOT NULL DEFAULT '',
            is_enabled BIGINT NOT NULL DEFAULT 1,
            placements_json TEXT NOT NULL DEFAULT '["playground_2026"]',
            popup_delay_seconds BIGINT NOT NULL DEFAULT 10,
            popup_frequency TEXT NOT NULL DEFAULT 'once_per_day',
            show_floating_entry BIGINT NOT NULL DEFAULT 1,
            daily_free_spins BIGINT NOT NULL DEFAULT 1,
            total_free_spins BIGINT NOT NULL DEFAULT 0,
            share_reward_spins BIGINT NOT NULL DEFAULT 1,
            share_reward_daily_limit BIGINT NOT NULL DEFAULT 3,
            condition_match BIGINT NOT NULL DEFAULT 0,
            cond_min_balance DOUBLE PRECISION NOT NULL DEFAULT 0,
            cond_month_spend DOUBLE PRECISION NOT NULL DEFAULT 0,
            cond_total_spend DOUBLE PRECISION NOT NULL DEFAULT 0,
            cond_month_recharge DOUBLE PRECISION NOT NULL DEFAULT 0,
            cond_total_recharge DOUBLE PRECISION NOT NULL DEFAULT 0,
            starts_at TIMESTAMPTZ,
            expires_at TIMESTAMPTZ,
            sort_order BIGINT NOT NULL DEFAULT 0,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "COMMENT ON TABLE mkt_game_campaigns IS '营销小游戏活动配置表（如3D大转盘）'",
        "CREATE INDEX IF NOT EXISTS idx_mkt_game_campaigns_enabled_sort ON mkt_game_campaigns (is_enabled, sort_order DESC, id DESC)",
        r#"CREATE TABLE IF NOT EXISTS mkt_game_prizes (
            id BIGSERIAL PRIMARY KEY,
            campaign_id BIGINT NOT NULL REFERENCES mkt_game_campaigns(id) ON DELETE CASCADE,
            slot_index BIGINT NOT NULL DEFAULT 0,
            name TEXT NOT NULL DEFAULT '',
            badge_text TEXT NOT NULL DEFAULT '',
            color_hex TEXT NOT NULL DEFAULT '',
            icon_key TEXT NOT NULL DEFAULT 'gift',
            prize_type TEXT NOT NULL DEFAULT 'none',
            member_plan_id BIGINT NOT NULL DEFAULT 0,
            member_plan_cycle TEXT NOT NULL DEFAULT '',
            redemption_group_name TEXT NOT NULL DEFAULT '',
            quota_amount DOUBLE PRECISION NOT NULL DEFAULT 0,
            space_mb BIGINT NOT NULL DEFAULT 0,
            allowed_models TEXT NOT NULL DEFAULT '[]',
            valid_days BIGINT NOT NULL DEFAULT 0,
            extra_spins BIGINT NOT NULL DEFAULT 1,
            custom_instructions TEXT NOT NULL DEFAULT '',
            probability_weight BIGINT NOT NULL DEFAULT 100,
            total_stock BIGINT NOT NULL DEFAULT -1,
            used_stock BIGINT NOT NULL DEFAULT 0,
            per_user_win_limit BIGINT NOT NULL DEFAULT -1,
            allow_share BIGINT NOT NULL DEFAULT 1,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "COMMENT ON TABLE mkt_game_prizes IS '营销小游戏大转盘扇区奖品表'",
        "CREATE INDEX IF NOT EXISTS idx_mkt_game_prizes_campaign ON mkt_game_prizes (campaign_id, slot_index ASC, id ASC)",
        r#"CREATE TABLE IF NOT EXISTS mkt_game_records (
            id BIGSERIAL PRIMARY KEY,
            campaign_id BIGINT NOT NULL,
            campaign_name TEXT NOT NULL DEFAULT '',
            prize_id BIGINT NOT NULL,
            slot_index BIGINT NOT NULL DEFAULT 0,
            winner_user_id TEXT NOT NULL,
            prize_name TEXT NOT NULL DEFAULT '',
            prize_type TEXT NOT NULL DEFAULT 'none',
            prize_snapshot_json TEXT NOT NULL DEFAULT '{}',
            claim_code TEXT NOT NULL DEFAULT '',
            claim_status TEXT NOT NULL DEFAULT 'none',
            allow_share BIGINT NOT NULL DEFAULT 1,
            claimed_by_user_id TEXT NOT NULL DEFAULT '',
            claimed_at TIMESTAMPTZ,
            share_rewarded BIGINT NOT NULL DEFAULT 0,
            is_bonus_spin BIGINT NOT NULL DEFAULT 0,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "COMMENT ON TABLE mkt_game_records IS '营销小游戏抽奖中奖记录与奖品兑换码核销表'",
        "CREATE INDEX IF NOT EXISTS idx_mkt_game_records_campaign_user ON mkt_game_records (campaign_id, winner_user_id, created_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_mkt_game_records_claimed_by ON mkt_game_records (claimed_by_user_id, claimed_at DESC)",
        "CREATE UNIQUE INDEX IF NOT EXISTS uidx_mkt_game_records_claim_code ON mkt_game_records (claim_code) WHERE claim_code <> ''",
        r#"CREATE TABLE IF NOT EXISTS mkt_game_bonus_spins (
            id BIGSERIAL PRIMARY KEY,
            campaign_id BIGINT NOT NULL,
            user_id TEXT NOT NULL,
            spins_granted BIGINT NOT NULL DEFAULT 1,
            spins_used BIGINT NOT NULL DEFAULT 0,
            source_type TEXT NOT NULL DEFAULT 'extra_spin',
            source_record_id BIGINT NOT NULL DEFAULT 0,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "COMMENT ON TABLE mkt_game_bonus_spins IS '营销小游戏用户额外获赠抽奖次数（再来一次/好友核销裂变奖励）'",
        "CREATE INDEX IF NOT EXISTS idx_mkt_game_bonus_spins_campaign_user ON mkt_game_bonus_spins (campaign_id, user_id, created_at DESC)"
    );

    // ── 余额兑换码支持赠送金余额面值（gift_quota / gift_amount） ──
    once_migration!(
        pool,
        done,
        "redemptions_gift_quota_v1",
        "ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS gift_quota DOUBLE PRECISION NOT NULL DEFAULT 0",
        "ALTER TABLE redemption_logs ADD COLUMN IF NOT EXISTS gift_amount DOUBLE PRECISION NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN redemptions.gift_quota IS '余额兑换码的赠送金余额面值（计入 users.gift_balance）'",
        "COMMENT ON COLUMN redemption_logs.gift_amount IS '本次兑换入账的赠送金余额金额'"
    );

    // ── 营销小游戏增量补齐字段（兼容已执行早期 marketing_games_plugin_v1 的数据库） ──
    once_migration!(
        pool,
        done,
        "marketing_games_plugin_v2",
        "ALTER TABLE mkt_game_records ADD COLUMN IF NOT EXISTS is_bonus_spin BIGINT NOT NULL DEFAULT 0",
        "ALTER TABLE mkt_game_bonus_spins ADD COLUMN IF NOT EXISTS spins_used BIGINT NOT NULL DEFAULT 0"
    );

    // ── 营销小游戏 v3：游戏券套餐与购买记录完整闭环 ──
    once_migration!(
        pool,
        done,
        "marketing_games_plugin_v3",
        "ALTER TABLE mkt_game_campaigns ADD COLUMN IF NOT EXISTS allow_ticket_purchase BIGINT NOT NULL DEFAULT 1",
        "COMMENT ON COLUMN mkt_game_campaigns.allow_ticket_purchase IS '1=允许使用账户系统余额购买游戏券增加抽奖次数，0=关闭购券'",
        "ALTER TABLE mkt_game_records ADD COLUMN IF NOT EXISTS ticket_order_id BIGINT NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN mkt_game_records.ticket_order_id IS '若使用游戏券抽奖(is_bonus_spin=2)，关联 mkt_game_ticket_orders.id'",
        r#"CREATE TABLE IF NOT EXISTS mkt_game_ticket_packages (
            id BIGSERIAL PRIMARY KEY,
            campaign_id BIGINT NOT NULL DEFAULT 0,
            name TEXT NOT NULL DEFAULT '',
            badge_text TEXT NOT NULL DEFAULT '',
            price DOUBLE PRECISION NOT NULL DEFAULT 10,
            spins_count BIGINT NOT NULL DEFAULT 20,
            valid_days BIGINT NOT NULL DEFAULT 0,
            per_user_limit BIGINT NOT NULL DEFAULT -1,
            total_stock BIGINT NOT NULL DEFAULT -1,
            sold_count BIGINT NOT NULL DEFAULT 0,
            sort_order BIGINT NOT NULL DEFAULT 10,
            is_enabled BIGINT NOT NULL DEFAULT 1,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "COMMENT ON TABLE mkt_game_ticket_packages IS '营销小游戏游戏券套餐表（campaign_id=0为通用套餐，>0为指定活动专属套餐；valid_days=0长期有效，>0为购买后N天有效）'",
        "CREATE INDEX IF NOT EXISTS idx_mkt_game_ticket_packages_campaign ON mkt_game_ticket_packages (campaign_id, is_enabled, sort_order DESC, id ASC)",
        r#"CREATE TABLE IF NOT EXISTS mkt_game_ticket_orders (
            id BIGSERIAL PRIMARY KEY,
            order_no TEXT NOT NULL DEFAULT '',
            user_id TEXT NOT NULL,
            package_id BIGINT NOT NULL DEFAULT 0,
            package_name TEXT NOT NULL DEFAULT '',
            campaign_id BIGINT NOT NULL DEFAULT 0,
            source_campaign_id BIGINT NOT NULL DEFAULT 0,
            campaign_name TEXT NOT NULL DEFAULT '',
            price_paid DOUBLE PRECISION NOT NULL DEFAULT 0,
            spins_granted BIGINT NOT NULL DEFAULT 1,
            spins_used BIGINT NOT NULL DEFAULT 0,
            valid_days BIGINT NOT NULL DEFAULT 0,
            expires_at TIMESTAMPTZ,
            status TEXT NOT NULL DEFAULT 'active',
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "COMMENT ON TABLE mkt_game_ticket_orders IS '营销小游戏用户游戏券购买订单与剩余次数表（仅扣减账户系统余额 users.balance）'",
        "CREATE UNIQUE INDEX IF NOT EXISTS uidx_mkt_game_ticket_orders_order_no ON mkt_game_ticket_orders (order_no) WHERE order_no <> ''",
        "CREATE INDEX IF NOT EXISTS idx_mkt_game_ticket_orders_user_active ON mkt_game_ticket_orders (user_id, campaign_id, status, expires_at ASC)",
        "CREATE INDEX IF NOT EXISTS idx_mkt_game_ticket_orders_created ON mkt_game_ticket_orders (created_at DESC, id DESC)",
        r#"INSERT INTO mkt_game_ticket_packages (campaign_id, name, badge_text, price, spins_count, valid_days, per_user_limit, total_stock, sort_order, is_enabled)
        SELECT 0, '畅玩游戏券 · 20次', '热门体验', 10, 20, 0, -1, -1, 30, 1
        WHERE NOT EXISTS (SELECT 1 FROM mkt_game_ticket_packages LIMIT 1)"#,
        r#"INSERT INTO mkt_game_ticket_packages (campaign_id, name, badge_text, price, spins_count, valid_days, per_user_limit, total_stock, sort_order, is_enabled)
        SELECT 0, '超值游戏券 · 100次', '热销推荐', 50, 100, 0, -1, -1, 20, 1
        WHERE (SELECT COUNT(*) FROM mkt_game_ticket_packages) = 1"#,
        r#"INSERT INTO mkt_game_ticket_packages (campaign_id, name, badge_text, price, spins_count, valid_days, per_user_limit, total_stock, sort_order, is_enabled)
        SELECT 0, '至尊游戏券 · 300次', '多送100次·钜惠', 100, 300, 0, -1, -1, 10, 1
        WHERE (SELECT COUNT(*) FROM mkt_game_ticket_packages) = 2"#
    );

    // ── 营销小游戏 v4：单活动玩一次消耗游戏券张数 (tickets_per_spin) 与游戏券张数语义升级 ──
    once_migration!(
        pool,
        done,
        "marketing_games_plugin_v4",
        "ALTER TABLE mkt_game_campaigns ADD COLUMN IF NOT EXISTS tickets_per_spin BIGINT NOT NULL DEFAULT 1",
        "COMMENT ON COLUMN mkt_game_campaigns.tickets_per_spin IS '玩一次该小游戏需要消耗的游戏券张数（BIGINT，默认 1 张/次）'",
        "UPDATE mkt_game_ticket_packages SET name = REPLACE(name, '· 20次', '· 20张') WHERE name LIKE '%· 20次%'",
        "UPDATE mkt_game_ticket_packages SET name = REPLACE(name, '· 100次', '· 100张') WHERE name LIKE '%· 100次%'",
        "UPDATE mkt_game_ticket_packages SET name = REPLACE(name, '· 300次', '· 300张'), badge_text = REPLACE(badge_text, '多送100次', '多送100张') WHERE name LIKE '%· 300次%' OR badge_text LIKE '%多送100次%'"
    );

    // ── 上游渠道配置预设服务商字典表 (channel_providers) 与存量 provider_type 回填 ──
    once_migration!(
        pool,
        done,
        "channel_providers_v1",
        r#"CREATE TABLE IF NOT EXISTS channel_providers (
            id BIGSERIAL PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            name_en TEXT NOT NULL DEFAULT '',
            sort_order BIGINT NOT NULL DEFAULT 0,
            is_active BIGINT NOT NULL DEFAULT 1,
            is_system BIGINT NOT NULL DEFAULT 0,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "COMMENT ON TABLE channel_providers IS '上游渠道配置预设服务商字典表'",
        r#"INSERT INTO channel_providers (name, name_en, sort_order, is_active, is_system)
           SELECT DISTINCT BTRIM(provider_type), '', 0, 1, 0
           FROM channel_configs
           WHERE provider_type IS NOT NULL
             AND BTRIM(provider_type) <> ''
             AND BTRIM(provider_type) <> 'high_availability_group'
           ON CONFLICT (name) DO NOTHING"#,
        r#"INSERT INTO channel_providers (name, name_en, sort_order, is_active, is_system)
           SELECT 'openai', 'OpenAI', 20, 1, 0
           WHERE NOT EXISTS (SELECT 1 FROM channel_providers LIMIT 1)"#,
        r#"INSERT INTO channel_providers (name, name_en, sort_order, is_active, is_system)
           SELECT 'custom', 'Custom', 10, 1, 0
           WHERE (SELECT COUNT(*) FROM channel_providers) = 1
             AND NOT EXISTS (SELECT 1 FROM channel_providers WHERE name = 'custom')"#
    );

    // ── 上游渠道配置分组倍率同步历史记录表 ──
    once_migration!(
        pool,
        done,
        "channel_config_sync_logs_v1",
        r#"CREATE TABLE IF NOT EXISTS channel_config_sync_logs (
            id BIGSERIAL PRIMARY KEY,
            config_id BIGINT NOT NULL,
            upstream_system TEXT NOT NULL DEFAULT 'newapi',
            upstream_group TEXT NOT NULL DEFAULT '',
            upstream_ratio DOUBLE PRECISION NOT NULL DEFAULT 0,
            rate_add DOUBLE PRECISION NOT NULL DEFAULT 0,
            upstream_currency TEXT NOT NULL DEFAULT '',
            site_currency TEXT NOT NULL DEFAULT '',
            currency_rate DOUBLE PRECISION NOT NULL DEFAULT 1,
            previous_rate DOUBLE PRECISION NOT NULL DEFAULT 1,
            applied_rate DOUBLE PRECISION NOT NULL DEFAULT 1,
            sync_interval_minutes BIGINT NOT NULL DEFAULT 0,
            trigger_type TEXT NOT NULL DEFAULT 'auto',
            status BIGINT NOT NULL DEFAULT 1,
            error_message TEXT NOT NULL DEFAULT '',
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "COMMENT ON TABLE channel_config_sync_logs IS '上游渠道配置分组倍率同步历史记录表'",
        "CREATE INDEX IF NOT EXISTS idx_channel_config_sync_logs_config_created ON channel_config_sync_logs (config_id, created_at DESC, id DESC)"
    );

    // ── 营销卡券会员：消费额度面额价值（展示价值）与卡券模型折扣价格 ──
    once_migration!(
        pool,
        done,
        "marketing_membership_credit_worth_discount_v1",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS credit_worth DOUBLE PRECISION NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN mkt_member_plans.credit_worth IS '消费额度面额价值（展示价值，0=不单独展示，实际可消费金额仍为 credit_amount）'",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS credit_discount DOUBLE PRECISION NOT NULL DEFAULT 1.0",
        "COMMENT ON COLUMN mkt_member_plans.credit_discount IS '卡券有效期内消耗该卡券面值时可享的默认模型折扣倍率（1.0=原价）'",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS credit_model_discounts TEXT NOT NULL DEFAULT '{}'",
        "COMMENT ON COLUMN mkt_member_plans.credit_model_discounts IS '卡券指定模型单独折扣倍率 JSON({\"mid\": 0.7})，优先于 credit_discount，受模型最低限价兜底约束'",
        "ALTER TABLE mkt_member_credits ADD COLUMN IF NOT EXISTS worth_value DOUBLE PRECISION NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN mkt_member_credits.worth_value IS '卡券发放时的面额价值（展示价值），实际可消费面额仍为 face_value'",
        "ALTER TABLE mkt_member_credits ADD COLUMN IF NOT EXISTS credit_discount DOUBLE PRECISION NOT NULL DEFAULT 1.0",
        "COMMENT ON COLUMN mkt_member_credits.credit_discount IS '卡券有效期内且剩余面值大于 0 时可享的默认模型折扣倍率'",
        "ALTER TABLE mkt_member_credits ADD COLUMN IF NOT EXISTS model_discounts TEXT NOT NULL DEFAULT '{}'",
        "COMMENT ON COLUMN mkt_member_credits.model_discounts IS '卡券有效期内且剩余面值大于 0 时指定模型的单独折扣倍率 JSON({\"mid\": 0.7})'"
    );

    // ── 模型卡券兑换码：面额价值（展示价值）与卡券模型折扣价格 ──
    once_migration!(
        pool,
        done,
        "model_voucher_redemption_worth_discount_v1",
        "ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS worth_quota DOUBLE PRECISION NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN redemptions.worth_quota IS '模型卡券兑换码的面额价值（展示价值，0=不单独展示，实际可消费金额仍为 quota）'",
        "ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS voucher_discount DOUBLE PRECISION NOT NULL DEFAULT 1.0",
        "COMMENT ON COLUMN redemptions.voucher_discount IS '模型卡券有效期内消耗该卡券面值时可享的默认模型折扣倍率（1.0=原价）'",
        "ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS model_discounts TEXT NOT NULL DEFAULT '{}'",
        "COMMENT ON COLUMN redemptions.model_discounts IS '模型卡券指定固定模型的单独折扣倍率 JSON({\"mid\": 0.7})，优先于 voucher_discount，受模型最低限价兜底约束'",
        "ALTER TABLE user_model_vouchers ADD COLUMN IF NOT EXISTS worth_value DOUBLE PRECISION NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN user_model_vouchers.worth_value IS '兑换卡券发放时的面额价值（展示价值），实际可消费面额仍为 face_value'",
        "ALTER TABLE user_model_vouchers ADD COLUMN IF NOT EXISTS credit_discount DOUBLE PRECISION NOT NULL DEFAULT 1.0",
        "COMMENT ON COLUMN user_model_vouchers.credit_discount IS '兑换卡券有效期内且剩余面值大于 0 时可享的默认模型折扣倍率'",
        "ALTER TABLE user_model_vouchers ADD COLUMN IF NOT EXISTS model_discounts TEXT NOT NULL DEFAULT '{}'",
        "COMMENT ON COLUMN user_model_vouchers.model_discounts IS '兑换卡券有效期内且剩余面值大于 0 时指定模型的单独折扣倍率 JSON({\"mid\": 0.7})'"
    );

    once_migration!(
        pool,
        done,
        "mkt_campaign_plan_sorts_v1",
        "ALTER TABLE mkt_campaigns ADD COLUMN IF NOT EXISTS plan_sorts_json TEXT NOT NULL DEFAULT '{}'",
        "COMMENT ON COLUMN mkt_campaigns.plan_sorts_json IS '手动选择参与方案时的页面排序映射(JSON对象: plan_id -> sort_order)'"
    );

    once_migration!(
        pool,
        done,
        "mkt_campaign_description_v1",
        "ALTER TABLE mkt_campaigns ADD COLUMN IF NOT EXISTS description TEXT NOT NULL DEFAULT ''",
        "COMMENT ON COLUMN mkt_campaigns.description IS '活动详细说明描述'"
    );

    once_migration!(
        pool,
        done,
        "marketing_games_plugin_v5",
        "ALTER TABLE mkt_game_campaigns ADD COLUMN IF NOT EXISTS multi_spin_counts_json TEXT NOT NULL DEFAULT '[5,10,20]'",
        "COMMENT ON COLUMN mkt_game_campaigns.multi_spin_counts_json IS '多次连抽档位配置 JSON 数组（默认 [5,10,20] 表示 5连抽/10连抽/20连抽，空数组 [] 表示关闭连抽）'"
    );

    once_migration!(
        pool,
        done,
        "mkt_member_plan_sale_limits_v1",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS sale_limit_total BIGINT NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN mkt_member_plans.sale_limit_total IS '方案全站总销售次数限制（0=不限）'",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS sale_limit_daily BIGINT NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN mkt_member_plans.sale_limit_daily IS '方案全站单日销售次数限制（自然日每日 00:00 重置，0=不限）'",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS sale_limit_window_days BIGINT NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN mkt_member_plans.sale_limit_window_days IS '方案全站 N 天销售限制窗口天数 N（0=不开启）'",
        "ALTER TABLE mkt_member_plans ADD COLUMN IF NOT EXISTS sale_limit_window_count BIGINT NOT NULL DEFAULT 0",
        "COMMENT ON COLUMN mkt_member_plans.sale_limit_window_count IS '方案全站近 N 天内销售次数限制（0=不限）'",
        "CREATE INDEX IF NOT EXISTS idx_mkt_orders_plan_paid_at ON mkt_member_orders (plan_id, COALESCE(paid_at, created_at) DESC) WHERE pay_status = 'paid'"
    );

    // ── 移除菜单配置中已废弃的用户端「资产素材」(/assets) 与「素材资产管理国际版」(/assets-intl) ──
    once_migration!(
        pool,
        done,
        "remove_user_asset_menus_from_menu_config_v1",
        r#"UPDATE settings SET value = (
              SELECT COALESCE(
                jsonb_set(
                  value::jsonb,
                  '{items}',
                  COALESCE((
                    SELECT jsonb_agg(elem)
                    FROM jsonb_array_elements(COALESCE(value::jsonb->'items', '[]'::jsonb)) elem
                    WHERE elem->>'key' NOT IN ('/assets', '/assets-intl')
                  ), '[]'::jsonb)
                )::text,
                value
              )
            )
            WHERE key = 'menu_config_settings'
              AND value IS NOT NULL
              AND value <> ''
              AND (
                value::jsonb->'items' @> '[{"key":"/assets"}]'::jsonb
                OR value::jsonb->'items' @> '[{"key":"/assets-intl"}]'::jsonb
              )"#
    );

    // ── 微信多账号共享绑定上限支持（contact_bind_limits 放行 wechat，补齐 users.wechat_id 索引） ──
    once_migration!(
        pool,
        done,
        "contact_bind_limits_wechat_v1",
        "ALTER TABLE contact_bind_limits DROP CONSTRAINT IF EXISTS contact_bind_limits_kind_check",
        "ALTER TABLE contact_bind_limits ADD CONSTRAINT contact_bind_limits_kind_check CHECK (kind IN ('email', 'mobile', 'wechat'))",
        "CREATE INDEX IF NOT EXISTS idx_users_wechat_id ON users (wechat_id) WHERE wechat_id IS NOT NULL AND btrim(wechat_id) <> ''"
    );

    // ── 同步管理后台普通用户备注(admin_remark)与推广用户备注(remark)存量数据 ──
    once_migration!(
        pool,
        done,
        "sync_users_admin_remark_and_remark_v1",
        r#"UPDATE users
           SET admin_remark = COALESCE(NULLIF(btrim(COALESCE(admin_remark, '')), ''), NULLIF(btrim(COALESCE(remark, '')), ''), ''),
               remark = COALESCE(NULLIF(btrim(COALESCE(admin_remark, '')), ''), NULLIF(btrim(COALESCE(remark, '')), ''))
           WHERE COALESCE(btrim(admin_remark), '') <> COALESCE(btrim(remark), '')"#
    );

    Ok(())
}
