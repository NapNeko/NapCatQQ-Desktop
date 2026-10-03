// 任务栏 / Alt+Tab / 窗口标题区图标(与通知区托盘无关)
// 全部 embed,安装包不落 icons 目录。

use tauri::AppHandle;
use tauri::Manager;
use tauri::image::Image;

fn embed(name: &str) -> Option<Image<'static>> {
    let bytes: &[u8] = match name {
        "256x256.png" => include_bytes!("../icons/256x256.png").as_ref(),
        "128x128.png" => include_bytes!("../icons/128x128.png").as_ref(),
        "64x64.png" => include_bytes!("../icons/64x64.png").as_ref(),
        "48x48.png" => include_bytes!("../icons/48x48.png").as_ref(),
        "32x32.png" => include_bytes!("../icons/32x32.png").as_ref(),
        _ => return None,
    };
    Image::from_bytes(bytes).ok().map(|i| i.to_owned())
}

/// 供主窗口与轻量模式重建窗口使用;优先高分辨率图标,避免高 DPI 显示器模糊
pub fn main_window_icon(_app: &AppHandle) -> Result<Image<'static>, String> {
    embed("256x256.png")
        .or_else(|| embed("128x128.png"))
        .or_else(|| embed("64x64.png"))
        .or_else(|| embed("48x48.png"))
        .or_else(|| embed("32x32.png"))
        .ok_or_else(|| "窗口图标缺失：请执行 pnpm icons:gen".to_string())
}

pub fn apply_main_window_icon(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(crate::lightweight::MAIN_WINDOW_LABEL) {
        apply_window_icon(app, &window)?;
    }
    Ok(())
}

pub fn apply_window_icon(app: &AppHandle, window: &tauri::WebviewWindow) -> Result<(), String> {
    window
        .set_icon(main_window_icon(app)?)
        .map_err(|error| error.to_string())?;
    #[cfg(all(windows, debug_assertions))]
    {
        let hwnd = window.hwnd().map_err(|error| error.to_string())?;
        // 只分离开发窗口的任务栏身份；配置 identifier 和通知的进程身份保持原值。
        let identity = format!("{}.Development", app.config().identifier);
        set_taskbar_identity(windows::Win32::Foundation::HWND(hwnd.0), &identity)
            .map_err(|error| format!("set development taskbar identity failed: {error}"))?;
    }
    Ok(())
}

#[cfg(all(windows, debug_assertions))]
#[allow(unsafe_code)] // Shell 窗口属性 API；字符串使用 COM 分配器，由 PropVariantClear 释放。
fn set_taskbar_identity(
    hwnd: windows::Win32::Foundation::HWND,
    identity: &str,
) -> windows::core::Result<()> {
    use std::mem::ManuallyDrop;
    use windows::Win32::Foundation::PROPERTYKEY;
    use windows::Win32::System::Com::StructuredStorage::{
        PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0, PropVariantClear,
    };
    use windows::Win32::System::Variant::VT_LPWSTR;
    use windows::Win32::UI::Shell::PropertiesSystem::{
        IPropertyStore, SHGetPropertyStoreForWindow,
    };
    use windows::Win32::UI::Shell::SHStrDupW;
    use windows::core::{GUID, PCWSTR};

    let key = PROPERTYKEY {
        fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
        pid: 5,
    };
    let value: Vec<u16> = identity.encode_utf16().chain(Some(0)).collect();
    let store: IPropertyStore = unsafe { SHGetPropertyStoreForWindow(hwnd)? };
    let owned = unsafe { SHStrDupW(PCWSTR(value.as_ptr()))? };
    let mut variant = PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_LPWSTR,
                Anonymous: PROPVARIANT_0_0_0 { pwszVal: owned },
                ..Default::default()
            }),
        },
    };
    unsafe {
        let result = store.SetValue(&key, &variant);
        PropVariantClear(&mut variant)?;
        result
    }
}

#[cfg(all(test, windows, debug_assertions))]
mod tests {
    use super::set_taskbar_identity;
    use windows::Win32::Foundation::{HWND, PROPERTYKEY};
    use windows::Win32::System::Com::StructuredStorage::PropVariantClear;
    use windows::Win32::UI::Shell::PropertiesSystem::{
        IPropertyStore, SHGetPropertyStoreForWindow,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE,
    };
    use windows::core::{GUID, w};

    struct TestWindow(HWND);

    #[allow(unsafe_code)] // 销毁测试创建的不可见 message-only 窗口。
    impl Drop for TestWindow {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }

    #[test]
    #[allow(unsafe_code)] // 验证真实 Shell 属性存储，无可见窗口或任务栏按钮。
    fn development_identity_is_copied_into_the_window_property_store() {
        let window = TestWindow(unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("taskbar identity test"),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                None,
                None,
            )
            .expect("create test window")
        });
        set_taskbar_identity(window.0, "com.napcatqq.desktop.Development").expect("set identity");
        let store: IPropertyStore =
            unsafe { SHGetPropertyStoreForWindow(window.0) }.expect("read store");
        let key = PROPERTYKEY {
            fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
            pid: 5,
        };
        let mut value = unsafe { store.GetValue(&key) }.expect("read AppUserModelID");
        let stored = unsafe { value.Anonymous.Anonymous.Anonymous.pwszVal.to_string() }
            .expect("UTF-16 identity");
        unsafe { PropVariantClear(&mut value) }.expect("release property value");
        unsafe { store.SetValue(&key, &Default::default()) }.expect("clear window property");
        assert_eq!(stored, "com.napcatqq.desktop.Development");
    }
}
