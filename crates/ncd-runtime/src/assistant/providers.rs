//! 本机 Agent 的发现与专用 ACP 启动配置。

mod arguments;
mod discovery;
mod private_directory;
mod profiles;
#[cfg(test)]
mod tests;

use std::path::Path;

use futures_util::{StreamExt, stream};
use ncd_domain::assistant::{
    AssistantProvider, AssistantProviderConfig, AssistantProviderStatus, AssistantSettings,
};

use super::acp::AcpLaunch;

const PROVIDERS: [AssistantProvider; 8] = [
    AssistantProvider::Dsh,
    AssistantProvider::Qwen,
    AssistantProvider::Kimi,
    AssistantProvider::MiniMax,
    AssistantProvider::CodeBuddy,
    AssistantProvider::Qoder,
    AssistantProvider::Trae,
    AssistantProvider::Custom,
];

pub fn validate_settings(settings: &AssistantSettings) -> Result<(), String> {
    super::credentials::validate_profile(&settings.builtin, false)?;
    let mut seen = std::collections::HashSet::new();
    if settings.providers.len() > PROVIDERS.len() {
        return Err("Agent 设置条目过多".into());
    }
    for config in &settings.providers {
        if config.provider == AssistantProvider::Builtin {
            return Err("内置模型请使用模型设置，不能配置为外部程序".into());
        }
        if !seen.insert(config.provider) {
            return Err("Agent 设置包含重复条目".into());
        }
        arguments::validate(config.provider, &config.extra_args)?;
        if let Some(path) = config
            .executable_path
            .as_deref()
            .filter(|path| !path.trim().is_empty())
        {
            if !Path::new(path).is_absolute() || path.chars().any(char::is_control) {
                return Err("Agent 可执行路径必须是绝对路径，不能填写命令或控制字符".into());
            }
        }
    }
    Ok(())
}

pub fn configuration_notice(
    provider: AssistantProvider,
    settings: &AssistantSettings,
) -> Option<&'static str> {
    if provider == AssistantProvider::MiniMax && !config_for(provider, settings).allow_native_tools
    {
        Some(
            "MiniMax 的受限接入使用此诊断会话的独立配置目录，需要在该目录独立登录；NCD 不会读取或复制已有登录凭据。若要沿用已有登录，可在助手设置中明确允许原生工具。原生工具遵循 Agent 自身权限，NCD 工具操作仍需单独在桌面确认。",
        )
    } else {
        None
    }
}

pub async fn discover(settings: &AssistantSettings) -> Vec<AssistantProviderStatus> {
    let mut results = stream::iter(
        PROVIDERS
            .into_iter()
            .enumerate()
            .map(|(index, provider)| async move { (index, inspect(provider, settings).await) }),
    )
    .buffer_unordered(2)
    .collect::<Vec<_>>()
    .await;
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, status)| status).collect()
}

async fn inspect(
    provider: AssistantProvider,
    settings: &AssistantSettings,
) -> AssistantProviderStatus {
    let config = config_for(provider, settings);
    let mut status = AssistantProviderStatus {
        provider,
        label: label(provider).into(),
        enabled: config.enabled,
        installed: false,
        executable_path: None,
        version: None,
        error: None,
        documentation_url: documentation_url(provider).into(),
    };
    let resolved = match discovery::resolve(provider, config.executable_path.as_deref()).await {
        Ok(resolved) => resolved,
        Err(error) => {
            status.error = Some(error);
            return status;
        }
    };
    status.installed = true;
    status.executable_path = Some(resolved.display_path.to_string_lossy().into_owned());
    match discovery::version(&resolved).await {
        Ok(version) => status.version = Some(version),
        Err(error) => {
            status.error = Some(error);
            return status;
        }
    }
    status.error = arguments::validate(provider, &config.extra_args)
        .err()
        .or_else(|| {
            profiles::check_support(
                provider,
                status.version.as_deref(),
                config.allow_native_tools,
            )
            .err()
        });
    status
}

pub async fn launch(
    provider: AssistantProvider,
    settings: &AssistantSettings,
    workspace: &Path,
) -> Result<AcpLaunch, String> {
    let config = config_for(provider, settings);
    if !config.enabled {
        return Err("此 Agent 已在设置中停用".into());
    }
    arguments::validate(provider, &config.extra_args)?;
    if !workspace.is_absolute() {
        return Err("助手工作目录必须是绝对路径".into());
    }
    let workspace = tokio::fs::canonicalize(workspace)
        .await
        .map_err(|_| "助手工作目录不存在或不可访问".to_string())?;
    if !tokio::fs::metadata(&workspace)
        .await
        .map_err(|_| "无法检查助手工作目录".to_string())?
        .is_dir()
    {
        return Err("助手工作目录必须是目录".into());
    }
    let resolved = discovery::resolve(provider, config.executable_path.as_deref()).await?;
    let version = discovery::version(&resolved).await?;
    profiles::check_support(provider, Some(&version), config.allow_native_tools)?;
    let mut launch = AcpLaunch {
        executable: resolved.program,
        args: resolved.prefix_args,
        env: Default::default(),
        cwd: workspace,
    };
    profiles::configure(provider, config.allow_native_tools, &mut launch).await?;
    // ACP 子命令的位置由 provider 决定，额外参数只能是经过白名单校验的选项。
    launch.args.extend(config.extra_args);
    Ok(launch)
}

