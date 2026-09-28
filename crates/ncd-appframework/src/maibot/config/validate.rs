//! 保存前的校验：生成的范围 / 选项检查，加上上游 `model_post_init` 里的跨字段规则（条件照抄）。
//! 停止时写坏了文件麦麦下次直接起不来；运行中上游也会校验，但在这里拦住能指到具体哪一栏。
//!
//! 路径前缀：`bot/…` 是 bot_config，`models/…` 是 model_config，`adapter/…` 是适配器名单。

use std::collections::HashSet;

use ncd_domain::AppConfigIssue;

use super::super::schema::{
    IssueSink, MaiBotBotConfigFile, MaiBotKeywordRuleConfig, MaiBotModelConfigFile,
    MaiBotTaskConfig,
};
use super::MaiBotInstanceConfig;

pub fn validate(cfg: &MaiBotInstanceConfig) -> Vec<AppConfigIssue> {
    let mut sink = IssueSink::default();
    ports(&cfg.bot, &mut sink);
    cfg.bot.validate("bot", &mut sink);
    bot_rules(&cfg.bot, &mut sink);
    cfg.models.validate("models", &mut sink);
    model_rules(&cfg.models, &mut sink);
    if let Some(adapter) = &cfg.adapter {
        let chat = &adapter.chat;
        for (path, list, what) in [
            ("adapter/chat/group_list", &chat.group_list, "群号"),
            ("adapter/chat/private_list", &chat.private_list, "QQ 号"),
            ("adapter/chat/ban_user_id", &chat.ban_user_id, "QQ 号"),
        ] {
            if let Some(bad) = list
                .iter()
                .map(|s| s.trim())
                .find(|s| s.is_empty() || !s.chars().all(|c| c.is_ascii_digit()))
            {
                sink.push(path, format!("{what}只能是数字：{bad:?}"));
            }
        }
    }
    // 生成的范围检查和上游手写的规则有重叠，同一栏只报第一条
    let mut seen = HashSet::new();
    sink.into_vec()
        .into_iter()
        .filter(|i| seen.insert(i.path.clone()))
        .collect()
}

fn ports(bot: &MaiBotBotConfigFile, sink: &mut IssueSink) {
    let webui = bot.webui.port;
    let legacy = bot.maim_message.ws_server_port;
    sink.range("bot/webui/port", webui as f64, Some(1.0), Some(65535.0));
    sink.range(
        "bot/maim_message/ws_server_port",
        legacy as f64,
        Some(1.0),
        Some(65535.0),
    );
    if webui == legacy {
        sink.push(
            "bot/maim_message/ws_server_port",
            "不能和 WebUI 用同一个端口",
        );
    }
    if bot.maim_message.enable_api_server {
        let api = bot.maim_message.api_server_port;
        sink.range(
            "bot/maim_message/api_server_port",
            api as f64,
            Some(1.0),
            Some(65535.0),
        );
        if api == webui || api == legacy {
            sink.push(
                "bot/maim_message/api_server_port",
                "不能和 WebUI 或旧版消息服务用同一个端口",
            );
        }
    }
}

/// 贴近 Python `re`：支持环视和反向引用，免得把上游收得下的正则误判成错的
fn check_regex(sink: &mut IssueSink, path: &str, pattern: &str) {
    if let Err(e) = fancy_regex::Regex::new(pattern) {
        sink.push(path, format!("正则写得不对：{pattern:?}（{e}）"));
    }
}

fn keyword_rule(sink: &mut IssueSink, path: &str, rule: &MaiBotKeywordRuleConfig) {
    if rule.keywords.is_empty() && rule.regex.is_empty() {
        sink.push(format!("{path}/keywords"), "关键词和正则至少填一个");
    }
    if rule.reaction.trim().is_empty() {
        sink.push(format!("{path}/reaction"), "要写命中后给麦麦的提示");
    }
    for (j, pattern) in rule.regex.iter().enumerate() {
        check_regex(sink, &format!("{path}/regex/{j}"), pattern);
    }
}

