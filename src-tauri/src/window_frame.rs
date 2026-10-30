//! 无边框窗口圆角：Win11 交给 DWM，Win10 使用随窗口几何更新的原生区域。

use tauri::WebviewWindow;

pub fn apply(window: &WebviewWindow) -> Result<(), String> {
    #[cfg(windows)]
    {
        let (sender, receiver) = std::sync::mpsc::channel();
        let frame_window = window.clone();
        window
            .run_on_main_thread(move || {
                let _ = sender.send(windows::apply(&frame_window));
            })
            .map_err(|error| error.to_string())?;
        receiver.recv().map_err(|error| error.to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = window;
        Ok(())
    }
}

#[cfg(windows)]
mod windows {
    #![expect(unsafe_code, reason = "窗口区域及窗口子类消息回调的 Win32 FFI")]
    #![warn(clippy::undocumented_unsafe_blocks)]

    use std::cell::Cell;
    use std::rc::Rc;

    use ::windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use ::windows::Win32::Graphics::Dwm::{
        DWM_WINDOW_CORNER_PREFERENCE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
        DwmSetWindowAttribute,
    };
    use ::windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, DeleteObject, SetWindowRgn};
    use ::windows::Win32::UI::HiDpi::GetDpiForWindow;
    use ::windows::Win32::UI::Shell::{
        DefSubclassProc, GetWindowSubclass, RemoveWindowSubclass, SetWindowSubclass,
    };
    use ::windows::Win32::UI::WindowsAndMessaging::{
        GWL_STYLE, GetWindowLongPtrW, GetWindowRect, IsIconic, IsZoomed, WM_DPICHANGED,
        WM_NCDESTROY, WM_SIZE, WM_STYLECHANGED, WS_CAPTION, WS_THICKFRAME,
    };
    use tauri::WebviewWindow;

    const SUBCLASS_ID: usize = 1;
    const CORNER_RADIUS_DIP: f64 = 12.0;

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Shape {
        Unclipped,
        Rounded {
            width: i32,
            height: i32,
            radius: i32,
        },
    }

    struct FrameState {
        shape: Cell<Option<Shape>>,
        destroyed: Cell<bool>,
    }

    unsafe fn retain_state(data: usize) -> Rc<FrameState> {
        let pointer = data as *const FrameState;
        // SAFETY: 已安装的子类持有一份 Rc；回调另持一份以跨过重入的销毁消息。
        unsafe {
            Rc::increment_strong_count(pointer);
            Rc::from_raw(pointer)
        }
    }

    pub fn apply(window: &WebviewWindow) -> Result<(), String> {
        let hwnd = window.hwnd().map_err(|error| error.to_string())?;
        let preference: DWM_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND;
        // SAFETY: HWND 来自当前窗口；枚举值及其大小匹配 DWM 属性，指针不被保留。
        if unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                (&raw const preference).cast(),
                size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
            )
        }
        .is_ok()
        {
            return Ok(());
        }

        // TAO 的阴影模式把侧/底 resize 边框留在客户区外。关闭后由 Tauri 的四边
        // resize 子窗口接手，圆角区域就不会裁掉那几条不可见的缩放热区。
        window
            .set_shadow(false)
            .map_err(|error| error.to_string())?;
        let mut state = 0usize;
        // SAFETY: 在窗口线程查询本回调的私有数据；该数据只在 WM_NCDESTROY 释放。
        if unsafe { GetWindowSubclass(hwnd, Some(frame_proc), SUBCLASS_ID, Some(&mut state)) }
            .as_bool()
        {
            // SAFETY: GetWindowSubclass 已确认该 HWND 的子类仍持有原始 Rc。
            let state = unsafe { retain_state(state) };
            return update_region(hwnd, &state);
        }

        let state = Rc::new(FrameState {
            shape: Cell::new(None),
            destroyed: Cell::new(false),
        });
        let data = Rc::into_raw(Rc::clone(&state));
        // SAFETY: 当前运行在 HWND 所属线程；原始 Rc 由 WM_NCDESTROY 释放。
        if !unsafe { SetWindowSubclass(hwnd, Some(frame_proc), SUBCLASS_ID, data as usize) }
            .as_bool()
        {
            // SAFETY: 安装失败，系统未持有此 Rc。
            unsafe { drop(Rc::from_raw(data)) };
            return Err("安装窗口圆角消息处理失败".into());
        }
        update_region(hwnd, &state)
    }

    fn update_region(hwnd: HWND, state: &FrameState) -> Result<(), String> {
        // SAFETY: 只查询本窗口的状态和样式；普通无边框窗口仍保留 TAO 的 frame 样式，
        // 进入全屏才会清除 WS_OVERLAPPEDWINDOW 中的 caption / thickframe。
        let shape = unsafe {
            if IsIconic(hwnd).as_bool() {
                return Ok(());
            }
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
            if IsZoomed(hwnd).as_bool() || style & (WS_CAPTION | WS_THICKFRAME).0 == 0 {
                Shape::Unclipped
            } else {
                let mut rect = RECT::default();
                GetWindowRect(hwnd, &mut rect).map_err(|error| error.to_string())?;
                Shape::Rounded {
                    width: rect.right - rect.left,
                    height: rect.bottom - rect.top,
                    radius: (CORNER_RADIUS_DIP * f64::from(GetDpiForWindow(hwnd)) / 96.0).round()
                        as i32,
                }
            }
        };
        if state.shape.get() == Some(shape) {
            return Ok(());
        }
        // SetWindowRgn 会同步发送窗口位置消息；先记录形状，重入时不重复设置区域。
        let previous = state.shape.replace(Some(shape));
        let result = set_region(hwnd, shape);
        if result.is_err() {
            state.shape.set(previous);
        }
        result
    }

    fn set_region(hwnd: HWND, shape: Shape) -> Result<(), String> {
        // SAFETY: 区域坐标相对整个 HWND，尺寸/半径均为本窗口的物理像素。
        unsafe {
            let region = match shape {
                Shape::Unclipped => None,
                Shape::Rounded {
                    width,
                    height,
                    radius,
                } => {
                    let region = CreateRoundRectRgn(0, 0, width, height, radius * 2, radius * 2);
                    if region.0.is_null() {
                        return Err("创建窗口圆角区域失败".into());
                    }
                    Some(region)
                }
            };
            if SetWindowRgn(hwnd, region, true) == 0 {
                // 成功时系统接管 HRGN；只有失败时才由调用方释放。
                if let Some(region) = region {
                    let _ = DeleteObject(region.into());
                }
                return Err("设置窗口圆角区域失败".into());
            }
        }
        Ok(())
    }

    unsafe extern "system" fn frame_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        id: usize,
        data: usize,
    ) -> LRESULT {
        // SAFETY: 系统在窗口所属线程调用；私有数据由 apply 安装，不捕获 WebviewWindow。
        unsafe {
            let state = retain_state(data);
            if message == WM_NCDESTROY {
                state.destroyed.set(true);
                let _ = RemoveWindowSubclass(hwnd, Some(frame_proc), id);
                // 释放子类持有的 Rc；重入外层回调仍持有自己的引用。
                drop(Rc::from_raw(data as *const FrameState));
                return DefSubclassProc(hwnd, message, wparam, lparam);
            }
            let result = DefSubclassProc(hwnd, message, wparam, lparam);
            if !state.destroyed.get()
                && matches!(message, WM_SIZE | WM_DPICHANGED | WM_STYLECHANGED)
                && let Err(error) = update_region(hwnd, &state)
            {
                tracing::warn!(%error, "更新窗口圆角失败");
            }
            result
        }
    }
}
