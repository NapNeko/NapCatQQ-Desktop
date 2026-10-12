// 按已绑定 QQ 的可访问性控件定位二维码，不识别全屏图案或读取账号缓存。
#![expect(unsafe_code, reason = "Windows 进程、UIA 和 WinRT FFI")]

use base64::Engine;
use image::ImageEncoder;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{
    GetCurrentProcessId, GetProcessTimes, OpenProcess, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize};
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    SW_SHOWNOACTIVATE, ShowWindowAsync,
};
use windows::core::{BOOL, Interface, PWSTR};

#[derive(Clone)]
pub(super) struct ProcessBinding {
    pub pid: u32,
    executable: PathBuf,
    creation: u64,
}
pub(super) enum CaptureOutcome {
    Image(String),
    Expired,
    Scanned,
    LoginDetected,
    Loading,
    Unavailable,
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
struct Apartment(DPI_AWARENESS_CONTEXT);
impl Apartment {
    fn new() -> Result<Self, String> {
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(|_| "uia_apartment_failed")?;
        let old =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        Ok(Self(old))
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            let _ = SetThreadDpiAwarenessContext(self.0);
            RoUninitialize();
        }
    }
}

fn process_identity(pid: u32) -> Result<(PathBuf, u64), String> {
    // SAFETY: 查询句柄仅用于读身份，Handle 在所有退出路径释放。
    unsafe {
        let mut own = 0;
        let mut target = 0;
        ProcessIdToSessionId(GetCurrentProcessId(), &mut own)
            .map_err(|_| "process_session_unknown")?;
        ProcessIdToSessionId(pid, &mut target).map_err(|_| "process_session_unknown")?;
        if own != target {
            return Err("different_session".into());
        }
        let handle = Handle(
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
                .map_err(|_| "process_unavailable")?,
        );
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        GetProcessTimes(handle.0, &mut creation, &mut exit, &mut kernel, &mut user)
            .map_err(|_| "process_identity_unknown")?;
        let mut buffer = vec![0u16; 32768];
        let mut len = buffer.len() as u32;
        QueryFullProcessImageNameW(
            handle.0,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut len,
        )
        .map_err(|_| "process_identity_unknown")?;
        let executable = std::fs::canonicalize(String::from_utf16_lossy(&buffer[..len as usize]))
            .map_err(|_| "process_identity_unknown")?;
        Ok((
            executable,
            ((creation.dwHighDateTime as u64) << 32) | u64::from(creation.dwLowDateTime),
        ))
    }
}
pub(super) fn bind(pid: u32, expected: &Path) -> Result<ProcessBinding, String> {
    let expected = std::fs::canonicalize(expected).map_err(|_| "process_identity_unknown")?;
    let (executable, creation) = process_identity(pid)?;
    if !executable
        .to_string_lossy()
        .eq_ignore_ascii_case(&expected.to_string_lossy())
    {
        return Err("process_identity_changed".into());
    }
    Ok(ProcessBinding {
        pid,
        executable,
        creation,
    })
}
fn verify(binding: &ProcessBinding) -> Result<(), String> {
    let (exe, creation) = process_identity(binding.pid)?;
    if creation != binding.creation
        || !exe
            .to_string_lossy()
            .eq_ignore_ascii_case(&binding.executable.to_string_lossy())
    {
        return Err("process_identity_changed".into());
    }
    Ok(())
}
struct WindowSearch {
    pid: u32,
    windows: Vec<isize>,
    restore_minimized: bool,
}
unsafe extern "system" fn collect_window(hwnd: HWND, param: LPARAM) -> BOOL {
    // SAFETY: EnumWindows 同步调用，param 指向该调用栈的 WindowSearch。
    unsafe {
        let search = &mut *(param.0 as *mut WindowSearch);
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == search.pid
            && IsWindowVisible(hwnd).as_bool()
            && (search.restore_minimized || !IsIconic(hwnd).as_bool())
        {
            let mut class = [0u16; 128];
            let n = GetClassNameW(hwnd, &mut class);
            let mut cloaked = 0u32;
            let uncloaked =
                DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, (&mut cloaked as *mut u32).cast(), 4)
                    .is_ok()
                    && cloaked == 0;
            if n > 0
                && String::from_utf16_lossy(&class[..n as usize]) == "Chrome_WidgetWin_1"
                && uncloaked
            {
                search.windows.push(hwnd.0 as isize);
            }
        }
    }
    BOOL(1)
}
struct Page {
    bounds: RECT,
    qr: Option<RECT>,
    expired: bool,
    scanned: bool,
    phone_label: bool,
    other_methods: Option<IUIAutomationElement>,
    password: Option<IUIAutomationElement>,
    scan: Option<IUIAutomationElement>,
    refresh: Option<IUIAutomationElement>,
    login_detected: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoginAction {
    OtherMethods,
    Password,
    Scan,
    Refresh,
}
fn login_action(name: &str, kind: UIA_CONTROLTYPE_ID) -> Option<LoginAction> {
    if kind != UIA_ButtonControlTypeId && kind != UIA_HyperlinkControlTypeId {
        return None;
    }
    match name {
        "其他登录方式" => Some(LoginAction::OtherMethods),
        "账密登录" => Some(LoginAction::Password),
        "扫码登录" => Some(LoginAction::Scan),
        "刷新二维码" => Some(LoginAction::Refresh),
        _ => None,
    }
}
#[derive(Clone, Copy, Default)]
struct LoginControls {
    other_methods: bool,
    password: bool,
    scan: bool,
    qr: bool,
    scanned: bool,
    login_detected: bool,
}
impl Page {
    fn controls(&self) -> LoginControls {
        LoginControls {
            other_methods: self.other_methods.is_some(),
            password: self.password.is_some(),
            scan: self.scan.is_some(),
            qr: self.qr.is_some(),
            scanned: self.scanned,
            login_detected: self.login_detected,
        }
    }

