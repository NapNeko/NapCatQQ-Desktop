//! Node 生态应用端共用的工具解析：找 node、找随 node 附带的 npm-cli.js、
//! 在实例目录内放一份项目私有 pnpm（不污染全局，不要求主机装 pnpm）。
//!
//! 全部通过 `node <script.js>` 调用，不依赖 `npm.cmd` / `pnpm` shim 是否在 PATH。
//!
//! 路径进命令行参数一律 `render_for(os)`：Host 只还原 program，参数原样透传，
//! Windows 上 `/e/x.js` 会被 node 解析成 `C:\e\x.js`。

use ncd_component::ActionError;
use ncd_host::{Host, HostCommand, HostPath, Os};

/// 实例目录下私有工具目录名（npm --prefix 落点）
pub const TOOLS_DIR: &str = ".ncd-tools";

/// 已解析的 Node 工具链
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeToolchain {
    /// node 可执行文件（HostPath；PATH 上的 node 也会被解析成绝对路径）
    pub node_bin: HostPath,
    /// node 安装目录下随附的 npm-cli.js
    pub npm_cli: HostPath,
}

impl NodeToolchain {
    /// node 所在目录（Windows: node.exe 同级；Linux: bin/）
    pub fn node_dir(&self) -> Option<HostPath> {
        self.node_bin.parent()
    }

    /// `node <npm-cli.js> args...`
    pub fn npm(&self, os: Os, args: &[&str]) -> HostCommand {
        HostCommand::new(self.node_bin.as_posix())
            .arg(self.npm_cli.render_for(os))
            .args(args.iter().copied())
    }
}

/// 实例目录下记录「安装时用的 node」的标记文件；起停 / 探测时优先复用，
/// 这样远端非交互 SSH 会话的 PATH 里没有 node 也能起来。
pub const NODE_MARKER_FILE: &str = ".ncd-node";

/// 解析 node：按 `preferred` 顺序尝试（桌面端管理的 Node、上次安装记录…），最后看 PATH。
/// 通过 `node -p process.execPath` 拿到真实绝对路径，从而定位随附 npm。
pub async fn resolve_node_toolchain(
    host: &dyn Host,
    preferred: &[HostPath],
) -> Result<NodeToolchain, ActionError> {
    let mut candidates: Vec<String> = Vec::new();
    for p in preferred {
        let s = p.as_posix().to_string();
        if !s.is_empty() && !candidates.contains(&s) {
            candidates.push(s);
        }
    }
    candidates.push("node".to_string());

    let mut last_err: Option<String> = None;
    for cand in candidates {
        let cmd = HostCommand::new(&cand).arg("-p").arg("process.execPath");
        match host.run_to_string(cmd).await {
            Ok(out) if out.success() => {
                let exec_path = out.stdout.trim();
                if exec_path.is_empty() {
                    last_err = Some(format!("{cand}: process.execPath 为空"));
                    continue;
                }
                let node_bin = match host.os() {
                    Os::Windows => HostPath::from_windows(exec_path),
                    _ => HostPath::from_posix(exec_path),
                };
                let npm_cli = npm_cli_for(&node_bin, host.os());
                if host.exists(&npm_cli).await? {
                    return Ok(NodeToolchain { node_bin, npm_cli });
                }
                last_err = Some(format!(
                    "{}: 未找到随附 npm（{}）",
                    node_bin.as_posix(),
                    npm_cli.as_posix()
                ));
            }
            Ok(out) => {
                last_err = Some(format!("{cand}: exit={:?} {}", out.exit_code, out.stderr.trim()));
            }
            Err(e) => {
                last_err = Some(format!("{cand}: {e}"));
            }
        }
    }
    Err(ActionError::install_step(
        "resolve-node",
        format!(
            "未找到可用的 Node.js（需 ≥18，且随附 npm）：{}",
            last_err.unwrap_or_default()
        ),
    ))
}

/// `<install_dir>/.ncd-node`
pub fn node_marker_path(install_dir: &HostPath) -> HostPath {
    install_dir.join(NODE_MARKER_FILE)
}

