//! 识别已有 Karin 项目（node-karin + .env）。

use ncd_domain::AppProjectProbe;
use ncd_host::{Host, HostPath};
use ncd_traits::AppFrameworkError;

use super::manifest::{
    ENV_HTTP_PORT, KARIN_ENV_FILE, KARIN_FRAMEWORK_ID, KARIN_NPM_PACKAGE, KARIN_PACKAGE_JSON,
};
use crate::env_file::EnvFile;

pub fn package_looks_like_karin(text: &str) -> bool {
    text.contains(KARIN_NPM_PACKAGE) || text.contains("\"karin\"")
}

pub async fn probe_karin(host: &dyn Host, path: &HostPath) -> Result<AppProjectProbe, AppFrameworkError> {
    let pkg_root = path.join("package.json");
    if !host
        .exists(&pkg_root)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?
    {
        return Err(AppFrameworkError::Validation(
            "目录里没有 package.json，不是 Karin 项目".into(),
        ));
    }
    let pkg_text = read_text(host, &pkg_root).await?.unwrap_or_default();
    let karin_pkg = path.join(KARIN_PACKAGE_JSON);
    let has_node_karin = host
        .exists(&karin_pkg)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    if !has_node_karin && !package_looks_like_karin(&pkg_text) {
        return Err(AppFrameworkError::Validation(
            "package.json 没有 node-karin，也还没装到 node_modules".into(),
        ));
    }

    let mut warnings = Vec::new();
    let env_text = read_text(host, &path.join(KARIN_ENV_FILE)).await?;
    if env_text.is_none() {
        warnings.push("没有 .env，可能还没做过 karin init".into());
    }
    let port = env_text.as_deref().and_then(|t| {
        EnvFile::parse(t)
            .get(ENV_HTTP_PORT)
            .and_then(|v| v.parse().ok())
            .filter(|p| *p > 0)
    });

    let version = if has_node_karin {
        read_text(host, &karin_pkg)
            .await?
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v.get("version")?.as_str().map(str::to_string))
    } else {
        None
    };

    let display_name = serde_json::from_str::<serde_json::Value>(&pkg_text)
        .ok()
        .and_then(|v| v.get("name")?.as_str().map(str::to_string))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| path.file_name().unwrap_or("Karin").to_string());

    let ready = has_node_karin && env_text.is_some();
    if !ready && has_node_karin {
        warnings.push("已经装了 node-karin，但还缺 .env，导入后要先补齐才能启动".into());
    }
    if !has_node_karin {
        warnings.push("还没装 node-karin，导入后要先装依赖".into());
    }

    Ok(AppProjectProbe {
        framework_id: ncd_domain::AppFrameworkId::new(KARIN_FRAMEWORK_ID),
        path: path.as_posix().to_string(),
        display_name,
        port,
        version,
        env_rel_path: KARIN_ENV_FILE.to_string(),
        environment: String::new(),
        ready,
        running: false,
        supervisors: Vec::new(),
        warnings,
        detected_bot_id: None,
    })
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
