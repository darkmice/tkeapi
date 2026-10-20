/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! HA 规则包：稀疏熔断表 + 旧扁平配置迁移。热路径经 `relay_settings` 缓存 `Arc` 共享。

use crate::db::Database;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

pub const MAX_RULES: usize = 32;
pub const MAX_RETRIES: u32 = 100;
const MAX_NAME: usize = 64;
const MAX_MELT: usize = 32;
const MAX_KEYWORDS: usize = 64;
const MAX_KEYWORD_LEN: usize = 128;
const MAX_ID: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HaRule {
    pub id: String,
    pub name: String,
    pub retries: u32,
    #[serde(default = "default_budget")]
    pub budget: u64,
    #[serde(default = "default_ttfb")]
    pub ttfb: u64,
    #[serde(default = "default_err")]
    pub err: String,
    #[serde(default)]
    pub melt: HashMap<String, u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny: Vec<String>,
}

fn default_budget() -> u64 {
    900
}

fn default_ttfb() -> u64 {
    0
}

fn default_err() -> String {
    "first".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HaRulesBundle {
    pub def: String,
    pub rules: Vec<HaRule>,
    /// 热路径共享，避免每次 `resolve_arc` 深拷贝规则
    #[serde(skip)]
    rule_arcs: OnceLock<Vec<Arc<HaRule>>>,
}

impl Clone for HaRulesBundle {
    fn clone(&self) -> Self {
        Self {
            def: self.def.clone(),
            rules: self.rules.clone(),
            rule_arcs: OnceLock::new(),
        }
    }
}

impl HaRule {
    pub fn default_rule(id: &str, name: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            retries: 3,
            budget: default_budget(),
            ttfb: default_ttfb(),
            err: default_err(),
            melt: default_melt(),
            allow: Vec::new(),
            deny: Vec::new(),
        }
    }

    #[inline]
    pub fn err_last(&self) -> bool {
        self.err == "last"
    }

    /// 精确码 → 否则 5xx 桶（仅 500–599）→ 否则 0（不熔断）
    pub fn melt_secs(&self, status: u16) -> u32 {
        if let Some(&s) = self.melt.get(&status.to_string()) {
            return s;
        }
        if (500..600).contains(&status) {
            return self.melt.get("5xx").copied().unwrap_or(0);
        }
        0
    }
}

fn default_melt() -> HashMap<String, u32> {
    HashMap::from([
        ("429".into(), 60),
        ("401".into(), 1800),
        ("402".into(), 1800),
        ("404".into(), 3),
        ("5xx".into(), 300),
    ])
}

pub fn default_rule_arc() -> Arc<HaRule> {
    static R: OnceLock<Arc<HaRule>> = OnceLock::new();
    R.get_or_init(|| Arc::new(HaRule::default_rule("r1", "通用对话")))
        .clone()
}

impl HaRulesBundle {
    pub fn has_id(&self, id: &str) -> bool {
        self.rules.iter().any(|r| r.id == id)
    }

    fn arcs(&self) -> &[Arc<HaRule>] {
        self.rule_arcs
            .get_or_init(|| self.rules.iter().cloned().map(Arc::new).collect())
    }

    pub fn resolve_arc(&self, id: Option<&str>) -> Arc<HaRule> {
        let want = id.filter(|s| !s.is_empty()).unwrap_or(self.def.as_str());
        let arcs = self.arcs();
        arcs.iter()
            .find(|r| r.id == want)
            .or_else(|| arcs.iter().find(|r| r.id == self.def.as_str()))
            .or_else(|| arcs.first())
            .cloned()
            .unwrap_or_else(default_rule_arc)
    }
}

pub fn parse_rule_id(config_json: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(config_json)
        .ok()?
        .get("rule")?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

pub fn normalize_keywords(list: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(list.len().min(MAX_KEYWORDS));
    for s in list {
        let trimmed = s.trim();
        if trimmed.is_empty() || trimmed.len() > MAX_KEYWORD_LEN {
            continue;
        }
        if seen.insert(trimmed.to_lowercase()) {
            out.push(trimmed.to_string());
            if out.len() >= MAX_KEYWORDS {
                break;
            }
        }
    }
    out
}

pub fn sanitize_bundle(raw: HaRulesBundle) -> Result<HaRulesBundle, String> {
    if raw.rules.is_empty() {
        return Err("至少保留一条高可用规则".into());
    }
    if raw.rules.len() > MAX_RULES {
        return Err(format!("高可用规则最多 {MAX_RULES} 条"));
    }
    let mut seen = HashSet::new();
    let mut rules = Vec::with_capacity(raw.rules.len());
    for (i, r) in raw.rules.into_iter().enumerate() {
        let id = sanitize_id(&r.id, i)?;
        if !seen.insert(id.clone()) {
            return Err(format!("规则 id 重复: {id}"));
        }
        rules.push(sanitize_rule(r, id)?);
    }
    let def = if rules.iter().any(|r| r.id == raw.def) {
        raw.def
    } else {
        rules[0].id.clone()
    };
    Ok(HaRulesBundle {
        def,
        rules,
        rule_arcs: OnceLock::new(),
    })
}

fn sanitize_id(id: &str, idx: usize) -> Result<String, String> {
    let id = id.trim();
    if id.is_empty() {
        return Ok(format!("r{}", idx + 1));
    }
    if id.len() > MAX_ID
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!("规则 id 非法: {id}"));
    }
    Ok(id.to_string())
}

