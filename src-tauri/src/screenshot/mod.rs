// 原生截图的异步入口，采集和编码在后台，窗口与输入留在 UI 线程。
use ncd_domain::chat_screenshot::{
    ChatScreenshotAttachment, ChatScreenshotRequest, ChatScreenshotShortcut,
};
use std::sync::{Arc, Mutex};
#[cfg(windows)]
use tauri::Emitter;
use tauri::Manager;
use tokio_util::sync::CancellationToken;

#[cfg(windows)]
pub mod capture;
#[cfg(windows)]
pub mod clipboard;
pub mod model;
#[cfg(windows)]
pub mod render;
#[cfg(windows)]
mod scroll;
#[cfg(windows)]
mod scroll_panel;
#[cfg(windows)]
mod shortcut;
pub mod stitch;
#[cfg(all(test, windows))]
mod stitch_native;
#[cfg(windows)]
mod toolbar;
#[cfg(windows)]
mod window;

#[derive(Default)]
pub struct ScreenshotCoordinator {
    active: Mutex<Option<(String, CancellationToken)>>,
}

pub(crate) const SHORTCUT_CAPTURE_OWNER: &str = "screenshot-shortcut";
impl ScreenshotCoordinator {
    fn acquire(&self, owner: &str) -> Result<CancellationToken, String> {
        let mut active = self.active.lock().map_err(|_| "截图状态不可用")?;
        if active.is_some() {
            return Err("已有截图正在进行".into());
        }
        let cancel = CancellationToken::new();
        *active = Some((owner.to_string(), cancel.clone()));
        Ok(cancel)
    }
    fn release(&self, owner: &str) {
        if let Ok(mut active) = self.active.lock() {
            if active.as_ref().is_some_and(|(p, _)| p == owner) {
                active.take();
            }
        }
    }
    fn cancel(&self, owner: &str) {
        if let Ok(active) = self.active.lock() {
            if let Some((page, token)) = active.as_ref()
                && page == owner
            {
                token.cancel();
            }
        }
    }
}

pub async fn capture_chat(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    cache: Arc<ncd_runtime::chat_screenshots::ChatScreenshotCache>,
    request: ChatScreenshotRequest,
    shortcut_context: Option<String>,
) -> Result<Option<ChatScreenshotAttachment>, String> {
    capture_inner(app, Some(window), cache, request, shortcut_context, None).await
}

#[cfg(windows)]
async fn capture_shortcut(
    app: tauri::AppHandle,
    window: Option<tauri::WebviewWindow>,
    cache: Arc<ncd_runtime::chat_screenshots::ChatScreenshotCache>,
    request: ChatScreenshotRequest,
    context: String,
    clipboard_owner: isize,
) -> Result<Option<ChatScreenshotAttachment>, String> {
    capture_inner(
        app,
        window,
        cache,
        request,
        Some(context),
        Some(clipboard_owner),
    )
    .await
}