fn bot_rules(bot: &MaiBotBotConfigFile, sink: &mut IssueSink) {
    for (i, pattern) in bot.message_receive.ban_msgs_regex.iter().enumerate() {
        check_regex(
            sink,
            &format!("bot/message_receive/ban_msgs_regex/{i}"),
            pattern,
        );
    }
    let kw = &bot.keyword_reaction;
    for (i, rule) in kw.keyword_rules.iter().enumerate() {
        keyword_rule(
            sink,
            &format!("bot/keyword_reaction/keyword_rules/{i}"),
            rule,
        );
    }
    for (i, rule) in kw.regex_rules.iter().enumerate() {
        keyword_rule(sink, &format!("bot/keyword_reaction/regex_rules/{i}"), rule);
    }

    // 额外提示词：整条空着算没写，写了就三样都要有
    for (i, item) in bot.chat.reply_style.chat_prompts.iter().enumerate() {
        let filled = [&item.platform, &item.item_id, &item.prompt].map(|s| !s.trim().is_empty());
        if filled.iter().any(|f| *f) && !filled.iter().all(|f| *f) {
            let path = format!("bot/chat/reply_style/chat_prompts/{i}");
            for (name, ok) in ["platform", "item_id", "prompt"].into_iter().zip(filled) {
                if !ok {
                    sink.push(format!("{path}/{name}"), "平台、聊天 ID 和提示词要一起填");
                }
            }
        }
    }

    let mem = &bot.a_memorix;
    if mem.threshold.min_threshold >= mem.threshold.max_threshold {
        sink.push("bot/a_memorix/threshold/min_threshold", "要小于最大阈值");
    }
    if mem.memory.revive_threshold <= mem.memory.prune_threshold {
        sink.push("bot/a_memorix/memory/revive_threshold", "要大于修剪阈值");
    }
    let int = &mem.integration;
    let base = "bot/a_memorix/integration";
    sink.range(
        &format!("{base}/fuzzy_modify_confirm_threshold"),
        int.fuzzy_modify_confirm_threshold,
        Some(0.0),
        Some(1.0),
    );
    sink.range(
        &format!("{base}/feedback_correction_auto_apply_threshold"),
        int.feedback_correction_auto_apply_threshold,
        Some(0.0),
        Some(1.0),
    );
    if int.feedback_correction_window_hours <= 0.0 {
        sink.push(
            format!("{base}/feedback_correction_window_hours"),
            "要大于 0",
        );
    }
    for (name, v) in [
        (
            "fuzzy_modify_candidate_limit",
            int.fuzzy_modify_candidate_limit,
        ),
        ("fuzzy_modify_max_targets", int.fuzzy_modify_max_targets),
        (
            "feedback_correction_check_interval_minutes",
            int.feedback_correction_check_interval_minutes,
        ),
        (
            "feedback_correction_batch_size",
            int.feedback_correction_batch_size,
        ),
        (
            "feedback_correction_max_feedback_messages",
            int.feedback_correction_max_feedback_messages,
        ),
        (
            "feedback_correction_reconcile_interval_minutes",
            int.feedback_correction_reconcile_interval_minutes,
        ),
        (
            "feedback_correction_reconcile_batch_size",
            int.feedback_correction_reconcile_batch_size,
        ),
    ] {
        sink.range(&format!("{base}/{name}"), v as f64, Some(1.0), None);
    }

    let mcp = &bot.mcp;
    for (i, root) in mcp.client.roots.items.iter().enumerate() {
        if root.enabled && root.uri.trim().is_empty() {
            sink.push(
                format!("bot/mcp/client/roots/items/{i}/uri"),
                "启用的目录要填 uri",
            );
        }
    }
    let eli = &mcp.client.elicitation;
    if eli.enable && !(eli.allow_form || eli.allow_url) {
        sink.push(
            "bot/mcp/client/elicitation/allow_form",
            "开启后至少允许一种方式",
        );
    }
    let mut names = HashSet::new();
    for (i, server) in mcp.servers.iter().enumerate() {
        if !server.enabled {
            continue;
        }
        let path = format!("bot/mcp/servers/{i}");
        let name = server.name.trim();
        if name.is_empty() {
            sink.push(format!("{path}/name"), "要起个名字");
        } else if !names.insert(name.to_string()) {
            sink.push(format!("{path}/name"), "和别的 MCP 服务重名了");
        }
        match server.transport.as_str() {
            "stdio" if server.command.trim().is_empty() => {
                sink.push(format!("{path}/command"), "stdio 方式要填启动命令");
            }
            "streamable_http" | "sse" if server.url.trim().is_empty() => {
                sink.push(format!("{path}/url"), "这种连接方式要填地址");
            }
            _ => {}
        }
        if server.authorization.mode == "bearer"
            && server.authorization.bearer_token.trim().is_empty()
        {
            sink.push(
                format!("{path}/authorization/bearer_token"),
                "bearer 认证要填 token",
            );
        }
    }
}

