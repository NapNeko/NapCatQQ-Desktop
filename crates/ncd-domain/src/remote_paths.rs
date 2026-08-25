//! 远端 Linux Native 路径模板与派生。
//!
//! 启动、安装、入口补丁只许从这里拼 POSIX 路径。禁止在 backend / 工厂再写
//! `{home}/Napcat`。本模块零 I/O。

use crate::remote_inventory::RemoteSelectedPaths;
use crate::snowluma_linux_package::SnowLumaLinuxPackage;

pub const REL_QQ_BIN: &str = "opt/QQ/qq";
pub const REL_QQ_APP: &str = "opt/QQ/resources/app";
pub const REL_QQ_PACKAGE_JSON: &str = "opt/QQ/resources/app/package.json";
pub const REL_LOAD_NAPCAT_JS: &str = "opt/QQ/resources/app/loadNapCat.js";
pub const REL_NAPCAT_MJS: &str = "opt/QQ/resources/app/app_launcher/napcat/napcat.mjs";
pub const REL_NAPCAT_ROOT: &str = "opt/QQ/resources/app/app_launcher/napcat";
pub const SYSTEM_QQ_BIN: &str = "/opt/QQ/qq";
pub const SYSTEM_SNOWLUMA_DIR: &str = "/opt/snowluma";

/// 从 `selected` 展开的具体文件路径（不落盘）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteLinuxDerivedPaths {
    pub home: String,
    pub qq_install_base: Option<String>,
    pub qq_bin: Option<String>,
    pub qq_app_dir: Option<String>,
    pub qq_package_json: Option<String>,
    pub load_napcat_js: Option<String>,
    pub napcat_mjs: Option<String>,
    pub napcat_root: Option<String>,
    pub napcat_config_dir: Option<String>,
    pub snowluma_dir: Option<String>,
    pub snowluma_workspace: Option<String>,
    pub node_bin: Option<String>,
    pub ncd_watch_root: Option<String>,
    pub needs_sudo: bool,
    pub snowluma_linux_package: SnowLumaLinuxPackage,
}

/// POSIX 拼接：`base == "/"` 时不得出现 `//opt/...`
pub fn join_under(base: &str, rel: &str) -> String {
    let base = normalize_posix(base);
    let rel = rel.trim_start_matches('/');
    if rel.is_empty() {
        return base;
    }
    if base.is_empty() || base == "/" {
        format!("/{rel}")
    } else {
        format!("{base}/{rel}")
    }
}

pub fn normalize_posix(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut prev_slash = false;
    for c in path.replace('\\', "/").chars() {
        if c == '/' {
            if prev_slash {
                continue;
            }
            prev_slash = true;
        } else {
            prev_slash = false;
        }
        out.push(c);
    }
    while out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    out
}

pub fn qq_bin(qq_install_base: &str) -> String {
    join_under(qq_install_base, REL_QQ_BIN)
}

pub fn qq_app_dir(qq_install_base: &str) -> String {
    join_under(qq_install_base, REL_QQ_APP)
}

pub fn qq_package_json(qq_install_base: &str) -> String {
    join_under(qq_install_base, REL_QQ_PACKAGE_JSON)
}

pub fn load_napcat_js(qq_install_base: &str) -> String {
    join_under(qq_install_base, REL_LOAD_NAPCAT_JS)
}

pub fn napcat_mjs(qq_install_base: &str) -> String {
    join_under(qq_install_base, REL_NAPCAT_MJS)
}

pub fn napcat_root_under_qq(qq_install_base: &str) -> String {
    join_under(qq_install_base, REL_NAPCAT_ROOT)
}

pub fn napcat_config_dir(napcat_root: &str) -> String {
    join_under(napcat_root, "config")
}

/// `~/.config/QQ` 是用户数据，不是安装根
pub fn is_qq_user_data_path(path: &str) -> bool {
    let n = normalize_posix(path);
    n.contains("/.config/QQ") || n.ends_with("/.config/QQ")
}

