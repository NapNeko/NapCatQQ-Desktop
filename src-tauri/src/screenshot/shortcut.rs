// 系统热键随应用存活，聊天页面只更新捕获目标和偏好。
#![expect(unsafe_code, reason = "RegisterHotKey 与消息窗口 FFI")]
#![warn(clippy::undocumented_unsafe_blocks)]
use crate::native_panel::sys::{self, Hwnd, Message, WndHandler};
use ncd_domain::chat_screenshot::{ChatScreenshotRequest, ChatScreenshotShortcut};
use std::cell::RefCell;
use tauri::Manager;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey,
    UnregisterHotKey, VK_CONTROL, VK_MENU, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, HHOOK, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_HOTKEY,
};
use windows::core::w;

pub const EVENT: &str = "chat-screenshot-shortcut";
struct Registration {
    hwnd: Hwnd,
    app: tauri::AppHandle,
    owner: String,
    attached: bool,
    request: ChatScreenshotShortcut,
    id: i32,
    focus_hook: Option<HHOOK>,
}
impl Drop for Registration {
    fn drop(&mut self) {
        // SAFETY: 该热键和窗口由当前 UI 线程拥有。
        if let Some(hook) = self.focus_hook {
            let _ = unsafe { UnhookWindowsHookEx(hook) };
        } else {
            let _ = unsafe { UnregisterHotKey(Some(self.hwnd.raw()), self.id) };
        }
        self.hwnd.destroy();
    }
}
thread_local! { static HOTKEY:RefCell<Option<Registration>>=const{RefCell::new(None)}; }
pub fn configure(
    app: tauri::AppHandle,
    owner: String,
    request: ChatScreenshotShortcut,
) -> Result<(), String> {
    request.validate()?;
    HOTKEY.with(|cell| {
        if !request.enabled {
            let mut state = cell.borrow_mut();
            if let Some(registration) = state.as_mut().filter(|r| r.owner == owner) {
                if request.release_owner && registration.request.global {
                    registration.attached = false;
                } else {
                    state.take();
                }
            }
            return Ok(());
        }
        let (modifiers, key) = binding(&request)?;
        let mut state = cell.borrow_mut();
        if let Some(registration) = state.as_mut() {
            if binding(&registration.request)? == (modifiers, key)
                && registration.request.global == request.global
            {
                registration.owner = owner;
                registration.attached = true;
                registration.app = app;
                registration.request = request;
                return Ok(());
            }
        }
        sys::register_window_class::<Handler>(w!("NCD.ChatScreenshotShortcut.v1"));
        // SAFETY: 消息窗口没有创建参数指针，也不显示。
        let hwnd = Hwnd::from_raw(
            unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("NCD.ChatScreenshotShortcut.v1"),
                    w!(""),
                    WINDOW_STYLE(0),
                    0,
                    0,
                    0,
                    0,
                    None,
                    None,
                    None,
                    None,
                )
            }
            .map_err(|e| e.to_string())?,
        );
        let focus_hook = if request.global {
            // SAFETY: HWND 在当前 UI 线程创建，失败时立即销毁；旧注册仍保留到新注册完成。
            if unsafe { RegisterHotKey(Some(hwnd.raw()), 1, modifiers, key) }.is_err() {
                hwnd.destroy();
                return Err("快捷键已被其他程序占用".into());
            }
            None
        } else {
            // SAFETY: 只监听本程序 UI 线程，不占用其他程序的组合键，不注入其他进程。
            match unsafe {
                SetWindowsHookExW(
                    WH_KEYBOARD,
                    Some(focused_keyboard),
                    None,
                    GetCurrentThreadId(),
                )
            } {
                Ok(hook) => Some(hook),
                Err(error) => {
                    hwnd.destroy();
                    return Err(format!("设置程序内快捷键失败：{error}"));
                }
            }
        };
        *state = Some(Registration {
            hwnd,
            app,
            owner,
            attached: true,
            request,
            id: 1,
            focus_hook,
        });
        Ok(())
    })
}

