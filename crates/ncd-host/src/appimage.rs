//! 隔离 AppImage 的库环境，供系统命令和系统浏览器共用。

pub fn clean_appimage_environment(command: &mut std::process::Command) {
    let Some(appdir) = std::env::var_os("APPDIR") else {
        return;
    };
    let mut vars: std::collections::HashMap<_, _> = std::env::vars_os().collect();
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            vars.insert(key.to_owned(), value.to_owned());
        } else {
            vars.remove(key);
        }
    }
    strip_appimage_paths(command, std::path::Path::new(&appdir), vars);
}

fn strip_appimage_paths(
    command: &mut std::process::Command,
    appdir: &std::path::Path,
    vars: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) {
    if !appdir.is_absolute() || appdir == std::path::Path::new("/") {
        return;
    }
    let vars: std::collections::HashMap<_, _> = vars.into_iter().collect();
    // ARGV0 会被 mise 等工具当作自身程序名；APPDIR/APPIMAGE 也只属于 Desktop。
    for key in ["APPDIR", "APPIMAGE", "ARGV0", "OWD"] {
        command.env_remove(key);
    }
    // Gear Lever 等启动器可能把 cwd 留在 FUSE 挂载中。系统命令（如 Podman/pasta）
    // 不应继承它；显式指定的工作目录仍由调用方决定。
    if command.get_current_dir().is_none()
        && std::env::current_dir().is_ok_and(|cwd| cwd.starts_with(appdir))
    {
        let host_cwd = ["OWD", "HOME"]
            .iter()
            .filter_map(|key| vars.get(std::ffi::OsStr::new(key)))
            .map(std::path::PathBuf::from)
            .find(|path| path.is_absolute() && !path.starts_with(appdir) && path.is_dir())
            .unwrap_or_else(|| std::path::PathBuf::from("/"));
        command.env("PWD", &host_cwd).current_dir(host_cwd);
    }
    for (key, value) in vars {
        let Some(name) = key.to_str() else { continue };
        let paths: Vec<_> = match name {
            "PATH" | "LD_LIBRARY_PATH" | "GTK_PATH" | "GIO_EXTRA_MODULES" | "XDG_DATA_DIRS"
            | "QT_PLUGIN_PATH" | "QML2_IMPORT_PATH" => std::env::split_paths(&value).collect(),
            "GDK_PIXBUF_MODULE_FILE"
            | "GSETTINGS_SCHEMA_DIR"
            | "GTK_EXE_PREFIX"
            | "GTK_DATA_PREFIX" => vec![std::path::PathBuf::from(&value)],
            "LD_PRELOAD" => {
                // ld.so 同时接受空格和冒号分隔。
                let Some(value) = value.to_str() else {
                    continue;
                };
                value
                    .split([':', ' ', '\t'])
                    .filter(|s| !s.is_empty())
                    .map(std::path::PathBuf::from)
                    .collect()
            }
            _ => continue,
        };
        let retained: Vec<_> = paths.iter().filter(|p| !p.starts_with(appdir)).collect();
        if retained.len() == paths.len() {
            continue;
        }
        if retained.is_empty() {
            command.env_remove(key);
        } else if let Ok(value) = std::env::join_paths(retained) {
            command.env(key, value);
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{ffi::OsString, path::Path, process::Command};
    #[test]
    fn host_browser_keeps_host_paths_and_session_but_drops_bundled_libraries() {
        let vars = [
            ("APPDIR", "/tmp/.mount_ncd"),
            ("APPIMAGE", "/home/user/Desktop.AppImage"),
            ("ARGV0", "napcatqq_desktop.appimage"),
            ("OWD", "/home/user"),
            ("PATH", "/tmp/.mount_ncd/usr/bin:/usr/bin:/bin"),
            ("LD_LIBRARY_PATH", "/tmp/.mount_ncd/usr/lib:/opt/host/lib"),
            (
                "LD_PRELOAD",
                "/tmp/.mount_ncd/libshim.so /opt/host/libshim.so",
            ),
            ("GDK_PIXBUF_MODULE_FILE", "/tmp/.mount_ncd/loaders.cache"),
            (
                "QT_PLUGIN_PATH",
                "/tmp/.mount_ncd-other/plugins:/usr/lib/qt/plugins",
            ),
            ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus"),
            ("DISPLAY", ":1"),
        ];
        let mut command = Command::new("/usr/bin/env");
        command.env_clear().envs(vars);
        strip_appimage_paths(
            &mut command,
            Path::new("/tmp/.mount_ncd"),
            vars.map(|(k, v)| (OsString::from(k), OsString::from(v))),
        );
        let output = command.output().unwrap();
        assert!(output.status.success());
        let env = String::from_utf8(output.stdout).unwrap();
        for expected in [
            "PATH=/usr/bin:/bin\n",
            "LD_LIBRARY_PATH=/opt/host/lib\n",
            "LD_PRELOAD=/opt/host/libshim.so\n",
            "QT_PLUGIN_PATH=/tmp/.mount_ncd-other/plugins:/usr/lib/qt/plugins\n",
            "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus\n",
            "DISPLAY=:1\n",
        ] {
            assert!(env.contains(expected), "{expected:?} missing from {env}");
        }
        assert!(!env.contains("GDK_PIXBUF_MODULE_FILE="));
        for key in ["APPDIR=", "APPIMAGE=", "ARGV0=", "OWD="] {
            assert!(!env.contains(key), "{key} leaked into the system command");
        }
    }

    #[test]
    fn host_commands_leave_inherited_appimage_mount_but_keep_explicit_cwd() {
        const MARKER: &str = "NCD_APPIMAGE_CWD_FIXTURE";
        if let Some(root) = std::env::var_os(MARKER) {
            let root = std::path::PathBuf::from(root);
            let mut command = Command::new("/bin/pwd");
            clean_appimage_environment(&mut command);
            let output = command.output().unwrap();
            assert!(output.status.success());
            assert_eq!(
                String::from_utf8(output.stdout).unwrap().trim(),
                root.to_str().unwrap()
            );

            let appdir = std::env::current_dir().unwrap();
            let mut explicit = Command::new("/bin/pwd");
            explicit.current_dir(&appdir);
            clean_appimage_environment(&mut explicit);
            let output = explicit.output().unwrap();
            assert!(output.status.success());
            assert_eq!(
                String::from_utf8(output.stdout).unwrap().trim(),
                appdir.to_str().unwrap()
            );
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let appdir = root.path().join("appimage");
        std::fs::create_dir(&appdir).unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "appimage::tests::host_commands_leave_inherited_appimage_mount_but_keep_explicit_cwd",
                "--nocapture",
            ])
            .current_dir(&appdir)
            .env("APPDIR", &appdir)
            .env("OWD", root.path())
            .env(MARKER, root.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}