fn sanitize_rule(r: HaRule, id: String) -> Result<HaRule, String> {
    if r.retries < 1 {
        return Err("最大备用切换次数至少为 1".into());
    }
    if r.retries > MAX_RETRIES {
        return Err(format!("最大备用切换次数不能超过 {MAX_RETRIES}"));
    }
    let err = match r.err.as_str() {
        "last" => "last",
        "first" => "first",
        _ => return Err("终态错误只能是 first 或 last".into()),
    };
    let mut melt = HashMap::new();
    for (k, secs) in r.melt {
        if secs == 0 {
            continue;
        }
        if melt.len() >= MAX_MELT {
            return Err(format!("单条规则熔断项最多 {MAX_MELT} 个"));
        }
        if !valid_melt_key(&k) {
            return Err(format!("熔断键非法: {k}"));
        }
        melt.insert(k, secs);
    }
    let name = {
        let n = r.name.trim();
        if n.is_empty() {
            "未命名".to_string()
        } else {
            n.chars().take(MAX_NAME).collect()
        }
    };
    let budget = if r.budget == 0 {
        default_budget()
    } else {
        r.budget
    };
    let ttfb = r.ttfb.min(300);
    Ok(HaRule {
        id,
        name,
        retries: r.retries,
        budget,
        ttfb,
        err: err.to_string(),
        melt,
        allow: normalize_keywords(r.allow),
        deny: normalize_keywords(r.deny),
    })
}

fn valid_melt_key(k: &str) -> bool {
    if k == "5xx" {
        return true;
    }
    k.parse::<u16>()
        .ok()
        .is_some_and(|n| (100..600).contains(&n))
}

pub async fn load_from_db(db: &Database) -> HaRulesBundle {
    let rows: Vec<(String, String)> =
        sqlx::query_as(&db.format_query(
            "SELECT config_key, config_value FROM plugin_configs WHERE plugin_name = ?",
        ))
        .bind("high_availability_channel")
        .fetch_all(&db.pool)
        .await
        .unwrap_or_default();
    let map: HashMap<String, String> = rows.into_iter().collect();
    if let Some(raw) = map.get("ha_rules") {
        match serde_json::from_str::<HaRulesBundle>(raw) {
            Ok(b) => match sanitize_bundle(b) {
                Ok(s) => return s,
                Err(e) => tracing::warn!("[HA] ha_rules 校验失败，回落旧配置: {}", e),
            },
            Err(e) => tracing::warn!("[HA] ha_rules JSON 解析失败，回落旧配置: {}", e),
        }
    }
    migrate_flat(&map)
}

fn parse_i64(map: &HashMap<String, String>, key: &str, default: i64) -> i64 {
    map.get(key)
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(default)
        .max(0)
}

fn migrate_flat(map: &HashMap<String, String>) -> HaRulesBundle {
    let mut melt = HashMap::new();
    let put = |melt: &mut HashMap<String, u32>, k: &str, secs: i64| {
        if secs > 0 {
            melt.insert(k.to_string(), secs as u32);
        }
    };
    put(&mut melt, "429", parse_i64(map, "ha_cooldown_429", 60));
    let auth = parse_i64(map, "ha_cooldown_auth", 1800);
    put(&mut melt, "401", auth);
    put(&mut melt, "402", auth);
    put(&mut melt, "404", parse_i64(map, "ha_cooldown_404", 3));
    put(&mut melt, "5xx", parse_i64(map, "ha_cooldown_network", 300));
    let keywords = |key: &str| -> Vec<String> {
        map.get(key)
            .and_then(|v| serde_json::from_str(v).ok())
            .map(normalize_keywords)
            .unwrap_or_default()
    };
    let retries = parse_i64(map, "ha_max_retries", 3)
        .max(1)
        .min(MAX_RETRIES as i64) as u32;
    let raw_budget = parse_i64(map, "ha_total_timeout_secs", 900) as u64;
    let budget = if raw_budget == 0 {
        default_budget()
    } else {
        raw_budget
    };
    let r = HaRule {
        id: "r1".into(),
        name: "通用对话".into(),
        retries,
        budget,
        ttfb: default_ttfb(),
        err: default_err(),
        melt,
        allow: keywords("ha_meltdown_whitelist"),
        deny: keywords("ha_meltdown_blacklist"),
    };
    HaRulesBundle {
        def: r.id.clone(),
        rules: vec![r],
        rule_arcs: OnceLock::new(),
    }
}
