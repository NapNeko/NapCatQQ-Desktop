//! QQ / Chromium / Electron 控制台噪声（NC 与 SL 共用）
//!
//! 两类后端都跑在官方 QQ 壳上，stdout/nohup 会混入同一类启动杂音。
//! 只抽象「QQ 宿主噪声」；NapCat / SnowLuma 业务噪声各自写在对应模块。
//!
//! 必须用有状态 filter：hotUpdate JSON、Electron 堆栈是多行块，逐行 contains
//! 只会删掉首行，留下 "baseVersion" / at Session.emit 碎片。

/// 单行是否为 QQ 宿主噪声（无跨行状态；测试与简单判断用）
pub fn is_qq_console_noise_line(line: &str) -> bool {
    QqConsoleNoiseFilter::new().is_noise_stateless(line)
}

/// 跨行 QQ 宿主噪声（hotUpdate JSON、Electron 堆栈续行）
#[derive(Debug, Default, Clone)]
pub struct QqConsoleNoiseFilter {
    /// 正在吞 hotUpdate / 同类 JSON 体：`{`…`}` 深度
    json_depth: i32,
    /// 刚吞掉 Electron/IPC 头，后续 `at ...` 堆栈也丢
    drop_stack: bool,
}

impl QqConsoleNoiseFilter {
    pub fn new() -> Self {
        Self::default()
    }

    /// 有状态：当前行是否应丢弃（会更新内部 JSON/堆栈状态）
    pub fn is_noise(&mut self, line: &str) -> bool {
        let t = line.trim();
        if t.is_empty() {
            return self.drop_stack || self.json_depth > 0;
        }

        if self.json_depth > 0 {
            self.json_depth += brace_delta(t);
            if self.json_depth < 0 {
                self.json_depth = 0;
            }
            return true;
        }

        if self.drop_stack {
            if is_js_stack_frame(t) {
                return true;
            }
            self.drop_stack = false;
        }

        if self.is_noise_stateless(t) {
            if is_hot_update_json_header(t) || t == "{" {
                let d = brace_delta(t);
                if d > 0 {
                    self.json_depth = d;
                } else if t == "{" {
                    self.json_depth = 1;
                }
            }
            if is_electron_stack_header(t) {
                self.drop_stack = true;
            }
            return true;
        }

        if is_orphan_json_fragment(t) {
            let d = brace_delta(t);
            if d > 0 {
                self.json_depth = d;
            } else if t == "{" {
                self.json_depth = 1;
            }
            return true;
        }
        if is_orphan_stack_noise(t) {
            return true;
        }

        false
    }

    fn is_noise_stateless(&self, line: &str) -> bool {
        if line.is_empty() {
            return true;
        }
        if is_js_loaded_line(line) {
            return true;
        }
        if is_gpu_noise(line) {
            return true;
        }
        if is_qq_startup_noise(line) {
            return true;
        }
        if is_bugly_noise(line) {
            return true;
        }
        if is_dbus_noise(line) {
            return true;
        }
        if is_electron_ipc_noise(line) {
            return true;
        }
        if is_qq_hot_update_noise(line) {
            return true;
        }
        if is_dropped_frame_or_long_task(line) {
            return true;
        }
        if is_crashpad_or_chromium_host_noise(line) {
            return true;
        }
        if is_native_crash_dump_noise(line) {
            return true;
        }
        false
    }
}

fn brace_delta(line: &str) -> i32 {
    let mut d = 0i32;
    for ch in line.chars() {
        match ch {
            '{' => d += 1,
            '}' => d -= 1,
            _ => {}
        }
    }
    d
}

fn is_js_stack_frame(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("at ") || t.starts_with("at\t")
}

fn is_hot_update_json_header(line: &str) -> bool {
    line.contains("[QQ hotUpdate]")
        || line.contains("hotUpdateApi ")
        || line.contains("startAutoUpdate")
}