/// `{base}/opt/QQ/qq` → base（系统包为 `/`）
pub fn qq_install_base_from_qq_bin(qq_bin: &str) -> Option<String> {
    let p = normalize_posix(qq_bin);
    let suffix = format!("/{REL_QQ_BIN}");
    let prefix = p.strip_suffix(suffix.as_str())?;
    if prefix.is_empty() {
        return Some("/".into());
    }
    if is_qq_user_data_path(prefix) {
        return None;
    }
    Some(prefix.to_string())
}

/// `{workspace}/snowluma` → 父目录为 workspace；`/opt/snowluma` 这类系统目录保持扁平。
pub fn snowluma_workspace_from_dir(snowluma_dir: &str) -> String {
    let p = normalize_posix(snowluma_dir);
    if let Some(parent) = p.strip_suffix("/snowluma") {
        if !parent.is_empty()
            && (parent.ends_with("/workspace") || parent.ends_with("snowluma-workspace"))
        {
            return parent.to_string();
        }
    }
    p
}

pub fn needs_sudo_for_qq(qq_install_base: Option<&str>, qq_bin: Option<&str>) -> bool {
    qq_install_base == Some("/") || qq_bin == Some(SYSTEM_QQ_BIN)
}

/// `{snowluma_dir}/node` 或 `{snowluma_dir}/node/bin/node` 是官方完整包自带运行时。
pub fn is_bundled_snowluma_node(bin: &str, snowluma_dir: Option<&str>) -> bool {
    let bin = normalize_posix(bin);
    if let Some(dir) = snowluma_dir.filter(|s| !s.is_empty()) {
        let bundled = join_under(dir, "node");
        if bin == bundled || bin == join_under(&bundled, "bin/node") {
            return true;
        }
    }
    bin.ends_with("/snowluma/node") || bin.ends_with("/snowluma/node/bin/node")
}

/// 桌面 lite 外置 Node：`{workspace}/node/bin/node`，不是完整包自带的 `{dir}/node`。
pub fn is_portable_lite_node(bin: &str, snowluma_dir: Option<&str>) -> bool {
    let bin = normalize_posix(bin);
    if is_bundled_snowluma_node(&bin, snowluma_dir) {
        return false;
    }
    bin.ends_with("/node/bin/node")
}

/// 完整包：自带 `{dir}/node`，或只发现系统 PATH node / 尚未探到 node。
/// lite：明确选中了 workspace 便携 Node。无 snowluma_dir 时默认完整包。
pub fn infer_snowluma_linux_package(
    selected: Option<&RemoteSelectedPaths>,
) -> SnowLumaLinuxPackage {
    let Some(sel) = selected else {
        return SnowLumaLinuxPackage::Full;
    };
    let Some(dir) = sel.snowluma_dir.as_deref().filter(|s| !s.is_empty()) else {
        return SnowLumaLinuxPackage::Full;
    };
    match sel
        .node_bin
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(bin) if is_bundled_snowluma_node(bin, Some(dir)) => SnowLumaLinuxPackage::Full,
        Some(bin) if is_portable_lite_node(bin, Some(dir)) => SnowLumaLinuxPackage::Lite,
        _ => SnowLumaLinuxPackage::Full,
    }
}

pub fn snowluma_install_candidates(home: &str) -> Vec<String> {
    let home = normalize_posix(home);
    vec![
        join_under(&home, "snowluma-remote/workspace/snowluma"),
        join_under(&home, "Napcat/snowluma-workspace/snowluma"),
        SYSTEM_SNOWLUMA_DIR.to_string(),
    ]
}

pub fn qq_bin_candidates(home: &str) -> Vec<String> {
    let home = normalize_posix(home);
    let mut out = vec![
        join_under(&home, "Napcat/opt/QQ/qq"),
        SYSTEM_QQ_BIN.to_string(),
    ];
    out.dedup();
    out
}

