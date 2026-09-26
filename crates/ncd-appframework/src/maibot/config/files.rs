//! 两份主配置停止时的写法：现文件 + 改前改后的类型化配置 → 只写改了的键。
//! 运行中不走这里，走麦麦自己的 WebUI 接口（它自己校验、合并、写盘、热加载，和它自己的写入不打架）。

use toml_edit::DocumentMut;

use super::super::schema::{
    DEFAULT_MODEL_CONFIG, MaiBotBotConfigFile, MaiBotModelConfigFile, bot_config_to_toml,
    model_config_to_toml,
};
use crate::toml_patch;

/// MCP 服务按名字认：换了顺序、删了中间一个，其余条目里 Desktop 不认识的键还在
pub fn bot_identity(path: &[&str]) -> Option<&'static str> {
    match path {
        ["mcp", "servers"] => Some("name"),
        _ => None,
    }
}

/// 提供商和模型按名字认，理由同上
pub fn model_identity(path: &[&str]) -> Option<&'static str> {
    match path {
        ["api_providers"] | ["models"] => Some("name"),
        _ => None,
    }
}

fn parse(text: &str, what: &str) -> Result<DocumentMut, String> {
    text.parse::<DocumentMut>()
        .map_err(|e| format!("{what} 不是合法的 TOML：{e}"))
}

/// 返回新文本；没有改动返回 None（不写盘，免得版本号跳了白白提示重启）。
/// 文件不在时从只有 `[inner].version` 的空文档起，写出来的只有和默认值不同的键
pub fn patch_bot_config(
    current: Option<&str>,
    config_version: &str,
    before: &MaiBotBotConfigFile,
    after: &MaiBotBotConfigFile,
) -> Result<Option<String>, String> {
    let base = match current {
        Some(text) => text.to_string(),
        None => format!("[inner]\nversion = \"{config_version}\"\n"),
    };
    let mut doc = parse(&base, "bot_config.toml")?;
    let changed = toml_patch::apply(
        &mut doc,
        &bot_config_to_toml(before)?,
        &bot_config_to_toml(after)?,
        &bot_identity,
    );
    Ok((!changed.is_empty()).then(|| doc.to_string()))
}

/// 同上。实例从没启动过、还没有 model_config.toml 时，拿上游默认文件当底稿（带着上游注释），
/// 版本号换成实例源码里的常量：用生成时的版本号，实例上游更新过的话会被当成旧文件整份重写
pub fn patch_model_config(
    current: Option<&str>,
    model_config_version: Option<&str>,
    before: &MaiBotModelConfigFile,
    after: &MaiBotModelConfigFile,
) -> Result<Option<String>, String> {
    let mut doc = match current {
        Some(text) => parse(text, "model_config.toml")?,
        None => {
            let mut doc = parse(DEFAULT_MODEL_CONFIG, "内置默认 model_config.toml")?;
            if let Some(version) = model_config_version
                && let Some(inner) = doc.get_mut("inner").and_then(toml_edit::Item::as_table_like_mut)
                && let Some(v) = inner.get_mut("version").and_then(toml_edit::Item::as_value_mut)
            {
                let decor = v.decor().clone();
                *v = version.into();
                *v.decor_mut() = decor;
            }
            doc
        }
    };
    let changed = toml_patch::apply(
        &mut doc,
        &model_config_to_toml(before)?,
        &model_config_to_toml(after)?,
        &model_identity,
    );
    Ok((!changed.is_empty()).then(|| doc.to_string()))
}

#[cfg(test)]
mod tests {
    use super::super::super::schema::{DEFAULT_BOT_CONFIG, read_bot_config_file, read_model_config_file};
    use super::*;

    #[test]
    fn untouched_config_writes_nothing() {
        let bot = read_bot_config_file(Some(DEFAULT_BOT_CONFIG)).unwrap();
        assert_eq!(patch_bot_config(Some(DEFAULT_BOT_CONFIG), "8.14.40", &bot, &bot).unwrap(), None);
        let models = read_model_config_file(None).unwrap();
        assert_eq!(patch_model_config(None, Some("1.17.9"), &models, &models).unwrap(), None);
    }

    #[test]
    fn seeded_file_only_gains_the_changed_keys() {
        let seed = "[inner]\nversion = \"8.14.40\"\n\n[webui]\nport = 29950\n\n[maim_message]\nws_server_port = 29951\n";
        let before = read_bot_config_file(Some(seed)).unwrap();
        let mut after = before.clone();
        after.bot.nickname = "小麦".into();
        let out = patch_bot_config(Some(seed), "8.14.40", &before, &after).unwrap().unwrap();
        assert!(out.starts_with(seed), "原有内容一字不动：{out}");
        assert!(out.contains("[bot]\nnickname = \"小麦\""), "{out}");
        assert_eq!(read_bot_config_file(Some(&out)).unwrap(), after);
    }

    #[test]
    fn missing_model_file_starts_from_upstream_defaults_with_instance_version() {
        let before = read_model_config_file(None).unwrap();
        let mut after = before.clone();
        after.api_providers[0].api_key = "sk-test".into();
        let out = patch_model_config(None, Some("1.18.0"), &before, &after).unwrap().unwrap();
        let root: toml::Table = toml::from_str(&out).unwrap();
        assert_eq!(root["inner"]["version"].as_str(), Some("1.18.0"));
        assert!(out.contains("# 模型标识符"), "上游注释要带着：{}", &out[..200.min(out.len())]);
        assert_eq!(read_model_config_file(Some(&out)).unwrap(), after);
    }

    #[test]
    fn providers_are_matched_by_name_when_reordered() {
        let text = "[inner]\nversion = \"1.17.9\"\n\n[[api_providers]]\nname = \"a\"\nbase_url = \"https://a\"\napi_key = \"k\"\nnote = \"留着\"\n\n[[api_providers]]\nname = \"b\"\nbase_url = \"https://b\"\napi_key = \"k\"\n\n[[models]]\nmodel_identifier = \"m\"\nname = \"m\"\napi_provider = \"a\"\n";
        let before = read_model_config_file(Some(text)).unwrap();
        let mut after = before.clone();
        after.api_providers.reverse();
        let out = patch_model_config(Some(text), None, &before, &after).unwrap().unwrap();
        let root: toml::Table = toml::from_str(&out).unwrap();
        let providers = root["api_providers"].as_array().unwrap();
        assert_eq!(providers[0]["name"].as_str(), Some("b"));
        assert_eq!(providers[1]["note"].as_str(), Some("留着"), "{out}");
    }
}