/// 安装成功后记下本次用的 node 绝对路径（覆盖写）。
pub async fn write_node_marker(
    host: &dyn Host,
    install_dir: &HostPath,
    toolchain: &NodeToolchain,
) -> Result<(), ActionError> {
    let body = format!("{}\n", toolchain.node_bin.as_posix());
    host.write_file(&node_marker_path(install_dir), body.as_bytes())
        .await?;
    Ok(())
}

/// 读上次安装记录的 node；没有 / 读不到都当 None（回退 PATH）。
pub async fn read_node_marker(host: &dyn Host, install_dir: &HostPath) -> Option<HostPath> {
    let path = node_marker_path(install_dir);
    let bytes = host.read_file(&path).await.ok()?;
    let text = String::from_utf8(bytes.to_vec()).ok()?;
    let line = text.lines().next()?.trim();
    if line.is_empty() {
        return None;
    }
    Some(match host.os() {
        Os::Windows => HostPath::from_windows(line),
        _ => HostPath::from_posix(line),
    })
}

/// 官方发行包布局：Windows `node.exe` 同级 `node_modules/npm/bin/npm-cli.js`；
/// Linux/macOS `bin/node` 上一级 `lib/node_modules/npm/bin/npm-cli.js`
pub fn npm_cli_for(node_bin: &HostPath, os: Os) -> HostPath {
    let dir = node_bin
        .parent()
        .unwrap_or_else(|| HostPath::from_posix("."));
    match os {
        Os::Windows => dir.join("node_modules/npm/bin/npm-cli.js"),
        _ => dir
            .parent()
            .unwrap_or(dir)
            .join("lib/node_modules/npm/bin/npm-cli.js"),
    }
}

/// 项目私有 pnpm 入口：`<install_dir>/.ncd-tools/node_modules/pnpm/bin/pnpm.cjs`
pub fn local_pnpm_cjs(install_dir: &HostPath) -> HostPath {
    install_dir.join(format!("{TOOLS_DIR}/node_modules/pnpm/bin/pnpm.cjs"))
}

/// 项目私有 pnpm 的 shim 目录（放进 PATH 让应用端自身的 `pnpm add` 能用）
pub fn local_pnpm_bin_dir(install_dir: &HostPath) -> HostPath {
    install_dir.join(format!("{TOOLS_DIR}/node_modules/.bin"))
}

/// `node <pnpm.cjs> args...`，cwd = install_dir
pub fn pnpm_command(
    toolchain: &NodeToolchain,
    install_dir: &HostPath,
    os: Os,
    args: &[&str],
) -> HostCommand {
    HostCommand::new(toolchain.node_bin.as_posix())
        .arg(local_pnpm_cjs(install_dir).render_for(os))
        .args(args.iter().copied())
        .working_dir(install_dir.clone())
}

/// 给应用端进程 / 包管理命令用的 PATH：私有 pnpm shim + node 目录 + 主机原 PATH。
/// 本机直接读当前进程 PATH；远端由调用方通过 `sh -c 'export PATH=…:$PATH'` 展开，这里给前缀。
pub fn path_prefix(toolchain: &NodeToolchain, install_dir: &HostPath, os: Os) -> Vec<String> {
    let style = match os {
        Os::Windows => ncd_host::PathStyle::Windows,
        _ => ncd_host::PathStyle::Posix,
    };
    let mut parts = vec![local_pnpm_bin_dir(install_dir).render(style)];
    if let Some(dir) = toolchain.node_dir() {
        parts.push(dir.render(style));
    }
    parts
}

