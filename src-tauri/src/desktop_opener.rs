//! 系统浏览器必须使用宿主库，不能继承 AppImage 内的 GTK / Qt / GLib 搜索路径。

#[tauri::command]
pub async fn open_external_url(url: String) -> Result<(), String> {
    open_url(&url).await
}

pub async fn open_url(url: &str) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|_| "无法打开非法 URL".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("只允许打开 http/https 链接".into());
    }
    #[cfg(target_os = "linux")]
    {
        let url = parsed.to_string();
        launch(
            linux_open_command(url),
            std::time::Duration::from_secs(3),
            "系统浏览器",
        )
        .await
    }
    #[cfg(not(target_os = "linux"))]
    tauri_plugin_opener::open_url(parsed.as_str(), None::<&str>).map_err(|e| e.to_string())
}

#[cfg(target_os = "linux")]
pub async fn open_directory(path: &std::path::Path) -> Result<(), String> {
    if !path.is_absolute() || !path.is_dir() {
        return Err(format!("目录不存在: {}", path.display()));
    }
    launch(
        linux_open_command(path),
        std::time::Duration::from_secs(3),
        "文件管理器",
    )
    .await
}

#[cfg(target_os = "linux")]
fn linux_open_command(target: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let mut command = std::process::Command::new("/usr/bin/xdg-open");
    command.arg(target);
    ncd_host::appimage::clean_appimage_environment(&mut command);
    command
}

#[cfg(target_os = "linux")]
async fn launch(
    command: std::process::Command,
    budget: std::time::Duration,
    application: &str,
) -> Result<(), String> {
    use std::process::Stdio;
    let mut child = tokio::process::Command::from(command)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("无法启动{application}: {e}"))?;
    // 部分浏览器前台运行；只等待启动错误，随后在后台回收子进程。
    match tokio::time::timeout(budget, child.wait()).await {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(status)) => Err(format!("{application}启动失败 ({status})")),
        Ok(Err(error)) => Err(format!("等待{application}失败: {error}")),
        Err(_) => {
            tauri::async_runtime::spawn(async move {
                let _ = child.wait().await;
            });
            Ok(())
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn directory_launcher_keeps_literal_paths_and_strips_appimage_environment() {
        const MARKER: &str = "NCD_DIRECTORY_OPENER_FIXTURE";
        if std::env::var_os(MARKER).is_some() {
            let path = std::path::Path::new("/tmp/日志目录; not-a-command");
            let command = linux_open_command(path);
            assert_eq!(command.get_program(), "/usr/bin/xdg-open");
            assert_eq!(command.get_args().collect::<Vec<_>>(), [path.as_os_str()]);
            let changes: std::collections::HashMap<_, _> = command.get_envs().collect();
            assert_eq!(changes.get(std::ffi::OsStr::new("APPDIR")), Some(&None));
            assert_eq!(changes.get(std::ffi::OsStr::new("ARGV0")), Some(&None));
            assert_eq!(
                changes.get(std::ffi::OsStr::new("LD_LIBRARY_PATH")),
                Some(&Some(std::ffi::OsStr::new("/usr/lib")))
            );
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let appdir = root.path().join("appimage");
        std::fs::create_dir_all(appdir.join("usr/lib")).unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "desktop_opener::tests::directory_launcher_keeps_literal_paths_and_strips_appimage_environment", "--nocapture"])
            .env(MARKER, "1")
            .env("APPDIR", &appdir)
            .env("ARGV0", "desktop.AppImage")
            .env("LD_LIBRARY_PATH", std::env::join_paths([appdir.join("usr/lib"), std::path::PathBuf::from("/usr/lib")]).unwrap())
            .output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[tokio::test]
    async fn directory_launcher_rejects_missing_directories_and_reports_startup_failure() {
        let root = tempfile::tempdir().unwrap();
        assert!(
            open_directory(&root.path().join("missing"))
                .await
                .unwrap_err()
                .contains("目录不存在")
        );
        let mut fail = Command::new("/bin/sh");
        fail.args(["-c", "exit 4"]);
        assert!(
            launch(fail, std::time::Duration::from_secs(1), "文件管理器")
                .await
                .unwrap_err()
                .contains("文件管理器启动失败")
        );
    }

    #[tokio::test]
    async fn rejects_non_web_urls_before_launching_a_process() {
        for url in [
            "invalid",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "ms-settings:appsfeatures",
        ] {
            assert!(open_url(url).await.is_err());
        }
    }

    #[tokio::test]
    async fn reports_launcher_failure_and_releases_foreground_browsers() {
        let mut fail = Command::new("/bin/sh");
        fail.args(["-c", "exit 4"]);
        assert!(
            launch(fail, std::time::Duration::from_secs(1), "系统浏览器")
                .await
                .unwrap_err()
                .contains("exit status: 4")
        );
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("browser-finished");
        let mut browser = Command::new("/bin/sh");
        browser
            .args(["-c", "sleep 0.1; touch \"$1\"", "browser"])
            .arg(&marker);
        launch(browser, std::time::Duration::from_millis(5), "系统浏览器")
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !marker.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }
}
