//! 读系统剪贴板里的文本（终端右键菜单的「粘贴」用）
//!
//! 网页里的 navigator.clipboard.readText 在 WebView2 上要单独授权，主窗口是从配置建的，开不了
//! 这个开关，一调就弹 WebView2 自己的授权框。Ctrl+V 这种带用户手势的粘贴不经过这里。

#![allow(unsafe_code)]

#[cfg(windows)]
pub fn read_text() -> Result<String, String> {
    use std::time::Duration;

    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    // 标准格式 CF_UNICODETEXT；为这一个常量不值得再开 Ole 特性
    const CF_UNICODETEXT: u32 = 13;

    // SAFETY: 剪贴板句柄只在 Open / Close 之间用；锁住的内存按 GlobalSize 给的大小读，
    // 读到第一个 0 为止
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT).is_err() {
            return Ok(String::new());
        }
        // 别的程序正开着剪贴板时打开会失败，稍等重试几次
        let mut opened = false;
        for _ in 0..10 {
            if OpenClipboard(None).is_ok() {
                opened = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if !opened {
            return Err("剪贴板被别的程序占着，稍后再试".into());
        }
        let text = match GetClipboardData(CF_UNICODETEXT) {
            Ok(handle) => {
                let memory = HGLOBAL(handle.0);
                let ptr = GlobalLock(memory) as *const u16;
                if ptr.is_null() {
                    Err("读不了剪贴板".to_string())
                } else {
                    let units = GlobalSize(memory) / 2;
                    let slice = std::slice::from_raw_parts(ptr, units);
                    let len = slice.iter().position(|c| *c == 0).unwrap_or(units);
                    let text = String::from_utf16_lossy(&slice[..len]);
                    let _ = GlobalUnlock(memory);
                    Ok(text)
                }
            }
            Err(e) => Err(format!("读不了剪贴板：{e}")),
        };
        let _ = CloseClipboard();
        text
    }
}

#[cfg(not(windows))]
pub fn read_text() -> Result<String, String> {
    Err("只在 Windows 上能读剪贴板".into())
}

#[cfg(test)]
#[cfg(windows)]
mod tests {
    /// 读一下本机剪贴板（只读，不改内容也不打印内容）；要桌面会话，默认不跑
    #[test]
    #[ignore]
    fn reads_the_desktop_clipboard() {
        let text = super::read_text();
        assert!(text.is_ok(), "{text:?}");
        println!("clipboard text length: {}", text.map(|t| t.chars().count()).unwrap_or(0));
    }
}
