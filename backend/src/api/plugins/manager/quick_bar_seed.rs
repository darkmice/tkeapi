/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use serde_json::{json, Value};

const QUICK_BAR_MAX: usize = 3;

/// Imagine 快捷栏三类：比例 / 时长 / 分辨率
fn is_imagine_quick_bar_key(key: &str) -> bool {
    matches!(
        key.trim().to_ascii_lowercase().as_str(),
        "ratio"
            | "aspect_ratio"
            | "aspectratio"
            | "size"
            | "duration"
            | "duration_seconds"
            | "seconds"
            | "resolution"
    )
}

fn param_is_quick_bar_eligible(p: &Value) -> bool {
    let ty = p.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if ty != "radio" && ty != "select" {
        return false;
    }
    p.get("options")
        .and_then(|v| v.as_array())
        .is_some_and(|a| !a.is_empty())
}

fn param_quick(p: &Value) -> bool {
    p.get("quick").and_then(|v| v.as_bool()).unwrap_or(false)
}

/// 系统方案被内置模板覆盖时，按 key 把库里管理员勾过的快捷栏贴回去
pub(crate) fn overlay_saved_quick_flags(saved_params: &Value, merged_params: &mut Value) {
    let Some(saved) = saved_params.as_array() else {
        return;
    };
    let Some(merged) = merged_params.as_array_mut() else {
        return;
    };
    let mut flags = std::collections::HashMap::<String, bool>::new();
    for p in saved {
        let Some(key) = p.get("key").and_then(|v| v.as_str()) else {
            continue;
        };
        if let Some(q) = p.get("quick").and_then(|v| v.as_bool()) {
            flags.insert(key.to_string(), q);
        }
    }
    if flags.is_empty() {
        return;
    }
    for p in merged {
        let key = p.get("key").and_then(|v| v.as_str()).unwrap_or("");
        let Some(&q) = flags.get(key) else {
            continue;
        };
        if let Some(obj) = p.as_object_mut() {
            obj.insert("quick".into(), json!(q));
        }
    }
}

/// 图片/视频方案参数按列表顺序勾选 Imagine 三类快捷栏，最多 3 个
pub(crate) fn seed_imagine_quick_bar_params(params: &mut Value) -> bool {
    let Some(arr) = params.as_array_mut() else {
        return false;
    };
    let already = arr.iter().filter(|p| param_quick(p)).count();
    let mut remain = QUICK_BAR_MAX.saturating_sub(already);
    if remain == 0 {
        return false;
    }
    let mut changed = false;
    for p in arr.iter_mut() {
        if remain == 0 {
            break;
        }
        if param_quick(p) {
            continue;
        }
        let key = p.get("key").and_then(|v| v.as_str()).unwrap_or("");
        if !is_imagine_quick_bar_key(key) || !param_is_quick_bar_eligible(p) {
            continue;
        }
        if let Some(obj) = p.as_object_mut() {
            obj.insert("quick".into(), json!(true));
            changed = true;
            remain -= 1;
        }
    }
    changed
}

pub(crate) fn seed_imagine_quick_bar_scheme(scheme: &mut Value) -> bool {
    let ty = scheme.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if ty != "image" && ty != "video" {
        return false;
    }
    let Some(params) = scheme.get_mut("params") else {
        return false;
    };
    seed_imagine_quick_bar_params(params)
}

pub(crate) fn seed_imagine_quick_bar_schemes(schemes: &mut [Value]) -> bool {
    let mut changed = false;
    for s in schemes.iter_mut() {
        if seed_imagine_quick_bar_scheme(s) {
            changed = true;
        }
    }
    changed
}

fn merged_quick_count(scheme_params: &[Value], overrides: &Value) -> usize {
    let removes: std::collections::HashSet<&str> = overrides
        .get("remove")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    let modifies = overrides.get("modify").and_then(|v| v.as_object());
    let mut n = 0;
    for p in scheme_params {
        let key = p.get("key").and_then(|v| v.as_str()).unwrap_or("");
        if key.is_empty() || removes.contains(key) {
            continue;
        }
        let patch_quick = modifies
            .and_then(|m| m.get(key))
            .and_then(|p| p.get("quick"))
            .and_then(|v| v.as_bool());
        if patch_quick.unwrap_or(param_quick(p)) {
            n += 1;
        }
    }
    if let Some(add) = overrides.get("add").and_then(|v| v.as_array()) {
        n += add.iter().filter(|p| param_quick(p)).count();
    }
    n
}

/// 模型微调：给 Imagine 三类参数勾快捷栏；与方案合并后仍不超过 3 个
pub(crate) fn seed_imagine_quick_bar_overrides(
    overrides: &mut Value,
    scheme_params: &[Value],
) -> bool {
    if !overrides.is_object() {
        return false;
    }
    let mut changed = false;

    let modify_keys: Vec<String> = overrides
        .get("modify")
        .and_then(|v| v.as_object())
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    for key in modify_keys {
        if merged_quick_count(scheme_params, overrides) >= QUICK_BAR_MAX {
            break;
        }
        if !is_imagine_quick_bar_key(&key) {
            continue;
        }
        let already = overrides
            .pointer(&format!("/modify/{key}/quick"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if already {
            continue;
        }
        let Some(patch) = overrides
            .get_mut("modify")
            .and_then(|v| v.as_object_mut())
            .and_then(|m| m.get_mut(&key))
            .and_then(|v| v.as_object_mut())
        else {
            continue;
        };
        patch.insert("quick".into(), json!(true));
        changed = true;
    }

    let add_len = overrides
        .get("add")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    for i in 0..add_len {
        if merged_quick_count(scheme_params, overrides) >= QUICK_BAR_MAX {
            break;
        }
        let Some(p) = overrides
            .get("add")
            .and_then(|v| v.as_array())
            .and_then(|a| a.get(i))
        else {
            break;
        };
        if param_quick(p) {
            continue;
        }
        let key = p.get("key").and_then(|v| v.as_str()).unwrap_or("");
        if !is_imagine_quick_bar_key(key) || !param_is_quick_bar_eligible(p) {
            continue;
        }
        if let Some(obj) = overrides
            .get_mut("add")
            .and_then(|v| v.as_array_mut())
            .and_then(|a| a.get_mut(i))
            .and_then(|v| v.as_object_mut())
        {
            obj.insert("quick".into(), json!(true));
            changed = true;
        }
    }

    changed
}
