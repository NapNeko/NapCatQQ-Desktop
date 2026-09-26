//! MaiBot 两份配置文件的强类型镜像。
//!
//! `generated.rs` 由 `scripts/maibot/codegen.py` 从上游 pydantic 类生成：字段名、类型、默认值
//! 都照上游，缺的键按上游默认补（每个结构体都是容器级 `serde(default)`），和上游按缺省读的结果一致。
//! `defaults/` 是上游自己写出来的完整默认文件，实例还没有 `model_config.toml`（从没启动过）时
//! 按它读，也拿它当新文件的底稿，带着上游的注释。

mod generated;

pub use generated::*;

pub use crate::config_doc::IssueSink;

pub const DEFAULT_BOT_CONFIG: &str = include_str!("defaults/bot_config.toml");
pub const DEFAULT_MODEL_CONFIG: &str = include_str!("defaults/model_config.toml");

/// 读 `config/bot_config.toml`；没有文件按全默认。`[inner]` 版本号不进类型化配置
pub fn read_bot_config_file(text: Option<&str>) -> Result<MaiBotBotConfigFile, String> {
    let table = parse(text.unwrap_or(""), "bot_config.toml")?;
    typed(table, "bot_config.toml")
}

/// 读 `config/model_config.toml`；没有文件按上游首启会生成的那份读。
/// 注意 `MaiBotModelConfigFile::default()` 是类的默认值（提供商、模型都是空列表），不是首启那份：
/// 上游首启另走 `create_default_model_config` 塞了一组 DeepSeek 默认，这里用 `defaults/` 里的文件对上
pub fn read_model_config_file(text: Option<&str>) -> Result<MaiBotModelConfigFile, String> {
    let mut table = parse(text.unwrap_or(DEFAULT_MODEL_CONFIG), "model_config.toml")?;
    for model in models_mut(&mut table) {
        if let Some(v) = model.get_mut("extra_params")
            && let toml::Value::Table(t) = v
        {
            *v = toml::Value::String(any_table::render(t));
        }
    }
    typed(table, "model_config.toml")
}

pub fn bot_config_to_toml(cfg: &MaiBotBotConfigFile) -> Result<toml::Table, String> {
    to_table(cfg)
}

/// 模型配置转成要写进文件的 TOML 值：`extra_params` 的文本转回表
pub fn model_config_to_toml(cfg: &MaiBotModelConfigFile) -> Result<toml::Table, String> {
    let mut table = to_table(cfg)?;
    for (i, model) in models_mut(&mut table).enumerate() {
        if let Some(toml::Value::String(text)) = model.get("extra_params") {
            let parsed =
                any_table::to_value(text).map_err(|e| format!("models/{i}/extra_params：{e}"))?;
            model.insert("extra_params".into(), toml::Value::Table(parsed));
        }
    }
    Ok(table)
}

fn to_table<T: serde::Serialize>(value: &T) -> Result<toml::Table, String> {
    match toml::Value::try_from(value).map_err(|e| e.to_string())? {
        toml::Value::Table(t) => Ok(t),
        _ => Err("配置序列化结果不是表".to_string()),
    }
}

fn models_mut(table: &mut toml::Table) -> impl Iterator<Item = &mut toml::Table> {
    table
        .get_mut("models")
        .and_then(toml::Value::as_array_mut)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_table_mut)
}

fn parse(text: &str, what: &str) -> Result<toml::Table, String> {
    toml::from_str::<toml::Table>(text).map_err(|e| format!("{what} 不是合法的 TOML：{e}"))
}

fn typed<T: serde::de::DeserializeOwned>(mut table: toml::Table, what: &str) -> Result<T, String> {
    table.remove("inner");
    toml::Value::Table(table)
        .try_into::<T>()
        .map_err(|e| format!("{what} 里有值的类型不对：{e}"))
}

/// 上游 `dict[str, Any]`（模型的 `extra_params`）：文件里是表，界面上当一段行内 TOML 文本编辑
pub mod any_table {
    pub fn render(table: &toml::Table) -> String {
        if table.is_empty() {
            return "{}".to_string();
        }
        toml::Value::Table(table.clone()).to_string()
    }

