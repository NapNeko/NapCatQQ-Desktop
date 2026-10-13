// 只从可信消息事件提取直接管理指令。
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Refresh(Option<String>),
    Status,
    Ignore(String),
}

#[derive(Debug, Clone)]
pub struct Incoming {
    pub messenger: String,
    pub sender: u64,
    pub message_id: String,
    pub reply_to: Option<String>,
    pub at: i64,
    pub command: Command,
}

fn id(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_i64().map(|id| id.to_string()))
}

pub fn parse_event(
    messenger: &str,
    event: &Value,
    allowed_groups: &[u64],
    now: i64,
) -> Option<Incoming> {
    if event.get("post_type")?.as_str()? != "message" {
        return None;
    }
    let self_id = event.get("self_id")?.as_u64()?;
    if self_id.to_string() != messenger {
        return None;
    }
    let sender = event.get("user_id")?.as_u64()?;
    if sender == 0 || sender == self_id {
        return None;
    }
    let at = event.get("time")?.as_i64()?;
    if at > now.saturating_add(15) || now.saturating_sub(at) > 90 {
        return None;
    }
    let message_id = id(event.get("message_id")?)?;
    if message_id.is_empty() || message_id.len() > 100 {
        return None;
    }
    let group = match event.get("message_type")?.as_str()? {
        "private" => false,
        "group" if allowed_groups.contains(&event.get("group_id")?.as_u64()?) => true,
        _ => return None,
    };
    let mut text = String::new();
    let mut addressed = false;
    let mut reply_to = None;
    match event.get("message")? {
        Value::String(raw) if !group && !raw.contains("[CQ:") => text = raw.clone(),
        Value::Array(segments) if segments.len() <= 16 => {
            for segment in segments {
                let data = segment.get("data")?;
                match segment.get("type")?.as_str()? {
                    "text" => text.push_str(data.get("text")?.as_str()?),
                    "at" if id(data.get("qq")?).as_deref() == Some(messenger) => addressed = true,
                    "reply" if reply_to.is_none() => reply_to = id(data.get("id")?),
                    _ => return None,
                }
            }
        }
        _ => return None,
    }
    if text.len() > 256 || (group && !addressed) {
        return None;
    }
    let text = text.trim();
    let command = if text == "刷新二维码" {
        Command::Refresh(None)
    } else {
        let mut parts = text.split_whitespace();
        if parts.next()? != "/ncd" {
            return None;
        }
        let command = match parts.next()? {
            "状态" => Command::Status,
            "刷新" => Command::Refresh(parts.next().map(str::to_owned)),
            "忽略" => Command::Ignore(parts.next()?.to_owned()),
            _ => return None,
        };
        if parts.next().is_some() {
            return None;
        }
        command
    };
    Some(Incoming {
        messenger: messenger.to_owned(),
        sender,
        message_id,
        reply_to,
        at,
        command,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn event() -> Value {
        json!({"post_type":"message","self_id":10,"user_id":20,"time":100,
        "message_id":1,"message_type":"private","message":[{"type":"text","data":{"text":"/ncd 刷新 30"}}]})
    }
    #[test]
    fn rejects_replay_echo_forward_and_foreign_account() {
        assert!(parse_event("10", &event(), &[], 100).is_some());
        assert!(parse_event("11", &event(), &[], 100).is_none());
        assert!(parse_event("10", &event(), &[], 200).is_none());
        let mut e = event();
        e["post_type"] = json!("message_sent");
        assert!(parse_event("10", &e, &[], 100).is_none());
        e = event();
        e["message"][0]["type"] = json!("forward");
        assert!(parse_event("10", &e, &[], 100).is_none());
    }
    #[test]
    fn groups_require_both_allowlist_and_direct_at() {
        let mut e = event();
        e["message_type"] = json!("group");
        e["group_id"] = json!(99);
        assert!(parse_event("10", &e, &[99], 100).is_none());
        e["message"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type":"at","data":{"qq":"10"}}));
        assert!(parse_event("10", &e, &[], 100).is_none());
        assert!(parse_event("10", &e, &[99], 100).is_some());
    }
}
