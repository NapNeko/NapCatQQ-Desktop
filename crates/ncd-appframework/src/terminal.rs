//! 终端开在实例目录时额外接上的环境
//!
//! 只加不减：PATH 前缀由编排层拼到主机原 PATH 前面（本机是当前进程的 PATH，远端在登录 shell
//! 读完 profile 之后再 export），用户自己装的工具照样找得到。
//! 工具链的找法和启动时一致（桌面端管理的 → 实例里的安装记录 → PATH），终端里敲的 node / uv
//! 和实例跑起来用的是同一个。

use ncd_domain::TerminalSnippet;
use ncd_host::{Host, HostPath, Os};

use crate::adapter::AppComponentSpec;
use crate::node_tooling::{TOOLS_DIR, read_node_marker, resolve_node_toolchain};
use crate::uv_tooling::read_uv_marker;

/// 实例终端的环境
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppTerminalProfile {
    /// 排在主机原 PATH 前面的目录，靠前的优先
    pub path_prefix: Vec<HostPath>,
    /// 追加或覆盖的环境变量；路径值已按主机风格写好
    pub env: Vec<(String, String)>,
    /// 开头打一行，说清环境里接好了什么
    pub hint: Option<String>,
    /// 框架预置的常用命令
    pub snippets: Vec<TerminalSnippet>,
}

fn venv_dir(install_dir: &HostPath) -> HostPath {
    install_dir.join(".venv")
}

fn venv_bin(install_dir: &HostPath, os: Os) -> HostPath {
    match os {
        Os::Windows => venv_dir(install_dir).join("Scripts"),
        _ => venv_dir(install_dir).join("bin"),
    }
}

/// 可执行文件所在目录；`uv` 这种裸命令名没有目录，返回 None
fn tool_dir(bin: &HostPath) -> Option<HostPath> {
    bin.parent()
        .filter(|d| !d.as_posix().is_empty() && d.as_posix() != ".")
}

/// uv 系框架（NoneBot2 / AstrBot / MaiBot）：实例 `.venv` 在最前，其次是装实例时用的 uv
pub async fn uv_venv_profile(
    host: &dyn Host,
    spec: &AppComponentSpec,
    snippets: Vec<TerminalSnippet>,
) -> AppTerminalProfile {
    let os = host.os();
    let bin = venv_bin(&spec.install_dir, os);
    let has_venv = host.exists(&bin).await.unwrap_or(false);

    let mut path_prefix = Vec::new();
    if has_venv {
        path_prefix.push(bin);
    }
    let mut uv_candidates = Vec::with_capacity(2);
    if let Some(p) = &spec.uv_bin {
        uv_candidates.push(p.clone());
    }
    if let Some(p) = read_uv_marker(host, &spec.install_dir).await {
        uv_candidates.push(p);
    }
    if let Some(dir) = uv_candidates.iter().find_map(tool_dir) {
        path_prefix.push(dir);
    }

    // 和实例启动时一样用 UTF-8，终端里跑 python 打中文不乱码
    let mut env = vec![
        ("PYTHONUTF8".to_string(), "1".to_string()),
        ("PYTHONIOENCODING".to_string(), "utf-8".to_string()),
    ];
    if has_venv {
        env.push((
            "VIRTUAL_ENV".to_string(),
            venv_dir(&spec.install_dir).render_for(os),
        ));
    }
    let hint = if has_venv {
        "python 和 uv 已指向这个实例的 .venv；这个环境里没有 pip，装包用 uv pip install"
    } else {
        "实例还没装好依赖（找不到 .venv），先在应用端页把它装好"
    };
    AppTerminalProfile {
        path_prefix,
        env,
        hint: Some(hint.to_string()),
        snippets,
    }
}