/// 仅组件**安装**在 selected 缺字段时使用。启动禁止调用。
pub fn desktop_default_install_paths(home: &str) -> Result<RemoteSelectedPaths, String> {
    let home = normalize_posix(home.trim());
    if home.is_empty() {
        return Err("远端 $HOME 为空，无法派生默认安装路径".into());
    }
    let qq_install_base = join_under(&home, "Napcat");
    let snowluma_workspace = join_under(&home, "snowluma-remote/workspace");
    let snowluma_dir = join_under(&snowluma_workspace, "snowluma");
    Ok(RemoteSelectedPaths {
        home: home.clone(),
        qq_install_base: Some(qq_install_base.clone()),
        qq_bin: Some(qq_bin(&qq_install_base)),
        napcat_root: Some(napcat_root_under_qq(&qq_install_base)),
        snowluma_dir: Some(snowluma_dir.clone()),
        snowluma_workspace: Some(snowluma_workspace),
        node_bin: Some(join_under(&snowluma_dir, "node")),
        ncd_watch_root: Some(join_under(&home, "ncd-watch")),
        needs_sudo: false,
    })
}

fn nonempty(s: Option<&str>) -> Option<String> {
    s.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(normalize_posix)
}

pub fn derive_remote_linux_paths(selected: &RemoteSelectedPaths) -> RemoteLinuxDerivedPaths {
    let home = normalize_posix(&selected.home);
    let mut qq_install_base = nonempty(selected.qq_install_base.as_deref());
    let mut qq_bin_sel = nonempty(selected.qq_bin.as_deref());
    if qq_install_base.is_none() {
        if let Some(bin) = qq_bin_sel.as_deref() {
            qq_install_base = qq_install_base_from_qq_bin(bin);
        }
    }
    let (qq_app_dir, qq_package_json, load_napcat_js, napcat_mjs) =
        if let Some(base) = qq_install_base.as_deref() {
            if qq_bin_sel.is_none() {
                qq_bin_sel = Some(qq_bin(base));
            }
            (
                Some(qq_app_dir(base)),
                Some(qq_package_json(base)),
                Some(load_napcat_js(base)),
                Some(napcat_mjs(base)),
            )
        } else {
            (None, None, None, None)
        };
    let napcat_root = nonempty(selected.napcat_root.as_deref())
        .or_else(|| qq_install_base.as_deref().map(napcat_root_under_qq));
    let napcat_config_dir = napcat_root.as_deref().map(napcat_config_dir);
    let snowluma_dir = nonempty(selected.snowluma_dir.as_deref());
    let snowluma_workspace = nonempty(selected.snowluma_workspace.as_deref())
        .or_else(|| snowluma_dir.as_deref().map(snowluma_workspace_from_dir));
    let node_bin = nonempty(selected.node_bin.as_deref());
    let ncd_watch_root = nonempty(selected.ncd_watch_root.as_deref());
    let needs_sudo = needs_sudo_for_qq(qq_install_base.as_deref(), qq_bin_sel.as_deref());
    let snowluma_linux_package = infer_snowluma_linux_package(Some(selected));
    RemoteLinuxDerivedPaths {
        home,
        qq_install_base,
        qq_bin: qq_bin_sel,
        qq_app_dir,
        qq_package_json,
        load_napcat_js,
        napcat_mjs,
        napcat_root,
        napcat_config_dir,
        snowluma_dir,
        snowluma_workspace,
        node_bin,
        ncd_watch_root,
        needs_sudo,
        snowluma_linux_package,
    }
}

fn missing_err(kind: &str, selected: &RemoteSelectedPaths) -> String {
    format!(
        "远端未发现 {kind}（home={}, qqInstallBase={:?}, qqBin={:?}, napcatRoot={:?}, snowlumaDir={:?}）。\
         请在组件页安装或填写覆盖路径后重新发现。",
        selected.home,
        selected.qq_install_base,
        selected.qq_bin,
        selected.napcat_root,
        selected.snowluma_dir
    )
}

pub fn require_qq_install_base(selected: &RemoteSelectedPaths) -> Result<String, String> {
    derive_remote_linux_paths(selected)
        .qq_install_base
        .ok_or_else(|| missing_err("QQ 安装树", selected))
}

