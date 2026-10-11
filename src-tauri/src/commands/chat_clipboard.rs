//! 消息复制：准备本机文件后，一次发布文本、图片和 CF_HDROP 文件列表。
use crate::AppState;
use base64::Engine as _;
use ncd_domain::chat_group_files::{ChatFileDownloadRequest, ChatFileSource};
use serde::Deserialize;
use std::{path::PathBuf, sync::Arc};
use tauri::{Manager, State, Webview};

const MAX_BYTES: usize = 64 * 1024 * 1024;

#[derive(Deserialize)]
pub struct ClipboardMedia {
    name: String,
    image: bool,
    path: Option<String>,
    url: Option<String>,
    base64: Option<String>,
    source: Option<ChatFileSource>,
}

fn cache_path(app: &tauri::AppHandle, name: &str) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("chat-clipboard");
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    // 缓存保留七天，让资源管理器稍后粘贴仍然有效，同时避免永久堆积。
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            if entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age.as_secs() > 7 * 24 * 3600)
            {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
    let directory = root.join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir(&directory).map_err(|e| e.to_string())?;
    let name: String = name
        .chars()
        .map(|c| {
            if c.is_control() || r#"\/:*?"<>|"#.contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(120)
        .collect();
    let name = name.trim_matches([' ', '.']);
    Ok(directory.join(if name.is_empty() { "附件" } else { name }))
}

#[tauri::command]
pub async fn chat_stage_attachment(
    app: tauri::AppHandle,
    name: String,
    base64: String,
) -> Result<String, String> {
    if base64.len() > MAX_BYTES * 4 / 3 + 4 {
        return Err("附件不能超过 64 MiB".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64)
        .map_err(|_| "附件内容无效")?;
    if bytes.len() > MAX_BYTES {
        return Err("附件不能超过 64 MiB".into());
    }
    let path = cache_path(&app, &name)?;
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

struct QuietProgress;
impl ncd_runtime::DebugStreamSink for QuietProgress {
    fn send(&self, _: &ncd_domain::onebot_debug::DebugStreamProgress) -> bool {
        true
    }
}

#[tauri::command]
pub async fn chat_copy_message(
    app: tauri::AppHandle,
    webview: Webview,
    state: State<'_, AppState>,
    bot_id: String,
    text: String,
    media: Vec<ClipboardMedia>,
) -> Result<(), String> {
    if !state.chat.is_enabled() {
        return Err("聊天功能已关闭".into());
    }
    if media.len() > 100 || text.len() > 4 * 1024 * 1024 {
        return Err("消息内容过大，无法复制".into());
    }
    let mut paths = Vec::new();
    let mut image_path = None;
    for item in media {
        let path = if let Some(path) = item.path {
            let path = PathBuf::from(path);
            if !tokio::fs::metadata(&path)
                .await
                .map_err(|e| e.to_string())?
                .is_file()
            {
                return Err("只能复制文件".into());
            }
            path
        } else {
            let mut path = cache_path(&app, &item.name)?;
            if let Some(source) = item.source {
                state
                    .chat
                    .download_file(
                        ChatFileDownloadRequest {
                            request_id: uuid::Uuid::new_v4().to_string(),
                            bot_id: bot_id.clone(),
                            source,
                            name: item.name,
                            dest: path.to_string_lossy().into_owned(),
                        },
                        Arc::new(QuietProgress),
                    )
                    .await?;
            } else {
                let bytes = if let Some(base64) = item.base64 {
                    if base64.len() > MAX_BYTES * 4 / 3 + 4 {
                        return Err("媒体超过复制大小限制（64 MiB）".into());
                    }
                    base64::engine::general_purpose::STANDARD
                        .decode(base64)
                        .map_err(|_| "媒体内容无效")?
                } else if let Some(url) = item.url {
                    let url = url::Url::parse(&url).map_err(|_| "媒体地址无效")?;
                    if !matches!(url.scheme(), "http" | "https") {
                        return Err("媒体地址无效".into());
                    }
                    tokio::time::timeout(std::time::Duration::from_secs(60), async {
                        let mut response = ncd_network::shared_client()
                            .get(url)
                            .send()
                            .await
                            .map_err(|e| e.to_string())?
                            .error_for_status()
                            .map_err(|e| e.to_string())?;
                        if response
                            .content_length()
                            .is_some_and(|len| len > MAX_BYTES as u64)
                        {
                            return Err("媒体超过复制大小限制（64 MiB）".to_owned());
                        }
                        let mut bytes = Vec::new();
                        while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
                            if bytes.len().saturating_add(chunk.len()) > MAX_BYTES {
                                return Err("媒体超过复制大小限制（64 MiB）".into());
                            }
                            bytes.extend_from_slice(&chunk);
                        }
                        Ok(bytes)
                    })
                    .await
                    .map_err(|_| "读取媒体超时")??
                } else {
                    return Err("媒体缺少可复制的来源".into());
                };
                if bytes.len() > MAX_BYTES {
                    return Err("媒体超过复制大小限制（64 MiB）".into());
                }
                if item.image
                    && let Ok(format) = image::guess_format(&bytes)
                    && let Some(extension) = format.extensions_str().first()
                {
                    path.set_extension(extension);
                }
                tokio::fs::write(&path, bytes)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            path
        };
        if item.image && image_path.is_none() {
            image_path = Some(path.clone());
        }
        paths.push(path);
    }
    #[cfg(windows)]
    {
        let owner = webview.window().hwnd().map_err(|e| e.to_string())?.0 as isize;
        tauri::async_runtime::spawn_blocking(move || {
            crate::screenshot::clipboard::copy_message(
                windows::Win32::Foundation::HWND(owner as *mut _),
                &text,
                &paths,
                image_path.as_deref(),
            )
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = (webview, text, paths, image_path);
        Err("文件复制需要 Windows 客户端".into())
    }
}

/// 逐条转发文件时先缓存到桌面端，再沿用带进度的附件上传通道。
#[tauri::command]
pub async fn chat_cache_file(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    bot_id: String,
    source: ChatFileSource,
    name: String,
) -> Result<String, String> {
    let path = cache_path(&app, &name)?;
    state
        .chat
        .download_file(
            ChatFileDownloadRequest {
                request_id: uuid::Uuid::new_v4().to_string(),
                bot_id,
                source,
                name,
                dest: path.to_string_lossy().into_owned(),
            },
            Arc::new(QuietProgress),
        )
        .await
}