/// Node 系框架（Karin）：项目自己的 `node_modules/.bin`、实例私有 pnpm、装实例时用的 node
pub async fn node_profile(
    host: &dyn Host,
    spec: &AppComponentSpec,
    snippets: Vec<TerminalSnippet>,
) -> AppTerminalProfile {
    let mut path_prefix = vec![
        spec.install_dir.join("node_modules/.bin"),
        spec.install_dir
            .join(format!("{TOOLS_DIR}/node_modules/.bin")),
    ];
    let mut preferred = Vec::with_capacity(2);
    if let Some(p) = &spec.node_bin {
        preferred.push(p.clone());
    }
    if let Some(p) = read_node_marker(host, &spec.install_dir).await {
        preferred.push(p);
    }
    let hint = match resolve_node_toolchain(host, &preferred).await {
        Ok(toolchain) => {
            if let Some(dir) = toolchain.node_dir() {
                path_prefix.push(dir);
            }
            "node、pnpm（实例私有的那份）和 karin 命令都能直接用"
        }
        Err(_) => "没找到实例用的 Node.js，pnpm 和 karin 命令可能用不了",
    };
    AppTerminalProfile {
        path_prefix,
        env: Vec::new(),
        hint: Some(hint.to_string()),
        snippets,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn venv_bin_follows_os_layout() {
        let dir = HostPath::from_posix("/home/u/ncd/apps/maibot/m1");
        assert_eq!(
            venv_bin(&dir, Os::Linux).as_posix(),
            "/home/u/ncd/apps/maibot/m1/.venv/bin"
        );
        let win = HostPath::from_windows(r"C:\data\apps\maibot\m1");
        assert_eq!(
            venv_bin(&win, Os::Windows).render_for(Os::Windows),
            r"C:\data\apps\maibot\m1\.venv\Scripts"
        );
    }

    #[test]
    fn bare_command_name_has_no_tool_dir() {
        assert_eq!(tool_dir(&HostPath::from_posix("uv")), None);
        assert_eq!(
            tool_dir(&HostPath::from_posix("/home/u/ncd/tools/uv/uv")),
            Some(HostPath::from_posix("/home/u/ncd/tools/uv"))
        );
    }

    #[cfg(windows)]
    fn spec_in(dir: &std::path::Path) -> AppComponentSpec {
        AppComponentSpec {
            install_dir: HostPath::from_windows(&dir.to_string_lossy()),
            port: 8080,
            node_bin: None,
            uv_bin: None,
            npm_registry: None,
            install_renderer: false,
            adopt_existing: false,
            instance_id: "t1".into(),
            webui_username: None,
            webui_password: None,
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn uv_profile_puts_venv_first_then_recorded_uv() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".venv").join("Scripts")).unwrap();
        std::fs::write(dir.path().join(".ncd-uv"), "/c/tools/uv/uv.exe\n").unwrap();
        let host = ncd_host::local::LocalWindowsHost::new();
        let profile = uv_venv_profile(&host, &spec_in(dir.path()), Vec::new()).await;

        let rendered: Vec<String> = profile
            .path_prefix
            .iter()
            .map(|p| p.render_for(Os::Windows))
            .collect();
        let venv_scripts = dir.path().join(".venv").join("Scripts");
        assert_eq!(
            rendered[0].to_lowercase(),
            venv_scripts.to_string_lossy().to_lowercase()
        );
        assert_eq!(rendered[1], r"C:\tools\uv");
        let venv = profile
            .env
            .iter()
            .find(|(k, _)| k == "VIRTUAL_ENV")
            .map(|(_, v)| v.to_lowercase());
        assert_eq!(
            venv,
            Some(dir.path().join(".venv").to_string_lossy().to_lowercase())
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn uv_profile_without_venv_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let host = ncd_host::local::LocalWindowsHost::new();
        let profile = uv_venv_profile(&host, &spec_in(dir.path()), Vec::new()).await;
        assert!(profile.path_prefix.is_empty());
        assert!(profile.env.iter().all(|(k, _)| k != "VIRTUAL_ENV"));
        assert!(profile.hint.unwrap_or_default().contains(".venv"));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn node_profile_starts_with_project_bins() {
        let dir = tempfile::tempdir().unwrap();
        let host = ncd_host::local::LocalWindowsHost::new();
        let profile = node_profile(&host, &spec_in(dir.path()), Vec::new()).await;
        let tails: Vec<String> = profile
            .path_prefix
            .iter()
            .take(2)
            .map(|p| p.as_posix().to_string())
            .collect();
        assert!(tails[0].ends_with("/node_modules/.bin"));
        assert!(tails[1].ends_with("/.ncd-tools/node_modules/.bin"));
    }
}