async fn capture_inner(
    app: tauri::AppHandle,
    window: Option<tauri::WebviewWindow>,
    cache: Arc<ncd_runtime::chat_screenshots::ChatScreenshotCache>,
    request: ChatScreenshotRequest,
    shortcut_context: Option<String>,
    clipboard_owner: Option<isize>,
) -> Result<Option<ChatScreenshotAttachment>, String> {
    #[cfg(not(windows))]
    {
        let _ = (
            app,
            window,
            cache,
            request,
            shortcut_context,
            clipboard_owner,
        );
        Err("当前平台不支持原生截图".into())
    }
    #[cfg(windows)]
    {
        let owner = window
            .as_ref()
            .map_or(SHORTCUT_CAPTURE_OWNER, |window| window.label())
            .to_string();
        let standalone_view = window
            .is_none()
            .then(|| app.state::<crate::AppState>().chat.view());
        let coordinator = app.state::<ScreenshotCoordinator>();
        let cancel = coordinator.acquire(&owner)?;
        if !app.state::<crate::AppState>().chat.is_enabled() {
            coordinator.release(&owner);
            return Err("聊天功能已关闭".into());
        }
        let shortcut = shortcut_context.map(|context| (context, uuid::Uuid::new_v4().to_string()));
        if let Some((context, capture_id)) = &shortcut {
            emit_shortcut(
                &app,
                &owner,
                context,
                capture_id,
                ncd_domain::chat_screenshot::ChatScreenshotShortcutResult::Started,
            );
        }
        let hidden = request.hide_window
            && window.as_ref().is_some_and(|window| {
                window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false)
            });
        let mut completed = false;
        let mut result: Result<Option<ChatScreenshotAttachment>, String> = async {
            if hidden && let Some(window) = &window {crate::webview_scheduler::hide_window(window)?;}
            let desktop=tauri::async_runtime::spawn_blocking(capture::capture_desktop).await.map_err(|e|format!("采集线程失败：{e}"))??;
            if cancel.is_cancelled(){return Ok(None);}
            let (sender,receiver)=tokio::sync::oneshot::channel();
            let owner_copy=owner.clone();let cancel_copy=cancel.clone();
            app.run_on_main_thread(move||{
                if cancel_copy.is_cancelled(){let _=sender.send(Ok(None));return;}
                if let Err(error)=window::open(desktop,owner_copy,sender){tracing::warn!(%error,"创建原生截图窗失败");}
            }).map_err(|e|e.to_string())?;
            let output=tokio::select!{
                result=receiver=>result.map_err(|_|"截图窗口未能打开")??,
                _=cancel.cancelled()=>{let page=owner.clone();let _=app.run_on_main_thread(move||window::cancel_owner(&page));None},
            };
            let Some(mut output)=output else{return Ok(None);};
            if output.action == window::OutputAction::Scroll {
                let Some(scrolled) = scroll::run(&app, output, cancel.clone()).await? else { return Ok(None); };
                output = scrolled;
            }
            if cancel.is_cancelled(){return Ok(None);}
            if window.is_some() && app.get_webview_window(&owner).is_none(){return Ok(None);}
            completed = true;
            let action=output.action;
            let mut encoded=tauri::async_runtime::spawn_blocking(move||encode(output)).await.map_err(|e|format!("导出线程失败：{e}"))??;
            if cancel.is_cancelled(){return Ok(None);}
            let owner_hwnd = match &window {
                Some(window) => window.hwnd().map_err(|e|e.to_string())?.0 as isize,
                None => clipboard_owner.ok_or("截图剪贴板窗口不可用")?,
            };
            let width = encoded.width; let height = encoded.height;
            let pixels = std::mem::take(&mut encoded.rgba);
            let clipboard_result = tauri::async_runtime::spawn_blocking(move||{
                clipboard::copy_rgba(windows::Win32::Foundation::HWND(owner_hwnd as *mut _),width,height,&pixels)
            }).await.map_err(|e|e.to_string())?;
            match action {
                window::OutputAction::Attach=>{
                    let mut attachment = tauri::async_runtime::spawn_blocking(move||cache.save(encoded.width,encoded.height,&encoded.png,&encoded.preview)).await.map_err(|e|e.to_string())??;
                    attachment.clipboard_error = clipboard_result.err();
                    Ok(Some(attachment))
                },
                window::OutputAction::Copy=>{
                    clipboard_result?; Ok(None)
                },
                window::OutputAction::Save=>{
                    use tauri_plugin_dialog::DialogExt;
                    let dialog=app.dialog().file().add_filter("PNG 图片",&["png"]).set_file_name("截图.png");
                    let path=tauri::async_runtime::spawn_blocking(move||dialog.blocking_save_file()).await.map_err(|e|e.to_string())?;
                    if let Some(path)=path {
                        let path=path.into_path().map_err(|e|e.to_string())?;
                        tauri::async_runtime::spawn_blocking(move||std::fs::write(path,encoded.png)).await.map_err(|e|e.to_string())?.map_err(|e|format!("保存截图失败：{e}"))?;
                    }
                    Ok(None)
                },
                window::OutputAction::Scroll => Err("滚动截图尚未完成".into()),
            }
        }.await;
        // 导出与复制先完成；显示请求被正常焦点事件取代，不能把一张成功的截图判成失败。
        if let Some(window) = &window
            && (hidden || completed)
            && app.get_webview_window(&owner).is_some()
        {
            if let Err(error) = restore_owner(window).await {
                tracing::warn!(%error, "截图已完成，聊天窗口恢复稍后重试");
            }
        }
        if let Some(view) = standalone_view
            && let Ok(Some(file)) = &result
        {
            let staged = app
                .state::<crate::AppState>()
                .chat
                .stage_screenshot(&view, file)
                .await;
            match staged {
                Ok(bot_id) => {
                    if let Err(error) =
                        crate::chat_window::open_chat_window(app.clone(), Some(bot_id), false).await
                    {
                        tracing::warn!(%error, "截图已加入草稿，聊天窗口稍后恢复");
                    }
                }
                Err(error) => result = Err(error),
            }
        }
        if let Some((context, capture_id)) = shortcut {
            use ncd_domain::chat_screenshot::ChatScreenshotShortcutResult;
            let outcome = match &result {
                Ok(file) => ChatScreenshotShortcutResult::Finished { file: file.clone() },
                Err(message) => ChatScreenshotShortcutResult::Failed {
                    message: message.clone(),
                },
            };
            emit_shortcut(&app, &owner, &context, &capture_id, outcome);
        }
        coordinator.release(&owner);
        result
    }
}