/// 本机场景：把前缀拼到当前进程 PATH 前面
pub fn local_path_env(prefix: &[String], os: Os) -> String {
    let sep = match os {
        Os::Windows => ";",
        _ => ":",
    };
    let current = std::env::var("PATH").unwrap_or_default();
    let mut all: Vec<&str> = prefix.iter().map(String::as_str).collect();
    if !current.is_empty() {
        all.push(&current);
    }
    all.join(sep)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npm_cli_layout_per_os() {
        let win = npm_cli_for(&HostPath::from_windows(r"C:\node\node.exe"), Os::Windows);
        assert_eq!(win.as_posix(), "/c/node/node_modules/npm/bin/npm-cli.js");
        let linux = npm_cli_for(&HostPath::from_posix("/opt/node/bin/node"), Os::Linux);
        assert_eq!(linux.as_posix(), "/opt/node/lib/node_modules/npm/bin/npm-cli.js");
    }

    #[test]
    fn pnpm_paths_live_under_instance_tools_dir() {
        let dir = HostPath::from_posix("/home/u/ncd/apps/karin/k1");
        assert_eq!(
            local_pnpm_cjs(&dir).as_posix(),
            "/home/u/ncd/apps/karin/k1/.ncd-tools/node_modules/pnpm/bin/pnpm.cjs"
        );
        assert_eq!(
            local_pnpm_bin_dir(&dir).as_posix(),
            "/home/u/ncd/apps/karin/k1/.ncd-tools/node_modules/.bin"
        );
    }

    /// 真机回归：PATH 上的 node 在 E 盘时，npm-cli.js / pnpm.cjs 作为参数必须是
    /// `E:\...`，否则 node 按当前盘解析成 `C:\e\...` 报 MODULE_NOT_FOUND
    #[test]
    fn script_arguments_are_rendered_native_on_windows() {
        let node_bin = HostPath::from_windows(r"E:\Program Files\nodejs\node.exe");
        let tc = NodeToolchain {
            npm_cli: npm_cli_for(&node_bin, Os::Windows),
            node_bin,
        };
        let npm = tc.npm(Os::Windows, &["install", "pnpm"]);
        assert_eq!(
            npm.args[0],
            r"E:\Program Files\nodejs\node_modules\npm\bin\npm-cli.js"
        );

        let dir = HostPath::from_windows(r"D:\NapCatQQ\apps\karin\k1");
        let pnpm = pnpm_command(&tc, &dir, Os::Windows, &["install"]);
        assert_eq!(
            pnpm.args[0],
            r"D:\NapCatQQ\apps\karin\k1\.ncd-tools\node_modules\pnpm\bin\pnpm.cjs"
        );

        let linux_tc = NodeToolchain {
            node_bin: HostPath::from_posix("/opt/node/bin/node"),
            npm_cli: HostPath::from_posix("/opt/node/lib/node_modules/npm/bin/npm-cli.js"),
        };
        let linux_dir = HostPath::from_posix("/home/u/ncd/apps/karin/k1");
        assert_eq!(
            pnpm_command(&linux_tc, &linux_dir, Os::Linux, &["install"]).args[0],
            "/home/u/ncd/apps/karin/k1/.ncd-tools/node_modules/pnpm/bin/pnpm.cjs"
        );
    }

    #[test]
    fn path_prefix_renders_native_style() {
        let tc = NodeToolchain {
            node_bin: HostPath::from_windows(r"C:\node\node.exe"),
            npm_cli: HostPath::from_windows(r"C:\node\node_modules\npm\bin\npm-cli.js"),
        };
        let dir = HostPath::from_windows(r"C:\apps\k1");
        let prefix = path_prefix(&tc, &dir, Os::Windows);
        assert_eq!(
            prefix,
            vec![
                r"C:\apps\k1\.ncd-tools\node_modules\.bin".to_string(),
                r"C:\node".to_string()
            ]
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn node_marker_round_trips_and_is_none_when_missing() {
        use ncd_host::local::LocalWindowsHost;
        let host = LocalWindowsHost::new();
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = HostPath::from_windows(&tmp.path().join("k1").to_string_lossy());
        host.create_dir_all(&dir).await.expect("mkdir");

        assert!(read_node_marker(&host, &dir).await.is_none());

        let tc = NodeToolchain {
            node_bin: HostPath::from_windows(r"C:\node\node.exe"),
            npm_cli: HostPath::from_windows(r"C:\node\node_modules\npm\bin\npm-cli.js"),
        };
        write_node_marker(&host, &dir, &tc).await.expect("write marker");
        let read = read_node_marker(&host, &dir).await.expect("marker present");
        assert_eq!(read, tc.node_bin);
    }
}
