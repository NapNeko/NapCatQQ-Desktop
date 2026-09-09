//! 识别已有 NoneBot2 项目（官方目录：pyproject.toml + bot.py + dotenv）。

use ncd_domain::AppProjectProbe;
use ncd_host::{Host, HostPath};
use ncd_traits::AppFrameworkError;

use super::component::NoneBot2Component;
use super::config::read_listen_port;
use super::env_layout::{self, NoneBotEnvLayout};
use super::manifest::{
    LOCK_PACKAGE_NONEBOT2, NONEBOT2_BOT_PY, NONEBOT2_ENV_FILE, NONEBOT2_FRAMEWORK_ID,
    NONEBOT2_PYPROJECT, NONEBOT2_UV_LOCK,
};

pub fn pyproject_looks_like_nonebot2(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("nonebot2") || lower.contains("[tool.nonebot]")
}

pub fn bot_py_looks_like_nonebot(text: &str) -> bool {
    text.contains("import nonebot") || text.contains("from nonebot")
}

pub fn display_name_from_pyproject(text: &str) -> Option<String> {
    let mut in_project = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_project = line == "[project]";
            continue;
        }
        if !in_project {
            continue;
        }
        if let Some(rest) = line.strip_prefix("name") {
            let rest = rest.trim().strip_prefix('=')?.trim();
            let rest = rest.strip_prefix('"')?.strip_suffix('"')?;
            if !rest.is_empty() {
                return Some(rest.to_string());
            }
        }
    }
    None
}

pub async fn probe_nonebot2(
    host: &dyn Host,
    path: &HostPath,
) -> Result<AppProjectProbe, AppFrameworkError> {
    let pyproject = path.join(NONEBOT2_PYPROJECT);
    if !host
        .exists(&pyproject)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?
    {
        return Err(AppFrameworkError::Validation(
            "目录里没有 pyproject.toml，不是 NoneBot2 项目".into(),
        ));
    }
    let toml = read_text(host, &pyproject).await?.unwrap_or_default();
    if !pyproject_looks_like_nonebot2(&toml) {
        return Err(AppFrameworkError::Validation(
            "pyproject.toml 没有 nonebot2 / [tool.nonebot]".into(),
        ));
    }

    let mut warnings = Vec::new();
    let bot_path = path.join(NONEBOT2_BOT_PY);
    let bot_text = read_text(host, &bot_path).await?;
    match bot_text.as_deref() {
        None => {
            return Err(AppFrameworkError::Validation(
                "目录里没有 bot.py（nb run / python bot.py 的默认入口）".into(),
            ));
        }
        Some(text) if !bot_py_looks_like_nonebot(text) => {
            warnings.push("bot.py 里没看到 nonebot，启动入口可能不是这个文件".into());
        }
        Some(_) => {}
    }

    let base = read_text(host, &path.join(NONEBOT2_ENV_FILE)).await?;
    if base.is_none() {
        warnings.push("没有 .env，会按 NoneBot 默认走 prod".into());
    }
    let environment = env_layout::environment_from_dotenv(base.as_deref().unwrap_or(""));
    let overlay_rel = env_layout::overlay_rel(&environment);
    let overlay_exists = host
        .exists(&path.join(&overlay_rel))
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    let layout = NoneBotEnvLayout {
        environment: environment.clone(),
        write_rel: if overlay_exists {
            overlay_rel
        } else {
            NONEBOT2_ENV_FILE.to_string()
        },
    };

    let overlay_text = if overlay_exists {
        read_text(host, &path.join(&layout.write_rel)).await?
    } else {
        None
    };
    let mut port = read_listen_port(base.as_deref(), overlay_text.as_deref());
    if port.is_none() {
        if let Some(text) = bot_text.as_deref() {
            port = port_from_python_source(text);
            if port.is_some() {
                warnings.push("端口写在 bot.py 里，不在配置文件".into());
            }
        }
    }
    if bot_text.as_deref().is_some_and(|t| t.contains("_env_file")) {
        warnings.push("bot.py 自己指定了配置文件，.env 里的 ENVIRONMENT 不会生效".into());
    }

    let lock = read_text(host, &path.join(NONEBOT2_UV_LOCK)).await?;
    let version = lock
        .as_deref()
        .and_then(|t| NoneBot2Component::lock_package_version(t, LOCK_PACKAGE_NONEBOT2));

    let python = NoneBot2Component::new(path.clone(), port.unwrap_or(8080)).venv_python(host.os());
    let has_venv = host
        .exists(&python)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    let ready = has_venv;
    if !has_venv {
        warnings.push("还没装依赖，导入后要先同步才能启动".into());
    }

    let display_name = display_name_from_pyproject(&toml)
        .unwrap_or_else(|| path.file_name().unwrap_or("NoneBot2").to_string());

    Ok(AppProjectProbe {
        framework_id: ncd_domain::AppFrameworkId::new(NONEBOT2_FRAMEWORK_ID),
        path: path.as_posix().to_string(),
        display_name,
        port,
        version,
        env_rel_path: layout.write_rel,
        environment: layout.environment,
        ready,
        running: false,
        supervisors: Vec::new(),
        warnings,
        detected_bot_id: None,
    })
}

/// `nonebot.init(port=8080)` / `nonebot.run(port=8080)`；官方 init 参数高于 dotenv。
pub fn port_from_python_source(text: &str) -> Option<u16> {
    let lower = text.to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("port") {
        let abs = from + rel;
        let before = abs
            .checked_sub(1)
            .and_then(|i| text.as_bytes().get(i).copied())
            .unwrap_or(b' ');
        if before.is_ascii_alphanumeric() || before == b'_' {
            from = abs + 4;
            continue;
        }
        let after = text[abs + 4..].trim_start();
        let Some(after) = after.strip_prefix('=') else {
            from = abs + 4;
            continue;
        };
        let digits: String = after.trim_start().chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(p) = digits.parse::<u16>()
            && p > 0
        {
            return Some(p);
        }
        from = abs + 4;
    }
    None
}

async fn read_text(host: &dyn Host, path: &HostPath) -> Result<Option<String>, AppFrameworkError> {
    if !host
        .exists(path)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?
    {
        return Ok(None);
    }
    let bytes = host
        .read_file(path)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_xiuxian_pyproject_shape() {
        let text = r#"
[project]
name = "bot-xiuxian"
dependencies = ["nonebot2[httpx,websockets]>=2.5.0"]

[tool.nonebot]
plugin_dirs = ["src/plugins"]
"#;
        assert!(pyproject_looks_like_nonebot2(text));
        assert_eq!(display_name_from_pyproject(text).as_deref(), Some("bot-xiuxian"));
        assert!(bot_py_looks_like_nonebot(
            "import nonebot\nfrom nonebot.adapters.onebot.v11 import Adapter as OneBotV11Adapter\n"
        ));
    }

    #[test]
    fn port_from_init_and_run() {
        assert_eq!(
            port_from_python_source("nonebot.init(port=13120)\nnonebot.run()\n"),
            Some(13120)
        );
        assert_eq!(
            port_from_python_source("if __name__ == '__main__':\n    nonebot.run(port = 8081)\n"),
            Some(8081)
        );
        assert_eq!(port_from_python_source("report_self = True\n"), None);
    }
}