fn model_rules(models: &MaiBotModelConfigFile, sink: &mut IssueSink) {
    if models.api_providers.is_empty() {
        sink.push("models/api_providers", "至少要有一个提供商");
    }
    if models.models.is_empty() {
        sink.push("models/models", "至少要有一个模型");
    }
    let mut providers = HashSet::new();
    for (i, p) in models.api_providers.iter().enumerate() {
        let path = format!("models/api_providers/{i}");
        if p.name.trim().is_empty() {
            sink.push(format!("{path}/name"), "要起个名字");
        } else if !providers.insert(p.name.as_str()) {
            sink.push(format!("{path}/name"), "和别的提供商重名了");
        }
        if p.auth_type != "none" && p.api_key.trim().is_empty() {
            sink.push(format!("{path}/api_key"), "要填 API Key");
        }
        if p.client_type != "gemini" && p.base_url.trim().is_empty() {
            sink.push(format!("{path}/base_url"), "要填接口地址");
        }
        if p.auth_type == "header" && p.auth_header_name.trim().is_empty() {
            sink.push(format!("{path}/auth_header_name"), "请求头认证要填头名");
        }
        if p.auth_type == "query" && p.auth_query_name.trim().is_empty() {
            sink.push(format!("{path}/auth_query_name"), "查询参数认证要填参数名");
        }
    }
    let mut model_names = HashSet::new();
    for (i, m) in models.models.iter().enumerate() {
        let path = format!("models/models/{i}");
        if m.name.trim().is_empty() {
            sink.push(format!("{path}/name"), "要起个名字");
        } else if !model_names.insert(m.name.as_str()) {
            sink.push(format!("{path}/name"), "和别的模型重名了");
        }
        if m.model_identifier.trim().is_empty() {
            sink.push(
                format!("{path}/model_identifier"),
                "要填服务商那边的模型标识",
            );
        }
        if !providers.contains(m.api_provider.as_str()) {
            sink.push(format!("{path}/api_provider"), "选一个已有的提供商");
        }
        if let Err(e) = super::super::schema::any_table::to_value(&m.extra_params) {
            sink.push(format!("{path}/extra_params"), e);
        }
    }
    // 上游调用时才查任务里的模型名，填错了要等麦麦说话那一刻才报错，这里保存时就拦
    let t = &models.model_task_config;
    let tasks: [(&str, &MaiBotTaskConfig); 11] = [
        ("replyer", &t.replyer),
        ("planner", &t.planner),
        ("memory", &t.memory),
        ("mid_memory", &t.mid_memory),
        ("utils", &t.utils),
        ("learner", &t.learner),
        ("expression_use", &t.expression_use),
        ("emoji", &t.emoji),
        ("vlm", &t.vlm),
        ("voice", &t.voice),
        ("embedding", &t.embedding),
    ];
    for (task, cfg) in tasks {
        for (j, name) in cfg.model_list.iter().enumerate() {
            if !model_names.contains(name.as_str()) {
                sink.push(
                    format!("models/model_task_config/{task}/model_list/{j}"),
                    format!("没有叫 {name:?} 的模型"),
                );
            }
        }
    }
}