#[cfg(windows)]
fn emit_shortcut(
    app: &tauri::AppHandle,
    owner: &str,
    context: &str,
    capture_id: &str,
    result: ncd_domain::chat_screenshot::ChatScreenshotShortcutResult,
) {
    let event = ncd_domain::chat_screenshot::ChatScreenshotShortcutEvent {
        v: 1,
        context: context.into(),
        capture_id: capture_id.into(),
        result,
    };
    if let Err(error) = app.emit_to(owner, shortcut::EVENT, event) {
        tracing::warn!(%owner, %error, "发布截图结果失败");
    }
}

#[cfg(windows)]
async fn restore_owner(window: &tauri::WebviewWindow) -> Result<(), String> {
    for attempt in 0..3 {
        if window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false) {
            return window.set_focus().map_err(|e| e.to_string());
        }
        match crate::webview_scheduler::show_window(window).await {
            Ok(()) => {
                let _ = window.unminimize();
                return window.set_focus().map_err(|e| e.to_string());
            }
            Err(error) if attempt == 2 => return Err(error),
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(40)).await,
        }
    }
    Err("聊天窗口暂时无法恢复".into())
}

pub fn cancel(app: &tauri::AppHandle, owner: &str) {
    app.state::<ScreenshotCoordinator>().cancel(owner);
    #[cfg(windows)]
    {
        let owner = owner.to_string();
        let _ = app.run_on_main_thread(move || window::cancel_owner(&owner));
    }
}
pub fn owner_destroyed(app: &tauri::AppHandle, owner: &str) {
    cancel(app, owner);
    #[cfg(windows)]
    {
        shortcut::release(owner);
    }
}
pub async fn configure_shortcut(
    app: tauri::AppHandle,
    owner: String,
    request: ChatScreenshotShortcut,
) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let _ = (app, owner, request);
        Ok(())
    }
    #[cfg(windows)]
    {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let handle = app.clone();
        app.run_on_main_thread(move || {
            let result = if request.enabled && !handle.state::<crate::AppState>().chat.is_enabled()
            {
                Err("聊天功能已关闭".into())
            } else {
                shortcut::configure(handle, owner, request)
            };
            let _ = sender.send(result);
        })
        .map_err(|e| e.to_string())?;
        receiver.await.map_err(|e| e.to_string())?
    }
}

#[cfg(windows)]
struct Encoded {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    png: Vec<u8>,
    preview: Vec<u8>,
}
#[cfg(windows)]
fn encode(output: window::Output) -> Result<Encoded, String> {
    use image::ImageEncoder;
    let action = output.action;
    let exported = render::export_rgba(&output.desktop, &output.editor);
    drop(output);
    crate::native_panel::gfx::release_shared();
    let (width, height, rgba) = exported?;
    if action == window::OutputAction::Copy {
        return Ok(Encoded {
            width,
            height,
            rgba,
            png: Vec::new(),
            preview: Vec::new(),
        });
    }
    let image = image::RgbaImage::from_raw(width, height, rgba).ok_or("截图像素长度无效")?;
    let png_bytes = |pixels: &[u8], w, h| -> Result<Vec<u8>, String> {
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new_with_quality(
            &mut bytes,
            image::codecs::png::CompressionType::Fast,
            image::codecs::png::FilterType::Adaptive,
        )
        .write_image(pixels, w, h, image::ExtendedColorType::Rgba8)
        .map_err(|e| format!("编码截图失败：{e}"))?;
        Ok(bytes)
    };
    let png = png_bytes(image.as_raw(), width, height)?;
    let preview = if action == window::OutputAction::Attach {
        let thumb = image::imageops::thumbnail(&image, 256, 256);
        png_bytes(thumb.as_raw(), thumb.width(), thumb.height())?
    } else {
        Vec::new()
    };
    Ok(Encoded {
        width,
        height,
        rgba: image.into_raw(),
        png,
        preview,
    })
}
