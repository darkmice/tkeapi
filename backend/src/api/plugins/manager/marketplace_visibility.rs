/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 模型广场单模型可见性：
//! - 广场展示开启：`level_ids` 为不可查看的用户等级，空 = 全部可查看
//! - 广场展示关闭：`level_ids` 为可查看的用户等级，空 = 全部不可查看
//! - `MP_GUEST_LEVEL_ID`（-1）表示未登录游客。基本配置允许免登录时，仍可按该虚拟等级单独挡住游客。

use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// 未登录游客的虚拟等级。真实 `user_levels.id` 为 BIGSERIAL，从 1 起，不会与 -1 冲突。
pub const MP_GUEST_LEVEL_ID: i64 = -1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarketplaceViewer {
    pub is_admin: bool,
    pub level_id: Option<i64>,
}

impl MarketplaceViewer {
    /// 未登录（或 token 无效）。已登录但没有匹配等级的用户不是游客，其 `level_id` 保持 `None`。
    pub fn guest() -> Self {
        Self {
            is_admin: false,
            level_id: Some(MP_GUEST_LEVEL_ID),
        }
    }
}

pub fn json_to_i64(v: &Value) -> Option<i64> {
    v.as_i64()
        .or_else(|| v.as_u64().map(|n| n as i64))
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

pub fn parse_level_id_list(v: Option<&Value>) -> Vec<i64> {
    v.and_then(|x| x.as_array())
        .map(|arr| arr.iter().filter_map(json_to_i64).collect())
        .unwrap_or_default()
}

pub fn parse_mp_level_ids(conf: &Value) -> Vec<i64> {
    parse_level_id_list(conf.get("level_ids"))
}

pub fn mp_enabled(conf: &Value, default_enabled: bool) -> bool {
    conf.get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(default_enabled)
}

pub fn mp_is_candidate(enabled: bool, level_ids: &[i64]) -> bool {
    enabled || !level_ids.is_empty()
}

#[allow(dead_code)]
pub fn mp_conf_is_candidate(conf: &Value, default_enabled: bool) -> bool {
    let enabled = mp_enabled(conf, default_enabled);
    let level_ids = parse_mp_level_ids(conf);
    mp_is_candidate(enabled, &level_ids)
}

/// 当前观察者是否可见该模型。
pub fn mp_visible_to(enabled: bool, level_ids: &[i64], viewer: MarketplaceViewer) -> bool {
    if viewer.is_admin {
        return mp_is_candidate(enabled, level_ids);
    }
    match viewer.level_id {
        None => enabled,
        Some(id) => {
            if enabled {
                !level_ids.contains(&id)
            } else {
                level_ids.contains(&id)
            }
        }
    }
}

pub fn deserialize_i64_flexible<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let v = Value::deserialize(deserializer)?;
    json_to_i64(&v).ok_or_else(|| serde::de::Error::custom("invalid i64 integer"))
}

pub fn deserialize_opt_i64_flexible<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<Value>::deserialize(deserializer)?;
    match opt {
        Some(v) => Ok(json_to_i64(&v)),
        None => Ok(None),
    }
}

pub fn deserialize_vec_i64_flexible<'de, D>(deserializer: D) -> Result<Vec<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    let v = Option::<Value>::deserialize(deserializer)?;
    Ok(parse_level_id_list(v.as_ref()))
}