pub fn require_snowluma_dir(selected: &RemoteSelectedPaths) -> Result<String, String> {
    derive_remote_linux_paths(selected)
        .snowluma_dir
        .ok_or_else(|| missing_err("SnowLuma framework", selected))
}

pub fn require_napcat_root(selected: &RemoteSelectedPaths) -> Result<String, String> {
    derive_remote_linux_paths(selected)
        .napcat_root
        .ok_or_else(|| missing_err("NapCat", selected))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_under_system_root_has_no_double_slash() {
        assert_eq!(join_under("/", REL_QQ_BIN), "/opt/QQ/qq");
        assert_eq!(
            join_under("/", REL_QQ_PACKAGE_JSON),
            "/opt/QQ/resources/app/package.json"
        );
        assert_eq!(
            join_under("/home/u/Napcat", REL_QQ_BIN),
            "/home/u/Napcat/opt/QQ/qq"
        );
    }

    #[test]
    fn qq_install_base_from_system_and_custom_bin() {
        assert_eq!(
            qq_install_base_from_qq_bin("/opt/QQ/qq").as_deref(),
            Some("/")
        );
        assert_eq!(
            qq_install_base_from_qq_bin("/data/qq/opt/QQ/qq").as_deref(),
            Some("/data/qq")
        );
        assert_eq!(
            qq_install_base_from_qq_bin("/home/u/Napcat/opt/QQ/qq").as_deref(),
            Some("/home/u/Napcat")
        );
        assert!(qq_install_base_from_qq_bin("/home/u/.config/QQ/opt/QQ/qq").is_none());
        assert!(is_qq_user_data_path("/home/u/.config/QQ"));
    }

    #[test]
    fn derive_fills_base_from_system_qq_bin() {
        let selected = RemoteSelectedPaths {
            home: "/root".into(),
            qq_bin: Some("/opt/QQ/qq".into()),
            ..RemoteSelectedPaths::default()
        };
        let d = derive_remote_linux_paths(&selected);
        assert_eq!(d.qq_install_base.as_deref(), Some("/"));
        assert_eq!(
            d.qq_package_json.as_deref(),
            Some("/opt/QQ/resources/app/package.json")
        );
        assert!(d.needs_sudo);
        assert_ne!(
            d.qq_package_json.as_deref(),
            Some("/root/Napcat/opt/QQ/resources/app/package.json")
        );
    }

    #[test]
    fn derive_fills_bin_from_install_base() {
        let selected = RemoteSelectedPaths {
            home: "/home/u".into(),
            qq_install_base: Some("/home/u/Napcat".into()),
            ..RemoteSelectedPaths::default()
        };
        let d = derive_remote_linux_paths(&selected);
        assert_eq!(d.qq_bin.as_deref(), Some("/home/u/Napcat/opt/QQ/qq"));
        assert!(!d.needs_sudo);
    }

    #[test]
    fn desktop_defaults_use_home_napcat_but_require_does_not_fallback() {
        let d = desktop_default_install_paths("/root").unwrap();
        assert_eq!(d.qq_install_base.as_deref(), Some("/root/Napcat"));
        assert_eq!(d.qq_bin.as_deref(), Some("/root/Napcat/opt/QQ/qq"));
        assert_eq!(
            d.snowluma_dir.as_deref(),
            Some("/root/snowluma-remote/workspace/snowluma")
        );
        let empty = RemoteSelectedPaths {
            home: "/root".into(),
            ..RemoteSelectedPaths::default()
        };
        let err = require_qq_install_base(&empty).unwrap_err();
        assert!(err.contains("home=/root"));
        assert!(err.contains("未发现"));
        assert!(!err.contains("$HOME/Napcat"));
    }

    #[test]
    fn empty_home_rejects_default_install_paths() {
        assert!(desktop_default_install_paths("").is_err());
        assert!(desktop_default_install_paths("   ").is_err());
    }

    #[test]
    fn infer_package_bundled_vs_lite() {
        let full = RemoteSelectedPaths {
            home: "/home/u".into(),
            snowluma_dir: Some("/opt/snowluma".into()),
            node_bin: Some("/opt/snowluma/node".into()),
            ..RemoteSelectedPaths::default()
        };
        assert_eq!(
            infer_snowluma_linux_package(Some(&full)),
            SnowLumaLinuxPackage::Full
        );
        let lite = RemoteSelectedPaths {
            home: "/home/u".into(),
            snowluma_dir: Some("/home/u/snowluma-remote/workspace/snowluma".into()),
            node_bin: Some("/home/u/snowluma-remote/workspace/node/bin/node".into()),
            ..RemoteSelectedPaths::default()
        };
        assert_eq!(
            infer_snowluma_linux_package(Some(&lite)),
            SnowLumaLinuxPackage::Lite
        );
        assert_eq!(
            infer_snowluma_linux_package(None),
            SnowLumaLinuxPackage::Full
        );
        let installed_without_node = RemoteSelectedPaths {
            home: "/home/u".into(),
            snowluma_dir: Some("/opt/snowluma".into()),
            node_bin: None,
            ..RemoteSelectedPaths::default()
        };
        assert_eq!(
            infer_snowluma_linux_package(Some(&installed_without_node)),
            SnowLumaLinuxPackage::Full
        );
        let full_with_system_node = RemoteSelectedPaths {
            home: "/home/u".into(),
            snowluma_dir: Some("/opt/snowluma".into()),
            node_bin: Some("/usr/bin/node".into()),
            ..RemoteSelectedPaths::default()
        };
        assert_eq!(
            infer_snowluma_linux_package(Some(&full_with_system_node)),
            SnowLumaLinuxPackage::Full
        );
        assert!(is_bundled_snowluma_node(
            "/opt/snowluma/node",
            Some("/opt/snowluma")
        ));
        assert!(is_bundled_snowluma_node(
            "/opt/snowluma/node/bin/node",
            Some("/opt/snowluma")
        ));
        assert!(!is_bundled_snowluma_node(
            "/home/u/snowluma-remote/workspace/node/bin/node",
            Some("/home/u/snowluma-remote/workspace/snowluma")
        ));
    }

    #[test]
    fn workspace_parent_when_dir_ends_with_snowluma() {
        assert_eq!(
            snowluma_workspace_from_dir("/home/u/snowluma-remote/workspace/snowluma"),
            "/home/u/snowluma-remote/workspace"
        );
        assert_eq!(
            snowluma_workspace_from_dir("/home/u/Napcat/snowluma-workspace/snowluma"),
            "/home/u/Napcat/snowluma-workspace"
        );
        assert_eq!(snowluma_workspace_from_dir("/data/sl"), "/data/sl");
        assert_eq!(
            snowluma_workspace_from_dir("/opt/snowluma"),
            "/opt/snowluma",
            "/opt/snowluma must not use /opt as workspace"
        );
    }

    #[test]
    fn require_snowluma_includes_home() {
        let selected = RemoteSelectedPaths {
            home: "/root".into(),
            qq_install_base: Some("/".into()),
            qq_bin: Some("/opt/QQ/qq".into()),
            ..RemoteSelectedPaths::default()
        };
        let err = require_snowluma_dir(&selected).unwrap_err();
        assert!(err.contains("home=/root"));
        assert!(err.contains("SnowLuma"));
    }

    #[test]
    fn candidates_include_opt_and_home() {
        let c = snowluma_install_candidates("/root");
        assert_eq!(c[0], "/root/snowluma-remote/workspace/snowluma");
        assert_eq!(c[1], "/root/Napcat/snowluma-workspace/snowluma");
        assert_eq!(c[2], "/opt/snowluma");
        let q = qq_bin_candidates("/root");
        assert_eq!(q[0], "/root/Napcat/opt/QQ/qq");
        assert_eq!(q[1], "/opt/QQ/qq");
    }
}
