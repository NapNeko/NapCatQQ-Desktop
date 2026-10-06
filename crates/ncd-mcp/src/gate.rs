//! 权限闸：按工具分级决定一次工具调用怎么走。
//!
//! 「需确认」用挑战-重放而不是 MCP elicitation：不假设客户端实现了 elicitation，
//! 任何客户端都能用「带 confirm_token 重放同一调用」完成确认。令牌绑（工具名, 参数指纹），
//! 一次性、两分钟有效；指纹不含 confirm_token 本身。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde_json::Value;
use sha2::{Digest, Sha256};

const CHALLENGE_TTL: Duration = Duration::from_secs(120);
pub(crate) const CHALLENGE_TTL_SECS: u64 = 120;

/// 一次调用的副作用分级。`call_action` 按目录里动作的分级现查，其余工具用静态表
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolSafety {
    ReadOnly,
    SideEffect,
    Dangerous,
}

/// 工具自带的静态分级；`call_action` 不走这张表（动作的分级在目录里）
pub(crate) fn static_safety(tool: &str) -> ToolSafety {
    match tool {
        "list_targets" | "list_channels" | "list_actions" | "describe_action"
        | "read_events" | "list_receivers" | "get_workspace" | "get_collections"
        | "query_history" | "get_history_entry" | "server_status"
        // 取消 / 退订只收掉发起方自己的在途操作，不再产生新的外部效果
        | "cancel_call" | "unsubscribe_events" => ToolSafety::ReadOnly,
        "clear_history" => ToolSafety::Dangerous,
        _ => ToolSafety::SideEffect,
    }
}

pub(crate) enum Decision {
    Allow,
    NeedConfirm { token: String },
    Reject { reason: String },
}

struct Challenge {
    fingerprint: [u8; 32],
    expires_at: Instant,
}

pub(crate) struct GateKeeper {
    allow_dangerous: AtomicBool,
    challenges: Mutex<HashMap<String, Challenge>>,
}

impl GateKeeper {
    pub fn new() -> Self {
        Self {
            allow_dangerous: AtomicBool::new(false),
            challenges: Mutex::new(HashMap::new()),
        }
    }

    pub fn set_allow_dangerous(&self, on: bool) {
        self.allow_dangerous.store(on, Ordering::SeqCst);
    }

    /// 带 `confirm_token` 的调用会在这里核销；没带 / 牌不对就只发新牌，不执行
    pub fn decide(&self, tool: &str, safety: ToolSafety, args: &Value) -> Decision {
        match safety {
            ToolSafety::ReadOnly => Decision::Allow,
            ToolSafety::Dangerous if !self.allow_dangerous.load(Ordering::SeqCst) => {
                Decision::Reject {
                    reason: format!(
                        "危险级操作默认不执行（{tool}）；设置 mcp.allowDangerous 打开后降级为「需确认」"
                    ),
                }
            }
            _ => self.check_or_issue(tool, args),
        }
    }

    /// 令牌只认参数完全一致的回放（指纹一致才核销）；过期、串参数、串工具的都换新牌
    fn check_or_issue(&self, tool: &str, args: &Value) -> Decision {
        let fingerprint = fingerprint(tool, args);
        let mut challenges = lock(&self.challenges);
        challenges.retain(|_, c| c.expires_at > Instant::now());

        if let Some(token) = args.get("confirm_token").and_then(Value::as_str) {
            let matched = challenges
                .get(token)
                .is_some_and(|c| c.fingerprint == fingerprint);
            if matched {
                challenges.remove(token);
                return Decision::Allow;
            }
        }

        let token = hex::encode(rand::random::<[u8; 16]>());
        challenges.insert(
            token.clone(),
            Challenge {
                fingerprint,
                expires_at: Instant::now() + CHALLENGE_TTL,
            },
        );
        Decision::NeedConfirm { token }
    }
}

