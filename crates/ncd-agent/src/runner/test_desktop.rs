//! 管理员测试启动器独享的隐藏桌面，不修改原窗口站或桌面的 ACL。
use super::*;
use windows_sys::Win32::System::StationsAndDesktops::*;

pub(super) struct Desktop {
    previous_station: HWINSTA,
    previous_desktop: HDESK,
    station: HWINSTA,
    desktop: HDESK,
    pub(super) path: Vec<u16>,
}

impl Desktop {
    pub(super) fn new(owner: &str) -> Result<Self, String> {
        let name = format!("NcdAgentTest-{}", uuid::Uuid::new_v4());
        let descriptor =
            native::Descriptor::new(&format!("D:P(A;;GA;;;SY)(A;;GA;;;{owner})S:(ML;;NW;;;ME)"))?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        unsafe {
            let previous_station = GetProcessWindowStation();
            let previous_desktop = GetThreadDesktop(GetCurrentThreadId());
            if previous_station.is_null() || previous_desktop.is_null() {
                return Err(native::last_error());
            }
            let station =
                CreateWindowStationW(native::wide(&name).as_ptr(), 0, GENERIC_ALL, &attributes);
            if station.is_null() {
                return Err(native::last_error());
            }
            let mut context = Self {
                previous_station,
                previous_desktop,
                station,
                desktop: null_mut(),
                path: native::wide(&format!("{name}\\Default")),
            };
            if SetProcessWindowStation(station) == 0 {
                return Err(native::last_error());
            }
            context.desktop = CreateDesktopW(
                native::wide("Default").as_ptr(),
                null(),
                null(),
                0,
                GENERIC_ALL,
                &attributes,
            );
            if context.desktop.is_null() {
                return Err(native::last_error());
            }
            Ok(context)
        }
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        unsafe {
            SetProcessWindowStation(self.previous_station);
            SetThreadDesktop(self.previous_desktop);
            if !self.desktop.is_null() {
                CloseDesktop(self.desktop);
            }
            CloseWindowStation(self.station);
        }
    }
}
