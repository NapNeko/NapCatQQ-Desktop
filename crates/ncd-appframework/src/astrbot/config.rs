//! AstrBot 对接密钥读取与备份清单。

use serde_json::Value;

use super::config_json::cmd_config_documents;
use super::platform::{
    self, Claim, claim_for_probe, claim_for_upsert, ncd_platform_id, platforms,
    read_aiocqhttp_token,
};

pub fn astrbot_config_documents() -> Vec<ncd_domain::AppConfigDocument> {
    cmd_config_documents()
}

pub fn read_access_token_from_root(root: &Value, instance_id: &str, port: u16) -> Option<String> {
    let rows = platforms(root);
    let want_id = ncd_platform_id(instance_id);
    let idx = rows
        .iter()
        .position(|row| {
            platform::row_id(row) == Some(want_id.as_str()) && platform::is_aiocqhttp(row)
        })
        .or_else(|| {
            if port == 0 {
                return None;
            }
            match claim_for_upsert(rows, instance_id, port).ok()? {
                Claim::Index(i) => Some(i),
                Claim::Append => None,
            }
        })
        .or_else(|| claim_for_probe(rows))?;
    read_aiocqhttp_token(&rows[idx])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_token_does_not_depend_on_ai_configuration() {
        let root = serde_json::json!({
            "provider": "unsupported-future-shape",
            "platform": [{"id": "ncd-app:a", "type": "aiocqhttp", "ws_reverse_token": "token"}]
        });
        assert_eq!(
            read_access_token_from_root(&root, "a", 6199).as_deref(),
            Some("token")
        );
    }
}