/// 键序打乱不影响指纹：对象键排序后的紧凑 JSON 再哈希（confirm_token 不参与）
fn fingerprint(tool: &str, args: &Value) -> [u8; 32] {
    let mut without_token = args.clone();
    if let Value::Object(map) = &mut without_token {
        map.remove("confirm_token");
    }
    let canonical = canonical_json(&without_token);
    let digest = Sha256::digest(format!("{tool}\n{canonical}").as_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

/// 对象键排序后的紧凑 JSON（serde_json 开了 preserve_order，直接 to_string 不稳）
fn canonical_json(value: &Value) -> String {
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut entries: Vec<(&String, &Value)> = map.iter().collect();
                entries.sort_by(|a, b| a.0.cmp(b.0));
                Value::Object(
                    entries
                        .into_iter()
                        .map(|(k, v)| (k.clone(), sorted(v)))
                        .collect(),
                )
            }
            Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
            other => other.clone(),
        }
    }
    sorted(value).to_string()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn gate() -> GateKeeper {
        GateKeeper::new()
    }

    #[test]
    fn static_table_ranks_tools() {
        assert_eq!(static_safety("list_targets"), ToolSafety::ReadOnly);
        assert_eq!(static_safety("cancel_call"), ToolSafety::ReadOnly);
        assert_eq!(static_safety("unsubscribe_events"), ToolSafety::ReadOnly);
        assert_eq!(static_safety("test_channel"), ToolSafety::SideEffect);
        assert_eq!(static_safety("set_enabled"), ToolSafety::SideEffect);
        assert_eq!(static_safety("clear_history"), ToolSafety::Dangerous);
        // 没在表里的按有副作用处理
        assert_eq!(static_safety("something_new"), ToolSafety::SideEffect);
    }

    #[test]
    fn read_only_passes_without_token() {
        let g = gate();
        let decision = g.decide("list_targets", ToolSafety::ReadOnly, &json!({}));
        assert!(matches!(decision, Decision::Allow));
    }

    #[test]
    fn side_effect_needs_replay_with_fresh_token() {
        let g = gate();
        let args = json!({"bot_id": "1"});
        let token = match g.decide("test_channel", ToolSafety::SideEffect, &args) {
            Decision::NeedConfirm { token } => token,
            _ => panic!("首次调用应该发确认令牌"),
        };
        // 不带牌重放：还是发牌（换新牌）
        assert!(matches!(
            g.decide("test_channel", ToolSafety::SideEffect, &args),
            Decision::NeedConfirm { .. }
        ));
        // 带牌但参数变了：不放行
        let different = json!({"bot_id": "2", "confirm_token": token});
        assert!(matches!(
            g.decide("test_channel", ToolSafety::SideEffect, &different),
            Decision::NeedConfirm { .. }
        ));
        // 带牌重放同一调用：放行，且令牌作废
        let replay = json!({"bot_id": "1", "confirm_token": token});
        assert!(matches!(
            g.decide("test_channel", ToolSafety::SideEffect, &replay),
            Decision::Allow
        ));
        assert!(matches!(
            g.decide("test_channel", ToolSafety::SideEffect, &replay),
            Decision::NeedConfirm { .. }
        ));
    }

    #[test]
    fn dangerous_rejected_by_default_and_confirmed_when_allowed() {
        let g = gate();
        assert!(matches!(
            g.decide("clear_history", ToolSafety::Dangerous, &json!({})),
            Decision::Reject { .. }
        ));
        g.set_allow_dangerous(true);
        let decision = g.decide("clear_history", ToolSafety::Dangerous, &json!({}));
        assert!(matches!(decision, Decision::NeedConfirm { .. }));
    }

    #[test]
    fn fingerprint_ignores_key_order_and_confirm_token() {
        let a = fingerprint("tool", &json!({"x": 1, "y": {"a": [1, 2], "b": 2}}));
        let b = fingerprint(
            "tool",
            &json!({"y": {"b": 2, "a": [1, 2]}, "x": 1, "confirm_token": "z"}),
        );
        assert_eq!(a, b);
        let other_tool = fingerprint("other", &json!({"x": 1, "y": {"a": [1, 2], "b": 2}}));
        assert_ne!(a, other_tool);
    }
}
