//! 防止额外参数覆盖 ACP、工具边界和审批策略。

use ncd_domain::assistant::AssistantProvider;

pub(super) fn validate(provider: AssistantProvider, args: &[String]) -> Result<(), String> {
    if args.len() > 32 || args.iter().map(String::len).sum::<usize>() > 8192 {
        return Err("Agent 额外参数过多".into());
    }
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if argument.is_empty() || argument.len() > 1024 || argument.chars().any(char::is_control) {
            return Err("Agent 参数包含空值、控制字符或过长内容".into());
        }
        let (flag, inline_value) = argument
            .split_once('=')
            .map_or((argument.as_str(), None), |(flag, value)| {
                (flag, Some(value))
            });
        let takes_value = match provider {
            AssistantProvider::Qwen => matches!(flag, "--model" | "-m"),
            AssistantProvider::CodeBuddy => matches!(flag, "--model" | "--effort"),
            AssistantProvider::Qoder => matches!(flag, "--model" | "-m"),
            AssistantProvider::Custom => matches!(flag, "--model" | "-m" | "--reasoning-effort"),
            _ => false,
        };
        if takes_value {
            let value = match inline_value {
                Some(value) => value,
                None => {
                    index += 1;
                    args.get(index)
                        .map(String::as_str)
                        .ok_or_else(|| "Agent 参数缺少值".to_string())?
                }
            };
            if value.is_empty()
                || value.starts_with('-')
                || value.len() > 256
                || !value
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || "._-/:".contains(ch))
            {
                return Err("模型参数只能包含模型标识，不能包含配置、命令或密钥".into());
            }
        } else if provider == AssistantProvider::Custom
            && inline_value.is_none()
            && matches!(flag, "acp" | "serve" | "--acp" | "--stdio")
        {
            // 自定义 Agent 只接收 argv，不经 shell 解释。
        } else {
            return Err(
                "额外参数未获允许；只能填写支持的模型选项，ACP、配置与权限由 NCD 管理".into(),
            );
        }
        index += 1;
    }
    Ok(())
}