pub fn deserialize_opt_vec_i64_flexible<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<i64>>, D::Error>
where
    D: Deserializer<'de>,
{
    let v = Option::<Value>::deserialize(deserializer)?;
    Ok(v.filter(|val| !val.is_null())
        .map(|val| parse_level_id_list(Some(&val))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_json_to_i64() {
        assert_eq!(json_to_i64(&json!(123456789012345_i64)), Some(123456789012345_i64));
        assert_eq!(json_to_i64(&json!("9876543210")), Some(9876543210_i64));
        assert_eq!(json_to_i64(&json!("invalid")), None);
        assert_eq!(json_to_i64(&json!(null)), None);
    }

    #[test]
    fn test_parse_mp_level_ids() {
        let conf = json!({
            "level_ids": [1, "2", 3000000000_i64]
        });
        assert_eq!(parse_mp_level_ids(&conf), vec![1, 2, 3000000000_i64]);
    }

    #[test]
    fn test_mp_visible_to() {
        let admin = MarketplaceViewer { is_admin: true, level_id: None };
        let guest = MarketplaceViewer::guest();
        let user_lv1 = MarketplaceViewer { is_admin: false, level_id: Some(1) };
        let user_lv2 = MarketplaceViewer { is_admin: false, level_id: Some(2) };

        // 开启展示，level_ids 为排除名单
        let level_ids = vec![2];
        assert!(mp_visible_to(true, &level_ids, admin));
        assert!(mp_visible_to(true, &level_ids, guest));
        assert!(mp_visible_to(true, &level_ids, user_lv1));
        assert!(!mp_visible_to(true, &level_ids, user_lv2));

        // 关闭展示，level_ids 为白名单
        assert!(mp_visible_to(false, &level_ids, admin));
        assert!(!mp_visible_to(false, &level_ids, guest));
        assert!(!mp_visible_to(false, &level_ids, user_lv1));
        assert!(mp_visible_to(false, &level_ids, user_lv2));
    }

    #[test]
    fn test_guest_level_is_selectable() {
        let guest = MarketplaceViewer::guest();
        let user_lv1 = MarketplaceViewer { is_admin: false, level_id: Some(1) };
        let logged_in_without_level = MarketplaceViewer { is_admin: false, level_id: None };
        assert_eq!(guest.level_id, Some(MP_GUEST_LEVEL_ID));

        let blocked = vec![MP_GUEST_LEVEL_ID];
        // 开启展示：游客在排除名单中不可见；已登录用户不受该虚拟等级影响
        assert!(!mp_visible_to(true, &blocked, guest));
        assert!(mp_visible_to(true, &blocked, user_lv1));
        assert!(mp_visible_to(true, &blocked, logged_in_without_level));
        // 关闭展示：游客在白名单中可见
        assert!(mp_visible_to(false, &blocked, guest));
        assert!(!mp_visible_to(false, &blocked, user_lv1));
    }

    #[derive(Deserialize, Debug, PartialEq)]
    struct DummyConfig {
        #[serde(deserialize_with = "deserialize_i64_flexible")]
        id: i64,
        #[serde(default, deserialize_with = "deserialize_opt_i64_flexible")]
        sort_order: Option<i64>,
        #[serde(default, deserialize_with = "deserialize_vec_i64_flexible")]
        level_ids: Vec<i64>,
        #[serde(default, deserialize_with = "deserialize_opt_vec_i64_flexible")]
        opt_level_ids: Option<Vec<i64>>,
    }

    #[test]
    fn test_flexible_deserializers() {
        let json_data = json!({
            "id": "123456789012345",
            "sort_order": "99",
            "level_ids": ["1", 2, "3000000000"],
            "opt_level_ids": ["-1", 5]
        });
        let parsed: DummyConfig = serde_json::from_value(json_data).unwrap();
        assert_eq!(
            parsed,
            DummyConfig {
                id: 123456789012345_i64,
                sort_order: Some(99),
                level_ids: vec![1, 2, 3000000000_i64],
                opt_level_ids: Some(vec![-1, 5]),
            }
        );

        let json_numeric = json!({
            "id": 8888,
        });
        let parsed_numeric: DummyConfig = serde_json::from_value(json_numeric).unwrap();
        assert_eq!(
            parsed_numeric,
            DummyConfig {
                id: 8888,
                sort_order: None,
                level_ids: vec![],
                opt_level_ids: None,
            }
        );
    }
}