pub async fn login_command(
    provider: AssistantProvider,
    settings: &AssistantSettings,
    workspace: &Path,
) -> Result<Option<String>, String> {
    if provider != AssistantProvider::MiniMax {
        return Ok(None);
    }
    let launch = launch(provider, settings, workspace).await?;
    render_minimax_login(&launch).map(Some)
}

fn powershell_literal(value: &str) -> Result<String, String> {
    if value.chars().any(char::is_control) {
        return Err("登录命令的路径不能包含控制字符".into());
    }
    Ok(format!("'{}'", value.replace('\'', "''")))
}

fn render_minimax_login(launch: &AcpLaunch) -> Result<String, String> {
    let acp = launch
        .args
        .iter()
        .position(|argument| argument == "acp")
        .ok_or("MiniMax 启动参数缺少 ACP 入口")?;
    if acp > 1
        || launch.args.get(acp + 1).map(String::as_str) != Some("--append-system-prompt-file")
        || launch.args.len() != acp + 3
        || launch.env.keys().any(|key| key != "MINIMAX_DATA_DIR")
    {
        return Err("MiniMax 登录只接受已校验的固定入口，不能附加参数或凭据".into());
    }
    let program = launch
        .executable
        .to_str()
        .ok_or("MiniMax 路径不是有效文本")?;
    let cwd = launch.cwd.to_str().ok_or("MiniMax 工作目录不是有效文本")?;
    if !launch.executable.is_absolute() || !launch.cwd.is_absolute() {
        return Err("MiniMax 登录必须使用绝对路径".into());
    }
    let mut argv = vec![powershell_literal(program)?];
    for argument in &launch.args[..acp] {
        if !Path::new(argument).is_absolute() {
            return Err("MiniMax npm 入口必须使用绝对路径".into());
        }
        argv.push(powershell_literal(argument)?);
    }
    argv.extend(
        ["acp", "login", "--region", "cn", "--no-browser"]
            .into_iter()
            .map(powershell_literal)
            .collect::<Result<Vec<_>, _>>()?,
    );
    let invocation = format!("& {}", argv.join(" "));
    let cwd = powershell_literal(cwd)?;
    let (set_home, restore_home) = if let Some(home) = launch.env.get("MINIMAX_DATA_DIR") {
        if !Path::new(home).is_absolute() {
            return Err("MiniMax 登录目录必须使用绝对路径".into());
        }
        (
            format!(
                "$ncdPreviousMiniMaxHome = $env:MINIMAX_DATA_DIR; $env:MINIMAX_DATA_DIR = {}; ",
                powershell_literal(home)?
            ),
            "[Environment]::SetEnvironmentVariable('MINIMAX_DATA_DIR', $ncdPreviousMiniMaxHome, 'Process'); ",
        )
    } else {
        (String::new(), "")
    };
    Ok(format!(
        "& {{ $ErrorActionPreference = 'Stop'; Push-Location -LiteralPath {cwd}; {set_home}try {{ {invocation} }} finally {{ {restore_home}Pop-Location }} }}"
    ))
}

fn config_for(
    provider: AssistantProvider,
    settings: &AssistantSettings,
) -> AssistantProviderConfig {
    settings
        .providers
        .iter()
        .find(|config| config.provider == provider)
        .cloned()
        .unwrap_or(AssistantProviderConfig {
            provider,
            ..Default::default()
        })
}

fn label(provider: AssistantProvider) -> &'static str {
    match provider {
        AssistantProvider::Builtin => "内置 Agent",
        AssistantProvider::Dsh => "DeepSeek DSH",
        AssistantProvider::Qwen => "Qwen Code",
        AssistantProvider::Kimi => "Kimi Code",
        AssistantProvider::MiniMax => "MiniMax Code",
        AssistantProvider::CodeBuddy => "CodeBuddy Code",
        AssistantProvider::Qoder => "Qoder CLI",
        AssistantProvider::Trae => "TRAE CLI",
        AssistantProvider::Custom => "自定义 ACP Agent",
    }
}

fn documentation_url(provider: AssistantProvider) -> &'static str {
    match provider {
        AssistantProvider::Builtin => "",
        AssistantProvider::Dsh => "https://github.com/deepseek-ai/deepseek-harness",
        AssistantProvider::Qwen => "https://qwenlm.github.io/qwen-code-docs/",
        AssistantProvider::Kimi => {
            "https://moonshotai.github.io/kimi-code/en/reference/kimi-acp.html"
        }
        AssistantProvider::MiniMax => "https://github.com/MiniMax-AI/minimax-code",
        AssistantProvider::CodeBuddy => "https://www.codebuddy.ai/docs/cli/acp",
        AssistantProvider::Qoder => "https://docs.qoder.com/cli/acp",
        AssistantProvider::Trae => "https://docs.trae.cn/cli_agent-client-protocol",
        AssistantProvider::Custom => "https://agentclientprotocol.com/",
    }
}
