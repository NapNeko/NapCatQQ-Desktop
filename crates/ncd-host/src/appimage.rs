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
    }
}