    /// 界面给的文本 → 表；空文本当空表
    pub fn to_value(text: &str) -> Result<toml::Table, String> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(toml::Table::new());
        }
        let wrapped = format!("v = {text}");
        let mut root: toml::Table =
            toml::from_str(&wrapped).map_err(|e| format!("不是合法的行内 TOML 表：{e}"))?;
        match root.remove("v") {
            Some(toml::Value::Table(t)) => Ok(t),
            _ => Err("要写成 { 键 = 值 } 这样的表".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_default_files_read_back_as_the_generated_defaults() {
        let bot = read_bot_config_file(Some(DEFAULT_BOT_CONFIG)).unwrap();
        assert_eq!(bot, MaiBotBotConfigFile::default(), "上游写出来的默认文件应该原样读回生成的默认值");
        assert_eq!(read_bot_config_file(None).unwrap(), bot);
        let model = read_model_config_file(None).unwrap();
        assert_eq!(model, read_model_config_file(Some(DEFAULT_MODEL_CONFIG)).unwrap());
        assert!(!model.api_providers.is_empty() && !model.models.is_empty());
    }

    #[test]
    fn seeded_minimal_bot_config_fills_the_rest_from_defaults() {
        let seed = "[inner]\nversion = \"8.14.40\"\n\n[webui]\nport = 29950\n\n[maim_message]\nws_server_port = 29951\n";
        let bot = read_bot_config_file(Some(seed)).unwrap();
        assert_eq!(bot.webui.port, 29950);
        assert_eq!(bot.maim_message.ws_server_port, 29951);
        let d = MaiBotBotConfigFile::default();
        assert_eq!(bot.personality, d.personality);
        assert_eq!(bot.chat.reply_timing.talk_value_rules.len(), 2, "默认两条频率规则");
    }

    #[test]
    fn floats_written_as_integers_are_accepted() {
        let bot = read_bot_config_file(Some("[chat.reply_timing]\ntalk_value = 0\n")).unwrap();
        assert_eq!(bot.chat.reply_timing.talk_value, 0.0);
    }

    #[test]
    fn partial_list_items_take_item_defaults() {
        let text = "[[api_providers]]\nname = \"p\"\nbase_url = \"https://x\"\napi_key = \"k\"\n\n[[models]]\nmodel_identifier = \"m\"\nname = \"m\"\napi_provider = \"p\"\n";
        let model = read_model_config_file(Some(text)).unwrap();
        assert_eq!(model.api_providers.len(), 1, "用户写了就整组用用户的");
        assert_eq!(model.api_providers[0].max_retry, 3);
        assert_eq!(model.models[0].extra_params, "{}");
    }

    #[test]
    fn extra_params_round_trip_through_text() {
        let model = read_model_config_file(None).unwrap();
        let think = model.models.iter().find(|m| m.name == "deepseek-v4-pro-think").unwrap();
        let table = any_table::to_value(&think.extra_params).unwrap();
        assert_eq!(table["thinking"]["type"].as_str(), Some("enabled"));
        assert_eq!(table["reasoning_effort"].as_str(), Some("high"));
        assert!(any_table::to_value("{").is_err());
        assert!(any_table::to_value("").unwrap().is_empty());

        let back = model_config_to_toml(&model).unwrap();
        let first = back["models"].as_array().unwrap()[0].as_table().unwrap();
        assert!(first["extra_params"].is_table(), "写回文件时要是表：{first:?}");
    }

    #[test]
    fn one_edit_on_the_upstream_file_changes_exactly_one_line() {
        let before = read_bot_config_file(Some(DEFAULT_BOT_CONFIG)).unwrap();
        let mut after = before.clone();
        after.chat.reply_timing.talk_value = 0.35;
        after.expression.learning_list[0].learn = false;
        let mut doc: toml_edit::DocumentMut = DEFAULT_BOT_CONFIG.parse().unwrap();
        let changed = crate::toml_patch::apply(
            &mut doc,
            &bot_config_to_toml(&before).unwrap(),
            &bot_config_to_toml(&after).unwrap(),
            &crate::toml_patch::no_identity,
        );
        assert_eq!(changed, vec!["chat.reply_timing.talk_value", "expression.learning_list"]);
        let out = doc.to_string();
        let diff: Vec<(&str, &str)> = DEFAULT_BOT_CONFIG
            .lines()
            .zip(out.lines())
            .filter(|(a, b)| a != b)
            .collect();
        assert_eq!(DEFAULT_BOT_CONFIG.lines().count(), out.lines().count());
        assert_eq!(diff.len(), 2, "{diff:#?}");
        assert!(diff[0].1.starts_with("talk_value = 0.35 # "), "行尾注释要留着：{:?}", diff[0].1);
        assert!(diff[1].1.contains("learn = false"), "{:?}", diff[1].1);
        assert_eq!(read_bot_config_file(Some(&out)).unwrap(), after);
    }

    /// 前端 mock 和读到配置前的占位要一份完整默认值；从这里导出，和 IPC 上的序列化逐字一致。
    /// 名字带 export_bindings_ 前缀，`pnpm run ts-bindings` 会顺带重写
    #[test]
    fn export_bindings_maibot_defaults() {
        let out = serde_json::json!({
            "bot": MaiBotBotConfigFile::default(),
            "models": read_model_config_file(None).unwrap(),
        });
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../src-ui/core/domain/apps/maibotDefaults.json"
        );
        std::fs::write(path, serde_json::to_string_pretty(&out).unwrap() + "\n").unwrap();
    }

    #[test]
    fn keyword_named_fields_keep_their_toml_names() {
        let bot = read_bot_config_file(None).unwrap();
        let table = bot_config_to_toml(&bot).unwrap();
        let item = &table["expression"]["learning_list"].as_array().unwrap()[0];
        assert!(item.get("type").is_some() && item.get("use").is_some(), "{item:?}");
    }
}