fn is_electron_stack_header(line: &str) -> bool {
    line.contains("No handler registered for 'get-remote-win'")
        || line.contains("Error occurred in handler for 'get-remote-win'")
        || line.contains("node:electron/js2c/browser_init")
}

fn is_orphan_json_fragment(line: &str) -> bool {
    let t = line.trim();
    if t == "{" || t == "}" || t == "}," {
        return true;
    }
    const KEYS: &[&str] = &[
        "\"baseVersion\"",
        "\"curVersion\"",
        "\"prevVersion\"",
        "\"onErrorVersions\"",
        "\"buildId\"",
        "\"unzipRetryCount\"",
        "\"updateInfo\"",
        "\"needUpdate\"",
    ];
    KEYS.iter().any(|k| t.starts_with(k))
}

fn is_orphan_stack_noise(line: &str) -> bool {
    let t = line.trim();
    if !t.starts_with("at ") {
        return false;
    }
    t.contains("node:electron")
        || t.contains("node:events")
        || t.contains("Session.emit")
        || t.contains("get-remote-win")
        || t.contains("browser_init")
        || t.contains("js2c/")
}

fn is_js_loaded_line(line: &str) -> bool {
    let t = line.trim();
    let bytes = t.as_bytes();
    if bytes.len() < 11 {
        return false;
    }
    let mut i = 0;
    if !bytes[i].is_ascii_digit() {
        return false;
    }
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    t.get(i..) == Some(" js loaded")
}

fn is_gpu_noise(line: &str) -> bool {
    line.contains("Exiting GPU process")
        || line.contains("viz_main_impl")
        || line.contains("gles2_cmd_decoder")
        || line.contains("gl_utils.cc")
        || line.contains("GPU stall due to ReadPixels")
        || line.contains("enable-unsafe-swiftshader")
        || line.contains("software WebGL has been deprecated")
        || line.contains("GroupMarkerNotSet")
}

fn is_qq_startup_noise(line: &str) -> bool {
    const KEYS: &[&str] = &[
        "version_config_filename",
        "app_package_filename",
        "config_build_id",
        "config_base_version",
        "config_current_version",
        "app_build_version",
        "not mini app",
        "[preload] succeeded",
        "resourcesPath:",
    ];
    KEYS.iter().any(|k| line.contains(k))
}

fn is_bugly_noise(line: &str) -> bool {
    let t = line.trim();
    if matches!(
        t,
        "SetLogger"
            | "fatalSetup"
            | "InitBuglyManager"
            | "UploadBugly"
            | "registBugly"
            | "regist native handler"
    ) {
        return true;
    }
    if t.starts_with("setParam/")
        || t.starts_with("StartWithOptions ")
        || t.starts_with("PostDelayedTask ")
        || t.starts_with("GetDllPath:")
        || t.starts_with("pub_key_path:")
        || t.starts_with("BuglyManager/")
        || t.starts_with("registBugly/")
    {
        return true;
    }
    const KEYS: &[&str] = &[
        "linux-bugly",
        "BuglyManager",
        "BuglyService",
        "NativeCrashHandler",
        "InitBuglyManager",
        "UploadBugly",
        "registBugly",
        "registSignalHandler",
        "regist native handler",
        "init bugly",
        "rqd_record",
        "getCrashDetailBeanFromRecord",
        "uploadCrashEvent",
        "get null crashDetailBean",
    ];
    KEYS.iter().any(|k| line.contains(k))
}

fn is_dbus_noise(line: &str) -> bool {
    line.contains("Failed to connect to the bus")
        || line.contains("dbus/bus.cc")
        || line.contains("dbus/object_proxy.cc")
        || line.contains("/tmp/dbus-")
}

fn is_electron_ipc_noise(line: &str) -> bool {
    is_electron_stack_header(line)
        || (line.contains("node:electron/js2c/browser_init") && line.contains("at Session"))
}