unsafe extern "system" fn focused_keyboard(code: i32, key: WPARAM, state: LPARAM) -> LRESULT {
    // SAFETY: WH_KEYBOARD 的参数由系统提供；不读取外部指针，只检查键值和按键状态位。
    if code == 0 && state.0 as usize & ((1usize << 31) | (1usize << 30)) == 0 {
        let handled = HOTKEY.with(|slot| {
            let Ok(slot) = slot.try_borrow() else {
                return false;
            };
            let Some(registration) = slot.as_ref().filter(|r| !r.request.global) else {
                return false;
            };
            let Ok((_, expected)) = binding(&registration.request) else {
                return false;
            };
            let down = |vk| unsafe { GetKeyState(vk) < 0 };
            if key.0 as u32 == expected
                && down(i32::from(VK_CONTROL.0)) == registration.request.control
                && down(i32::from(VK_MENU.0)) == registration.request.alt
                && down(i32::from(VK_SHIFT.0)) == registration.request.shift
                && !down(0x5b)
                && !down(0x5c)
            {
                dispatch(registration);
                return true;
            }
            false
        });
        if handled {
            return LRESULT(1);
        }
    }
    // SAFETY: 原样转发当前钩子参数，不持有返回后失效的系统数据。
    unsafe { CallNextHookEx(None, code, key, state) }
}
pub fn release(owner: &str) {
    HOTKEY.with(|s| {
        let mut state = s.borrow_mut();
        if let Some(registration) = state.as_mut().filter(|r| r.owner == owner) {
            if registration.request.global {
                registration.attached = false;
            } else {
                state.take();
            }
        }
    });
}
fn binding(request: &ChatScreenshotShortcut) -> Result<(HOT_KEY_MODIFIERS, u32), String> {
    let mut modifiers = MOD_NOREPEAT;
    if request.control {
        modifiers |= MOD_CONTROL;
    }
    if request.alt {
        modifiers |= MOD_ALT;
    }
    if request.shift {
        modifiers |= MOD_SHIFT;
    }
    let key = if request.key.len() == 1 {
        u32::from(request.key.as_bytes()[0])
    } else {
        request
            .key
            .strip_prefix('F')
            .and_then(|n| n.parse::<u32>().ok())
            .filter(|n| (1..=12).contains(n))
            .map(|n| 0x70 + n - 1)
            .ok_or("截图快捷键无效")?
    };
    Ok((modifiers, key))
}

fn dispatch(registration: &Registration) {
    let app = registration.app.clone();
    let owner = registration.owner.clone();
    let context = registration.request.context.clone();
    let attached = registration.attached;
    let clipboard_owner = registration.hwnd.key();
    let request = ChatScreenshotRequest {
        hide_window: registration.request.hide_window,
    };
    tauri::async_runtime::spawn(async move {
        let window = attached.then(|| app.get_webview_window(&owner)).flatten();
        let state = app.state::<crate::AppState>();
        if let Err(error) = state.migrate_gate.ensure_idle() {
            tracing::warn!(%error, "截图被数据迁移阻止");
            return;
        }
        let cache = state.chat.screenshot_cache();
        if let Err(error) = super::capture_shortcut(
            app.clone(),
            window,
            cache,
            request,
            context,
            clipboard_owner,
        )
        .await
        {
            tracing::warn!(%owner, %error, "派发截图快捷键失败");
        }
    });
}

struct Handler;
impl WndHandler for Handler {
    fn handle(msg: &Message) -> Option<LRESULT> {
        if msg.id() != WM_HOTKEY {
            return None;
        }
        HOTKEY.with(|s| {
            if let Some(registration) = s.borrow().as_ref()
                && msg.wparam() == registration.id as usize
                && msg.hwnd() == registration.hwnd
            {
                dispatch(registration);
            }
        });
        Some(LRESULT(0))
    }
}
