//! 识别已有的 TRSS-Yunzai 目录（导入用）。Miao-Yunzai / 原版云崽给一句能照着做的话：
//! 它们用 icqq 登录、不认 OneBot，桌面端对接不了；Miao-Yunzai 自带 `trss.js` 能原地迁成 TRSS 版。

use ncd_domain::{AppFrameworkId, AppProjectProbe};
use ncd_host::{Host, HostPath};
use ncd_traits::AppFrameworkError;

use super::config::{config_rel, read_yunzai_config};
use super::manifest::{YUNZAI_FRAMEWORK_ID, YUNZAI_PACKAGE_JSON, YUNZAI_PACKAGE_NAME};

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

/// package.json 的 name 不是 trss-yunzai 时给用户的话；None = 就是 TRSS 版
pub fn foreign_yunzai_hint(name: &str) -> Option<String> {
    match name {
        YUNZAI_PACKAGE_NAME => None,
        "miao-yunzai" => Some(
            "这是 Miao-Yunzai：它用 icqq 登录、不接 OneBot，桌面端对接不了。可以在它的目录里运行 node trss.js 原地迁移成 TRSS-Yunzai 再导入".into(),
        ),
        "yunzai-bot" => Some("这是原版 / 喵喵维护版 Yunzai-Bot，桌面端只接 TRSS-Yunzai".into()),
        other => Some(format!("package.json 的 name 是 {other}，不是 TRSS-Yunzai")),
    }
}

pub async fn probe_yunzai(
    host: &dyn Host,
    path: &HostPath,
) -> Result<AppProjectProbe, AppFrameworkError> {
    let Some(pkg) = read_text(host, &path.join(YUNZAI_PACKAGE_JSON)).await? else {
        return Err(AppFrameworkError::Validation(
            "目录里没有 package.json，不是云崽项目".into(),
        ));
    };
    let value: serde_json::Value = serde_json::from_str(&pkg)
        .map_err(|e| AppFrameworkError::Validation(format!("package.json 解析失败: {e}")))?;
    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if let Some(hint) = foreign_yunzai_hint(name) {
        return Err(AppFrameworkError::Validation(hint));
    }
    let version = value
        .get("version")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let mut warnings = Vec::new();
    let ready = host
        .exists(&path.join("node_modules"))
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    if !ready {
        warnings.push("还没装依赖，导入后会先装一遍".into());
    }
    let (cfg, _) = read_yunzai_config(host, path).await?;
    let has_server_yaml = host
        .exists(&path.join(config_rel("server")))
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    if !has_server_yaml {
        warnings.push("还没跑过（没有 config/config），端口按默认 2536 算".into());
    }
    if cfg.redis.path == "redis-server" && !host.command_exists("redis-server").await {
        warnings.push(
            "redis.yaml 里的 redis-server 在这台机器上找不到，导入时会指向桌面端装的 Redis".into(),
        );
    }
    if !cfg.server.extra_auth_headers.is_empty() {
        warnings.push(format!(
            "server.yaml 的 auth 里还有 {}，对接时只留 Authorization（NapCat 只带这一个）",
            cfg.server.extra_auth_headers.join("、")
        ));
    }

    Ok(AppProjectProbe {
        framework_id: AppFrameworkId::new(YUNZAI_FRAMEWORK_ID),
        path: path.as_posix().to_string(),
        display_name: path.file_name().unwrap_or("TRSS-Yunzai").to_string(),
        port: Some(cfg.server.port).filter(|p| *p > 0),
        version,
        env_rel_path: config_rel("server"),
        environment: String::new(),
        ready,
        running: false,
        supervisors: Vec::new(),
        warnings,
        detected_bot_id: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn other_yunzai_flavors_get_actionable_hints() {
        assert!(foreign_yunzai_hint("trss-yunzai").is_none());
        assert!(
            foreign_yunzai_hint("miao-yunzai")
                .unwrap()
                .contains("node trss.js")
        );
        assert!(foreign_yunzai_hint("yunzai-bot").is_some());
        assert!(foreign_yunzai_hint("karin-x").unwrap().contains("karin-x"));
    }
}