fn is_qq_hot_update_noise(line: &str) -> bool {
    is_hot_update_json_header(line) || line.contains("app 启动分界线")
}

fn is_dropped_frame_or_long_task(line: &str) -> bool {
    line.contains("DroppedFrame(") || line.contains("LongTask(")
}

fn is_crashpad_or_chromium_host_noise(line: &str) -> bool {
    if line.contains("Crashpad") || line.contains("crashpad/") {
        return true;
    }
    if line.contains("file_io_posix.cc") || line.contains("file_io_win.cc") {
        return true;
    }
    if line.contains(":ERROR:")
        && (line.contains("third_party/")
            || line.contains("components/")
            || line.contains("content/")
            || line.contains("chrome/")
            || line.contains("gpu/")
            || line.contains("viz/"))
    {
        return true;
    }
    false
}

/// Bugly / 原生崩溃上报刷屏：/proc/maps、$$pc 堆栈、EupInfo 流水
fn is_native_crash_dump_noise(line: &str) -> bool {
    let t = line.trim();
    // 235,0665,Read map line: 7e80...-/usr/lib/...
    if t.contains("Read map line:") {
        return true;
    }
    // $$00    pc 000000000702e51e    /home/.../qq
    if t.starts_with("$$") && (t.contains(" pc ") || t.contains("\tpc ")) {
        return true;
    }
    // Bugly native record 流水
    if t.contains("Record EupInfo")
        || t.contains("EupInfo has been recorded")
        || t.contains("Record native key-value")
        || t.contains("Native key-value list")
        || t.contains("Record native log")
        || t.contains("Native log has")
        || t.contains("native_record_lock")
        || t.contains("/crash_files")
        || t.contains("crash_files/")
    {
        return true;
    }
    // 编号前缀 + 崩溃相关动作：235,0005,Try to unlock file: ...
    if looks_like_bugly_numbered_line(t)
        && (t.contains("Try to unlock file")
            || t.contains("Successfully unlock file")
            || t.contains("Successfully lock file")
            || t.contains("Try to lock file"))
    {
        return true;
    }
    false
}