    fn control_mut(&mut self, action: LoginAction) -> &mut Option<IUIAutomationElement> {
        match action {
            LoginAction::OtherMethods => &mut self.other_methods,
            LoginAction::Password => &mut self.password,
            LoginAction::Scan => &mut self.scan,
            LoginAction::Refresh => &mut self.refresh,
        }
    }
}
#[derive(Default)]
struct LoginNavigation {
    attempted: u8,
    pending: Option<(LoginAction, LoginControls)>,
}
impl LoginNavigation {
    fn next_action(&mut self, controls: LoginControls) -> Option<LoginAction> {
        if controls.qr || controls.scanned || controls.login_detected {
            return None;
        }
        if let Some((action, previous)) = self.pending {
            let transitioned = match action {
                LoginAction::OtherMethods => {
                    (controls.scan && !previous.scan) || (controls.password && !previous.password)
                }
                LoginAction::Password => controls.scan && !previous.scan,
                LoginAction::Scan | LoginAction::Refresh => false,
            };
            if !transitioned {
                return None;
            }
            self.pending = None;
        }
        for (action, available, bit) in [
            (LoginAction::Scan, controls.scan, 1),
            (LoginAction::Password, controls.password, 2),
            (LoginAction::OtherMethods, controls.other_methods, 4),
        ] {
            if available && self.attempted & bit == 0 {
                self.attempted |= bit;
                self.pending = Some((action, controls));
                return Some(action);
            }
        }
        None
    }
}
fn checkpoint(deadline: Instant, cancel: &CancellationToken) -> Result<(), String> {
    if cancel.is_cancelled() {
        Err("capture_cancelled".into())
    } else if Instant::now() >= deadline {
        Err("capture_timed_out".into())
    } else {
        Ok(())
    }
}
fn page(
    automation: &IUIAutomation,
    hwnd: HWND,
    pid: u32,
    deadline: Instant,
    cancel: &CancellationToken,
) -> Result<Page, String> {
    checkpoint(deadline, cancel)?;
    // SAFETY: UIA 对象在本次 MTA 线程内创建和消费，控件数量和调用时限均有上限。
    unsafe {
        let window = automation
            .ElementFromHandle(hwnd)
            .map_err(|_| "login_window_unavailable")?;
        if window
            .CurrentProcessId()
            .map_err(|_| "login_window_unavailable")?
            != pid as i32
        {
            return Err("window_owner_changed".into());
        }
        let controls = window
            .FindAll(
                TreeScope_Descendants,
                &automation
                    .CreateTrueCondition()
                    .map_err(|_| "login_window_unavailable")?,
            )
            .map_err(|_| "login_window_unavailable")?;
        let count = controls.Length().map_err(|_| "login_window_unavailable")?;
        if count > 200 {
            return Err("not_login_window".into());
        }
        let mut result = Page {
            bounds: window
                .CurrentBoundingRectangle()
                .map_err(|_| "login_window_unavailable")?,
            qr: None,
            expired: false,
            scanned: false,
            phone_label: false,
            other_methods: None,
            password: None,
            scan: None,
            refresh: None,
            login_detected: false,
        };
        for i in 0..count {
            checkpoint(deadline, cancel)?;
            let element = controls
                .GetElement(i)
                .map_err(|_| "login_window_unavailable")?;
            if element
                .CurrentIsOffscreen()
                .map_err(|_| "login_window_unavailable")?
                .as_bool()
            {
                continue;
            }
            let name = element
                .CurrentName()
                .map_err(|_| "login_window_unavailable")?
                .to_string();
            let kind = element
                .CurrentControlType()
                .map_err(|_| "login_window_unavailable")?;
            if (name == "联系人" || name.starts_with("消息")) && kind != UIA_TextControlTypeId
            {
                result.login_detected = true;
                return Ok(result);
            }
            match name.as_str() {
                "手机QQ扫码登录" if kind == UIA_TextControlTypeId => {
                    result.phone_label = true
                }
                "二维码" if kind == UIA_ImageControlTypeId => {
                    if result.qr.is_some() {
                        return Err("ambiguous_qr_control".into());
                    }
                    result.qr = Some(
                        element
                            .CurrentBoundingRectangle()
                            .map_err(|_| "login_window_unavailable")?,
                    );
                }
                "当前二维码已过期" => result.expired = true,
                "已扫码" | "已扫描" | "扫码成功" | "扫描成功" | "请在手机上确认登录" => {
                    result.scanned = true
                }
                _ => {}
            }
            if kind == UIA_TextControlTypeId && name.contains("手机") && name.contains("确认") {
                result.scanned = true;
            }
            if let Some(action) = login_action(&name, kind)
                && element.CurrentIsEnabled().is_ok_and(|v| v.as_bool())
            {
                let control = result.control_mut(action);
                if control.is_some() {
                    return Err("ambiguous_login_control".into());
                }
                *control = Some(element);
            }
        }
        Ok(result)
    }
}
fn invoke(
    element: Option<IUIAutomationElement>,
    action: LoginAction,
    binding: &ProcessBinding,
    deadline: Instant,
    cancel: &CancellationToken,
) -> Result<(), String> {
    checkpoint(deadline, cancel)?;
    verify(binding)?;
    let element = element.ok_or("login_control_missing")?;
    // SAFETY: 仅允许 page() 白名单收集的切页和刷新控件，不触碰缓存账号登录按钮。
    unsafe {
        if element
            .CurrentProcessId()
            .map_err(|_| "login_control_unavailable")?
            != binding.pid as i32
            || element
                .CurrentIsOffscreen()
                .map_err(|_| "login_control_unavailable")?
                .as_bool()
            || !element
                .CurrentIsEnabled()
                .map_err(|_| "login_control_unavailable")?
                .as_bool()
            || login_action(
                &element
                    .CurrentName()
                    .map_err(|_| "login_control_unavailable")?
                    .to_string(),
                element
                    .CurrentControlType()
                    .map_err(|_| "login_control_unavailable")?,
            ) != Some(action)
        {
            return Err("login_control_changed".into());
        }
        checkpoint(deadline, cancel)?;
        element
            .GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
            .and_then(|pattern| pattern.Invoke())
    }
    .map_err(|error| format!("login_action_unavailable:{:08x}", error.code().0 as u32))
}

pub(super) fn capture(
    binding: ProcessBinding,
    refresh: bool,
    cancel: CancellationToken,
) -> CaptureOutcome {
    match capture_inner(&binding, refresh, &cancel) {
        Ok(outcome) => outcome,
        Err(error)
            if error == "not_login_window"
                || error == "ambiguous_login_window"
                || error == "capture_timed_out" =>
        {
            CaptureOutcome::Loading
        }
        Err(_) => CaptureOutcome::Unavailable,
    }
}
fn capture_inner(
    binding: &ProcessBinding,
    refresh: bool,
    cancel: &CancellationToken,
) -> Result<CaptureOutcome, String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let _apartment = Apartment::new()?;
    checkpoint(deadline, cancel)?;
    verify(binding)?;
    let automation2: IUIAutomation2 =
        unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER) }
            .map_err(|_| "uia_unavailable")?;
    unsafe {
        automation2
            .SetConnectionTimeout(750)
            .map_err(|_| "uia_unavailable")?;
        automation2
            .SetTransactionTimeout(750)
            .map_err(|_| "uia_unavailable")?;
    }
    let automation: IUIAutomation = automation2.cast().map_err(|_| "uia_unavailable")?;
    let mut search = WindowSearch {
        pid: binding.pid,
        windows: Vec::new(),
        restore_minimized: refresh,
    };
    unsafe {
        EnumWindows(
            Some(collect_window),
            LPARAM((&mut search as *mut WindowSearch) as isize),
        )
    }
    .map_err(|_| "login_window_unavailable")?;
    if search.windows.len() != 1 {
        return Err("ambiguous_login_window".into());
    }
    let hwnd = HWND(search.windows[0] as *mut _);
    if refresh && unsafe { IsIconic(hwnd) }.as_bool() {
        checkpoint(deadline, cancel)?;
        // SAFETY: 仅用户主动打开/刷新时恢复已验证 PID 的窗口，不抢前台焦点。
        if !unsafe { ShowWindowAsync(hwnd, SW_SHOWNOACTIVATE) }.as_bool() {
            return Err("login_window_restore_failed".into());
        }
        while unsafe { IsIconic(hwnd) }.as_bool() {
            checkpoint(deadline, cancel)?;
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    let mut before = page(&automation, hwnd, binding.pid, deadline, cancel)?;
    if before.login_detected {
        return Ok(CaptureOutcome::LoginDetected);
    }
    if refresh {
        if before.scanned {
            return Ok(CaptureOutcome::Scanned);
        }
        if before.qr.is_some() {
            if before.refresh.is_some() {
                invoke(
                    before.refresh.take(),
                    LoginAction::Refresh,
                    binding,
                    deadline,
                    cancel,
                )?;
            } else if before.expired {
                return Ok(CaptureOutcome::Expired);
            }
        } else {
            let mut navigation = LoginNavigation::default();
            loop {
                if before.login_detected {
                    return Ok(CaptureOutcome::LoginDetected);
                }
                if before.scanned {
                    return Ok(CaptureOutcome::Scanned);
                }
                if before.qr.is_some() {
                    break;
                }
                if let Some(action) = navigation.next_action(before.controls()) {
                    invoke(
                        before.control_mut(action).take(),
                        action,
                        binding,
                        deadline,
                        cancel,
                    )?;
                } else if navigation.pending.is_none() {
                    return Ok(CaptureOutcome::Unavailable);
                }
                checkpoint(deadline, cancel)?;
                std::thread::sleep(Duration::from_millis(20));
                verify(binding)?;
                before = page(&automation, hwnd, binding.pid, deadline, cancel)?;
            }
        }
        loop {
            checkpoint(deadline, cancel)?;
            before = page(&automation, hwnd, binding.pid, deadline, cancel)?;
            if before.login_detected {
                return Ok(CaptureOutcome::LoginDetected);
            }
            if before.scanned {
                return Ok(CaptureOutcome::Scanned);
            }
            if before.expired {
                return Ok(CaptureOutcome::Expired);
            }
            if before.phone_label && before.qr.is_some() && !before.expired {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    if before.scanned {
        return Ok(CaptureOutcome::Scanned);
    }
    if before.expired {
        return Ok(CaptureOutcome::Expired);
    }
    if !before.phone_label || before.qr.is_none() {
        return Ok(CaptureOutcome::Unavailable);
    }
    verify(binding)?;
    let frame = super::windows_qr_capture::capture(hwnd, deadline, cancel)?;
    verify(binding)?;
    let after = page(&automation, hwnd, binding.pid, deadline, cancel)?;
    if after.scanned {
        return Ok(CaptureOutcome::Scanned);
    }
    if after.login_detected {
        return Ok(CaptureOutcome::LoginDetected);
    }
    if after.expired {
        return Ok(CaptureOutcome::Expired);
    }
    if !after.phone_label || after.qr != before.qr {
        return Ok(CaptureOutcome::Unavailable);
    }
    let rect = after.qr.ok_or("login_window_unavailable")?;
    let bounds = after.bounds;
    let bound_width = bounds.right - bounds.left;
    let bound_height = bounds.bottom - bounds.top;
    if bound_width <= 0 || bound_height <= 0 {
        return Err("qr_bounds_invalid".into());
    }
    let x = ((rect.left - bounds.left) as f64 * f64::from(frame.width) / f64::from(bound_width))
        .round() as i32;
    let y = ((rect.top - bounds.top) as f64 * f64::from(frame.height) / f64::from(bound_height))
        .round() as i32;
    let width = ((rect.right - rect.left) as f64 * f64::from(frame.width) / f64::from(bound_width))
        .round() as i32;
    let height = ((rect.bottom - rect.top) as f64 * f64::from(frame.height)
        / f64::from(bound_height))
    .round() as i32;
    if x < 0
        || y < 0
        || !(64..=1024).contains(&width)
        || !(64..=1024).contains(&height)
        || x + width > frame.width as i32
        || y + height > frame.height as i32
    {
        return Err("qr_bounds_invalid".into());
    }
    let mut rgba = vec![0u8; width as usize * height as usize * 4];
    for row in 0..height as usize {
        checkpoint(deadline, cancel)?;
        let start = ((y as usize + row) * frame.width as usize + x as usize) * 4;
        let target = &mut rgba[row * width as usize * 4..(row + 1) * width as usize * 4];
        target.copy_from_slice(&frame.bgra[start..start + target.len()]);
        for pixel in target.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
    }
    let dark = rgba
        .chunks_exact(4)
        .filter(|pixel| pixel[..3].iter().all(|value| *value < 100))
        .count();
    let density = dark as f64 / (width as f64 * height as f64);
    if !(0.08..=0.7).contains(&density) {
        rgba.fill(0);
        return Ok(CaptureOutcome::Loading);
    }
    let mut bytes = Cursor::new(Vec::new());
    let encoded = image::codecs::png::PngEncoder::new(&mut bytes).write_image(
        &rgba,
        width as u32,
        height as u32,
        image::ExtendedColorType::Rgba8,
    );
    rgba.fill(0);
    encoded.map_err(|_| "qr_encode_failed")?;
    checkpoint(deadline, cancel)?;
    let payload = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes.get_ref())
    );
    bytes.get_mut().fill(0);
    if payload.len() > 128 * 1024 {
        return Err("qr_payload_too_large".into());
    }
    Ok(CaptureOutcome::Image(payload))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_account_submit_and_management_controls_are_never_invoked() {
        for name in ["登录", "账号管理", "删除账号", "添加账号", "+"] {
            assert_eq!(login_action(name, UIA_ButtonControlTypeId), None);
        }
    }

    #[test]
    fn other_methods_link_can_start_account_picker_navigation() {
        assert_eq!(
            login_action("其他登录方式", UIA_HyperlinkControlTypeId),
            Some(LoginAction::OtherMethods)
        );
    }

    #[test]
    fn matching_navigation_caption_is_not_an_action() {
        assert_eq!(login_action("其他登录方式", UIA_TextControlTypeId), None);
    }

    #[test]
    fn account_picker_navigation_waits_for_each_actual_page_transition() {
        let mut navigation = LoginNavigation::default();
        let accounts = LoginControls {
            other_methods: true,
            ..LoginControls::default()
        };
        assert_eq!(
            navigation.next_action(accounts),
            Some(LoginAction::OtherMethods)
        );
        assert_eq!(navigation.next_action(accounts), None);
        let password = LoginControls {
            password: true,
            ..LoginControls::default()
        };
        assert_eq!(
            navigation.next_action(password),
            Some(LoginAction::Password)
        );
        assert_eq!(navigation.next_action(password), None);
        let scan = LoginControls {
            scan: true,
            ..LoginControls::default()
        };
        assert_eq!(navigation.next_action(scan), Some(LoginAction::Scan));
        assert_eq!(navigation.next_action(scan), None);
    }

    #[test]
    fn direct_scan_entry_does_not_detour_through_saved_accounts() {
        let mut navigation = LoginNavigation::default();
        assert_eq!(
            navigation.next_action(LoginControls {
                scan: true,
                password: true,
                other_methods: true,
                ..LoginControls::default()
            }),
            Some(LoginAction::Scan)
        );
    }

    #[test]
    fn qr_confirmation_and_logged_in_pages_stop_navigation() {
        for terminal in [
            LoginControls {
                qr: true,
                ..LoginControls::default()
            },
            LoginControls {
                scanned: true,
                ..LoginControls::default()
            },
            LoginControls {
                login_detected: true,
                ..LoginControls::default()
            },
        ] {
            let mut navigation = LoginNavigation::default();
            assert_eq!(
                navigation.next_action(LoginControls {
                    other_methods: true,
                    ..LoginControls::default()
                }),
                Some(LoginAction::OtherMethods)
            );
            assert_eq!(
                navigation.next_action(LoginControls {
                    scan: true,
                    ..terminal
                }),
                None
            );
        }
    }

    #[test]
    fn process_binding_rejects_a_reused_identity() {
        let mut binding = bind(
            std::process::id(),
            &std::env::current_exe().expect("test executable"),
        )
        .expect("own process binding");
        binding.creation = binding.creation.saturating_sub(1);
        assert!(verify(&binding).is_err());
    }

    #[test]
    fn cancellation_prevents_reading_controls_or_pixels() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(checkpoint(Instant::now() + Duration::from_secs(5), &cancel).is_err());
    }

    #[test]
    #[ignore = "requires an explicitly owned live QQ trial"]
    fn owned_qq_window_capture_smoke() {
        let pid = std::env::var("NCD_QR_SMOKE_PID")
            .expect("owned trial PID")
            .parse::<u32>()
            .expect("PID");
        let exe = PathBuf::from(std::env::var("NCD_QR_SMOKE_EXE").expect("owned trial executable"));
        let root = exe.parent().and_then(Path::parent).expect("trial root");
        assert!(
            root.file_name()
                .expect("root name")
                .to_string_lossy()
                .starts_with("ncd-sl-windows-live-qq-")
        );
        let binding = bind(pid, &exe).expect("bound owned QQ");
        let outcome =
            capture_inner(&binding, true, &CancellationToken::new()).expect("owned native capture");
        let CaptureOutcome::Image(payload) = outcome else {
            panic!("owned QQ did not expose a QR image");
        };
        let encoded = payload
            .strip_prefix("data:image/png;base64,")
            .expect("PNG data URI");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .expect("PNG bytes");
        let image = image::load_from_memory(&bytes).expect("valid PNG");
        assert!((64..=1024).contains(&image.width()));
        assert!((64..=1024).contains(&image.height()));
    }
}
