//! 桌面控制的只读权限与一次性调用挑战；挑战不能证明用户已经批准。

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

const CHALLENGE_TTL: Duration = Duration::from_secs(120);
pub(crate) const CHALLENGE_TTL_SECS: u64 = 120;
const MAX_CHALLENGES: usize = 256;

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
        "list_targets"
        | "list_channels"
        | "list_actions"
        | "describe_action"
        | "read_events"
        | "list_receivers"
        | "get_workspace"
        | "get_collections"
        | "query_history"
        | "get_history_entry"
        | "server_status"
        | "get_security_policy"
        | "search_actions"
        | "diagnose_bot"
        | "get_bot_runtime"
        | "tail_bot_logs"
        | "list_app_instances"
        | "get_app_runtime"
        | "list_servers"
        | "list_component_tasks" => ToolSafety::ReadOnly,
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
    state: Mutex<GateState>,
}

struct GateState {
    read_only: bool,
    allow_dangerous: bool,
    challenges: HashMap<String, Challenge>,
    subscriptions: HashSet<String>,
    generation: u64,
    cancellation: CancellationToken,
}

#[derive(Clone)]
pub(crate) struct AuthorizationEpoch {
    pub generation: u64,
    cancellation: CancellationToken,
}

impl AuthorizationEpoch {
    pub async fn cancelled(&self) {
        self.cancellation.cancelled().await;
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }
}

impl GateState {
    fn revoke(&mut self) {
        self.challenges.clear();
        self.generation = self.generation.wrapping_add(1);
        self.cancellation.cancel();
        self.cancellation = CancellationToken::new();
    }
}