/// `235,0665,...` 这类 Bugly 序号前缀（业务日志不会长这样）
fn looks_like_bugly_numbered_line(line: &str) -> bool {
    let bytes = line.as_bytes();
    if bytes.len() < 5 {
        return false;
    }
    let mut i = 0;
    if !bytes[i].is_ascii_digit() {
        return false;
    }
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b',' {
        return false;
    }
    i += 1;
    if i >= bytes.len() || !bytes[i].is_ascii_digit() {
        return false;
    }
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    i < bytes.len() && bytes[i] == b','
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_qq_startup_gpu_js() {
        assert!(is_qq_console_noise_line(
            "version_config_filename :/home/ubuntu/.config/QQ/versions/config.json"
        ));
        assert!(is_qq_console_noise_line("not mini app."));
        assert!(is_qq_console_noise_line(
            "Exiting GPU process due to errors during initialization"
        ));
        assert!(is_qq_console_noise_line("141 js loaded"));
        assert!(is_qq_console_noise_line(
            "[preload] succeeded. /path/major.node"
        ));
    }

    #[test]
    fn drops_dbus_bugly_hotupdate_frames() {
        assert!(is_qq_console_noise_line(
            "Failed to connect to the bus: Failed to connect to socket /tmp/dbus-xxx"
        ));
        assert!(is_qq_console_noise_line("linux-bugly: init bugly ..."));
        assert!(is_qq_console_noise_line(
            "19:33:45.691 › [19f50f40f80][QQ hotUpdate] ------------ app 启动分界线 ------------"
        ));
        assert!(is_qq_console_noise_line(
            "[231224][34576700564]DroppedFrame(1): host_id=1"
        ));
        assert!(is_qq_console_noise_line(
            "[231307][34578192022]LongTask(2): duration=1476ms"
        ));
    }

    #[test]
    fn keeps_business_looking_lines() {
        assert!(!is_qq_console_noise_line(
            "07-11 19:00:00 [info] plugin setParam ok"
        ));
        assert!(!is_qq_console_noise_line(
            "[info] [NapCat] [WebUi] WebUi User Panel Url: http://127.0.0.1:6099/webui?token=x"
        ));
        assert!(!is_qq_console_noise_line("SnowLuma daemon ready on 5099"));
        assert!(!is_qq_console_noise_line(
            "07-11 19:00:00 [error] QIAO | 发生错误 Error: Timeout"
        ));
    }

    #[test]
    fn drops_hotupdate_json_body_statefully() {
        let mut f = QqConsoleNoiseFilter::new();
        assert!(f.is_noise("19:33:45 › [QQ hotUpdate] ------------ app 启动分界线 ------------"));
        assert!(f.is_noise("{"));
        assert!(f.is_noise("\"baseVersion\": \"3.2.25-45758\","));
        assert!(f.is_noise("\"curVersion\": \"3.2.25-45758\","));
        assert!(f.is_noise("\"prevVersion\": \"\","));
        assert!(f.is_noise("\"onErrorVersions\": [],"));
        assert!(f.is_noise("\"buildId\": \"45758\","));
        assert!(f.is_noise("\"unzipRetryCount\": 0"));
        assert!(f.is_noise("}"));
        assert!(!f.is_noise("07-11 19:00:00 [info] [NapCat] [Core] NapCat.Core Version: 4.18.9"));
    }

    #[test]
    fn drops_orphan_json_and_stack_and_crashpad() {
        let mut f = QqConsoleNoiseFilter::new();
        assert!(f.is_noise("\"baseVersion\": \"3.2.25-45758\","));
        assert!(f.is_noise("at Session.emit (node:events:518:28)"));
        assert!(f.is_noise(
            "[0711/194648.699288:ERROR:third_party/crashpad/crashpad/util/file/file_io_posix.cc:153] open /home/ubuntu/.config/QQ/Crashpad/pending/x.lock: File exists (17)"
        ));
    }

    #[test]
    fn drops_electron_stack_after_header() {
        let mut f = QqConsoleNoiseFilter::new();
        assert!(f.is_noise(
            "Error occurred in handler for 'get-remote-win': Error: No handler registered"
        ));
        assert!(f.is_noise("at Session.emit (node:events:518:28)"));
        assert!(f.is_noise("at something (node:electron/js2c/browser_init:2:1)"));
        assert!(!f.is_noise("07-11 19:00:00 [info] QIAO | hello"));
    }

    #[test]
    fn drops_bugly_maps_and_native_pc_frames() {
        assert!(is_qq_console_noise_line(
            "235,0665,Read map line: 7e80babb8000-7e80babbd000 r-xp 00002000 08:01 5441                       /usr/lib/x86_64-linux-gnu/libuuid.so.1.3.0"
        ));
        assert!(is_qq_console_noise_line(
            "$$00    pc 000000000702e51e    /home/ubuntu/Napcat/opt/QQ/qq [x86_64::54d9d484dde88c534a6c13d3f8660b81]"
        ));
        assert!(is_qq_console_noise_line(
            "$$22    pc 000000000002a28b    /usr/lib/x86_64-linux-gnu/libc.so.6 (__libc_start_main+139)"
        ));
        assert!(is_qq_console_noise_line("235,0001,Record EupInfo"));
        assert!(is_qq_console_noise_line(
            "235,0002,EupInfo has been recorded."
        ));
        assert!(is_qq_console_noise_line(
            "235,0005,Try to unlock file: /home/ubuntu/.config/QQ/crash_files//../files/native_record_lock"
        ));
        assert!(!is_qq_console_noise_line(
            "07-11 19:00:00 [info] [NapCat] [Core] ready"
        ));
    }
}
