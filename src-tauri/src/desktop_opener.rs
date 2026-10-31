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
        let mut command = std::process::Command::new("/usr/bin/xdg-open");
        command.arg(url);
        ncd_host::appimage::clean_appimage_environment(&mut command);
        launch(command, std::time::Duration::from_secs(3)).await
    }
    #[cfg(not(target_os = "linux"))]
    tauri_plugin_opener::open_url(parsed.as_str(), None::<&str>).map_err(|e| e.to_string())
}

#[cfg(target_os = "linux")]
async fn launch(command: std::process::Command, budget: std::time::Duration) -> Result<(), String> {
    use std::process::Stdio;
    let mut child = tokio::process::Command::from(command)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("无法启动系统浏览器: {e}"))?;
    // 部分浏览器前台运行；只等待启动错误，随后在后台回收子进程。
    match tokio::time::timeout(budget, child.wait()).await {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(status)) => Err(format!("系统浏览器启动失败 ({status})")),
        Ok(Err(error)) => Err(format!("等待系统浏览器失败: {error}")),
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
            launch(fail, std::time::Duration::from_secs(1))
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
        launch(browser, std::time::Duration::from_millis(5))
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
