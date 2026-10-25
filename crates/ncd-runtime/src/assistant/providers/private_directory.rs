//! MiniMax 独立认证目录必须先收紧权限，再交给 Agent 写 OAuth 凭据。

use std::path::Path;

#[cfg(test)]
mod tests;
#[cfg(windows)]
mod windows;

pub(super) async fn secure_minimax_home(workspace: &Path, home: &Path) -> Result<(), String> {
    let workspace = tokio::fs::canonicalize(workspace)
        .await
        .map_err(|_| "无法确认 MiniMax 工作目录".to_string())?;
    let expected = workspace.join(".ncd-agent").join("minimax");
    if home != expected {
        return Err("只能保护当前诊断会话的指定 MiniMax 目录".into());
    }
    let metadata = tokio::fs::symlink_metadata(home)
        .await
        .map_err(|_| "MiniMax 登录目录不存在".to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("MiniMax 登录目录不能是文件或符号链接".into());
    }
    let home = tokio::fs::canonicalize(home)
        .await
        .map_err(|_| "无法解析 MiniMax 登录目录".to_string())?;
    if !home.starts_with(&workspace) || home != expected {
        return Err("MiniMax 登录目录越出诊断工作目录".into());
    }
    tokio::task::spawn_blocking(move || protect(&home))
        .await
        .map_err(|_| "保护 MiniMax 登录目录失败".to_string())?
}

#[cfg(windows)]
fn protect(path: &Path) -> Result<(), String> {
    windows::protect(path)
}

#[cfg(unix)]
fn protect(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|_| "无法将 MiniMax 登录目录权限设为 0700".to_string())
}

#[cfg(not(any(windows, unix)))]
fn protect(_path: &Path) -> Result<(), String> {
    Err("当前平台无法保护 MiniMax 独立登录目录".into())
}