impl GateKeeper {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(GateState {
                read_only: true,
                allow_dangerous: false,
                challenges: HashMap::new(),
                subscriptions: HashSet::new(),
                generation: 0,
                cancellation: CancellationToken::new(),
            }),
        }
    }

    #[cfg(test)]
    pub fn set_allow_dangerous(&self, on: bool) {
        let mut state = lock(&self.state);
        if state.allow_dangerous != on {
            state.allow_dangerous = on;
            state.revoke();
        }
    }

    #[cfg(test)]
    pub fn set_read_only(&self, on: bool) {
        let mut state = lock(&self.state);
        if state.read_only != on {
            state.read_only = on;
            state.revoke();
        }
    }

    pub fn set_policy(&self, read_only: bool, allow_dangerous: bool) -> bool {
        let mut state = lock(&self.state);
        let changed = state.read_only != read_only || state.allow_dangerous != allow_dangerous;
        if changed {
            state.read_only = read_only;
            state.allow_dangerous = allow_dangerous;
            state.revoke();
        }
        changed
    }

    pub fn read_only(&self) -> bool {
        lock(&self.state).read_only
    }

    pub fn allow_dangerous(&self) -> bool {
        lock(&self.state).allow_dangerous
    }

    pub fn reset(&self) {
        lock(&self.state).revoke();
    }

    pub fn generation(&self) -> u64 {
        lock(&self.state).generation
    }

    pub fn epoch(&self) -> AuthorizationEpoch {
        let state = lock(&self.state);
        AuthorizationEpoch {
            generation: state.generation,
            cancellation: state.cancellation.clone(),
        }
    }

    pub fn register_subscription(&self, id: String, generation: u64) -> Result<(), String> {
        let mut state = lock(&self.state);
        if generation != state.generation || state.subscriptions.len() >= 32 {
            return Err("MCP 订阅已失效或已达32个上限".to_owned());
        }
        state.subscriptions.insert(id);
        Ok(())
    }

    pub fn remove_subscription(&self, id: &str) -> bool {
        lock(&self.state).subscriptions.remove(id)
    }

    pub fn take_subscriptions(&self) -> Vec<String> {
        lock(&self.state).subscriptions.drain().collect()
    }

    /// 带 `confirm_token` 的调用会在这里核销；没带 / 牌不对就只发新牌，不执行
    #[cfg(test)]
    pub fn decide(&self, tool: &str, safety: ToolSafety, args: &Value) -> Decision {
        self.decide_at_generation(tool, safety, args, self.generation())
    }

    pub fn decide_at_generation(
        &self,
        tool: &str,
        safety: ToolSafety,
        args: &Value,
        generation: u64,
    ) -> Decision {
        let fingerprint = fingerprint(tool, args);
        let mut state = lock(&self.state);
        if state.generation != generation {
            return Decision::Reject {
                reason: "MCP 权限已改变，请重新发起调用".to_owned(),
            };
        }
        match safety {
            ToolSafety::ReadOnly => Decision::Allow,
            _ if state.read_only => Decision::Reject {
                reason: "MCP 当前为只读模式；写操作只能由用户在桌面设置中开启".to_owned(),
            },
            ToolSafety::Dangerous if !state.allow_dangerous => Decision::Reject {
                reason: format!(
                    "危险级操作默认不执行（{tool}）；设置 mcp.allowDangerous 打开后降级为「需确认」"
                ),
            },
            _ => Self::check_or_issue(&mut state, fingerprint, args),
        }
    }

    /// 令牌只认参数完全一致的回放（指纹一致才核销）；过期、串参数、串工具的都换新牌
    fn check_or_issue(state: &mut GateState, fingerprint: [u8; 32], args: &Value) -> Decision {
        let challenges = &mut state.challenges;
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

        if challenges.len() >= MAX_CHALLENGES {
            return Decision::Reject {
                reason: "待确认调用过多，请等待两分钟后重试".to_owned(),
            };
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
        assert_eq!(static_safety("cancel_call"), ToolSafety::SideEffect);
        assert_eq!(static_safety("unsubscribe_events"), ToolSafety::SideEffect);
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
        g.set_read_only(false);
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
        g.set_read_only(false);
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

    #[test]
    fn readonly_and_policy_changes_revoke_challenges() {
        let g = gate();
        assert!(matches!(
            g.decide("close_all", ToolSafety::SideEffect, &json!({})),
            Decision::Reject { .. }
        ));
        g.set_read_only(false);
        let Decision::NeedConfirm { token } =
            g.decide("close_all", ToolSafety::SideEffect, &json!({}))
        else {
            panic!("challenge");
        };
        g.set_read_only(true);
        g.set_read_only(false);
        assert!(matches!(
            g.decide(
                "close_all",
                ToolSafety::SideEffect,
                &json!({"confirm_token": token})
            ),
            Decision::NeedConfirm { .. }
        ));
    }

    #[test]
    fn challenge_storage_is_bounded() {
        let g = gate();
        g.set_read_only(false);
        for _ in 0..MAX_CHALLENGES {
            assert!(matches!(
                g.decide("close_all", ToolSafety::SideEffect, &json!({})),
                Decision::NeedConfirm { .. }
            ));
        }
        assert!(matches!(
            g.decide("close_all", ToolSafety::SideEffect, &json!({})),
            Decision::Reject { .. }
        ));
    }

    #[test]
    fn policy_updates_revoke_admission_and_subscription_generation() {
        let g = gate();
        let before = g.epoch();
        assert!(!g.set_policy(true, false));
        assert!(!before.is_cancelled());
        assert!(g.set_policy(false, true));
        assert!(before.is_cancelled());
        assert!(matches!(
            g.decide_at_generation(
                "list_targets",
                ToolSafety::ReadOnly,
                &json!({}),
                before.generation
            ),
            Decision::Reject { .. }
        ));
        assert!(
            g.register_subscription("stale".to_owned(), before.generation)
                .is_err()
        );
        let current = g.epoch();
        assert!(!current.is_cancelled());
        assert!(
            g.register_subscription("current".to_owned(), current.generation)
                .is_ok()
        );
        g.reset();
        assert!(current.is_cancelled());
        assert_eq!(g.take_subscriptions(), vec!["current".to_owned()]);
    }

    #[tokio::test]
    async fn reset_wakes_requests_waiting_on_old_epoch() {
        let g = gate();
        let epoch = g.epoch();
        g.reset();
        tokio::time::timeout(Duration::from_secs(1), epoch.cancelled())
            .await
            .unwrap();
        assert!(!g.epoch().is_cancelled());
    }
}
