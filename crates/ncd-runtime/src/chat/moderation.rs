//! 成员管理在发送前重新读取角色，界面缓存不能替代 QQ 的权限判断。
use ncd_domain::bot_config::BackendType;
use ncd_domain::onebot_debug::{DebugCallOrigin, DebugCallRequest, DebugCallResult, DebugError};
use serde_json::{Value, json};

use super::ChatManager;

pub(super) fn is_member_action(action: &str) -> bool {
    matches!(action, "set_group_ban" | "set_group_kick")
}

fn invalid(message: &str) -> DebugError {
    DebugError::InvalidParams {
        message: message.to_owned(),
    }
}

fn peer(value: &Value) -> Result<u64, DebugError> {
    let number = value.as_u64().or_else(|| value.as_str()?.parse().ok());
    number
        .filter(|number| *number > 0 && *number <= 9_007_199_254_740_991)
        .ok_or_else(|| invalid("无效的群号或成员 QQ 号"))
}

fn allowed(actor: &str, member: &str, own: bool) -> bool {
    !own && matches!(member, "member" | "admin")
        && (actor == "owner" || (actor == "admin" && member == "member"))
}

fn normalized(action: &str, params: &Value, backend: BackendType) -> Result<Value, DebugError> {
    let group = peer(&params["group_id"])?;
    let member = peer(&params["user_id"])?;
    let wire = |number: u64| match backend {
        BackendType::SnowLuma => json!(number),
        _ => json!(number.to_string()),
    };
    match action {
        "set_group_ban" => {
            let duration = params["duration"]
                .as_u64()
                .filter(|duration| *duration <= 30 * 86_400)
                .ok_or_else(|| invalid("禁言时长应在 0 到 30 天之间"))?;
            Ok(json!({ "group_id": wire(group), "user_id": wire(member), "duration": duration }))
        }
        "set_group_kick" => Ok(json!({
            "group_id": wire(group), "user_id": wire(member), "reject_add_request": false,
        })),
        _ => Err(invalid("不支持的成员操作")),
    }
}

impl ChatManager {
    pub(super) async fn check_member_action(
        &self,
        request: &mut DebugCallRequest,
    ) -> Result<(), DebugError> {
        let target = self
            .targets()
            .await
            .into_iter()
            .find(|target| target.bot_id == request.bot_id && target.running)
            .ok_or(DebugError::BotNotRunning)?;
        let params = normalized(&request.action, &request.params, target.backend)?;
        let group = peer(&params["group_id"])?;
        let member = peer(&params["user_id"])?;
        if member == target.qq_id {
            return Err(invalid("不能对当前账号执行成员管理操作"));
        }
        let role = async |user: u64| {
            let wire = |number: u64| match target.backend {
                BackendType::SnowLuma => json!(number),
                _ => json!(number.to_string()),
            };
            let response = self
                .transport
                .call(DebugCallRequest {
                    request_id: uuid::Uuid::new_v4().to_string(),
                    bot_id: request.bot_id.clone(),
                    channel: request.channel.clone(),
                    action: "get_group_member_info".into(),
                    params: json!({ "group_id": wire(group), "user_id": wire(user), "no_cache": true }),
                    timeout_ms: Some(10_000),
                    origin: DebugCallOrigin::Other,
                })
                .await;
            match response.result {
                DebugCallResult::Err { error } => Err(error),
                DebugCallResult::Ok { outcome } if outcome.ok && !outcome.truncated => outcome.data
                    ["role"]
                    .as_str()
                    .filter(|role| matches!(*role, "owner" | "admin" | "member"))
                    .map(str::to_owned)
                    .ok_or_else(|| invalid("无法确认群成员身份，请刷新后重试")),
                _ => Err(invalid("无法读取群权限，未执行成员操作")),
            }
        };
        let (actor, member_role) = tokio::try_join!(role(target.qq_id), role(member))?;
        if !allowed(&actor, &member_role, false) {
            return Err(invalid("当前账号没有管理这位成员的权限"));
        }
        request.params = params;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn management_requires_a_higher_role_and_never_targets_self() {
        assert!(allowed("owner", "admin", false));
        assert!(allowed("admin", "member", false));
        for (actor, member) in [
            ("admin", "admin"),
            ("member", "member"),
            ("owner", "owner"),
            ("owner", "unknown"),
        ] {
            assert!(!allowed(actor, member, false));
        }
        assert!(!allowed("owner", "member", true));
    }

    #[test]
    fn member_actions_validate_duration_and_drop_unrelated_parameters() {
        let params =
            json!({ "group_id": "3", "user_id": "4", "duration": 600, "reject_add_request": true });
        let kick = normalized("set_group_kick", &params, BackendType::NapCat).unwrap();
        assert_eq!(
            kick,
            json!({ "group_id": "3", "user_id": "4", "reject_add_request": false })
        );
        let ban = normalized("set_group_ban", &params, BackendType::SnowLuma).unwrap();
        assert_eq!(ban, json!({ "group_id": 3, "user_id": 4, "duration": 600 }));
        for duration in [json!(-1), json!(2_592_001), json!(1.5)] {
            assert!(
                normalized(
                    "set_group_ban",
                    &json!({"group_id": 3, "user_id": 4, "duration": duration}),
                    BackendType::NapCat
                )
                .is_err()
            );
        }
        assert!(
            normalized(
                "set_group_kick",
                &json!({"group_id": 0, "user_id": 4}),
                BackendType::NapCat
            )
            .is_err()
        );
    }
}
