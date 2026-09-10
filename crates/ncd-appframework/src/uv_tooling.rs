//! Python 生态应用端共用的工具解析：找 uv，记下实例安装时用的 uv。
//!
//! 与 `node_tooling` 同构：候选顺序 = 调用方指定（桌面端管理 / 用户覆盖）→ 上次安装记录 → PATH。
//! 解释器与虚拟环境不在这里管，由各实例目录内的 `uv sync` 按 `pyproject.toml` 自理。

use ncd_component::{ActionError, UvComponent};
use ncd_host::{Host, HostCommand, HostPath, Os};

/// 实例目录下记录「安装时用的 uv」的标记文件
pub const UV_MARKER_FILE: &str = ".ncd-uv";

/// 已解析的 uv
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UvToolchain {
    /// uv 可执行文件（PATH 上的也解析成传入的字面量，uv 没有 `process.execPath` 等价物）
    pub uv_bin: HostPath,
    pub version: String,
}

/// 按 `preferred` 顺序尝试，最后看 PATH 的 `uv`；以 `uv --version` 能跑为准。
pub async fn resolve_uv(host: &dyn Host, preferred: &[HostPath]) -> Result<UvToolchain, ActionError> {
    let mut candidates: Vec<String> = Vec::new();
    for p in preferred {
        let s = p.as_posix().to_string();
        if !s.is_empty() && !candidates.contains(&s) {
            candidates.push(s);
        }
    }
    candidates.push("uv".to_string());

    let mut last_err: Option<String> = None;
    for cand in candidates {
        let cmd = HostCommand::new(&cand).arg("--version");
        match host.run_to_string(cmd).await {
            Ok(out) if out.success() => match UvComponent::parse_version_output(&out.stdout) {
                Some(version) => {
                    return Ok(UvToolchain {
                        uv_bin: HostPath::from_posix(&cand),
                        version,
                    });
                }
                None => {
                    last_err = Some(format!("{cand}: 无法识别版本输出 {:?}", out.stdout.trim()));
                }
            },
            Ok(out) => {
                last_err = Some(format!("{cand}: exit={:?} {}", out.exit_code, out.stderr.trim()));
            }
            Err(e) => {
                last_err = Some(format!("{cand}: {e}"));
            }
        }
    }
    Err(ActionError::install_step(
        "resolve-uv",
        format!("未找到可用的 uv：{}", last_err.unwrap_or_default()),
    ))
}

pub fn uv_marker_path(install_dir: &HostPath) -> HostPath {
    install_dir.join(UV_MARKER_FILE)
}

pub async fn write_uv_marker(
    host: &dyn Host,
    install_dir: &HostPath,
    toolchain: &UvToolchain,
) -> Result<(), ActionError> {
    let body = format!("{}\n", toolchain.uv_bin.as_posix());
    host.write_file(&uv_marker_path(install_dir), body.as_bytes())
        .await?;
    Ok(())
}

/// 读上次安装记录的 uv；没有 / 读不到都当 None（回退 PATH）。
pub async fn read_uv_marker(host: &dyn Host, install_dir: &HostPath) -> Option<HostPath> {
    let bytes = host.read_file(&uv_marker_path(install_dir)).await.ok()?;
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

/// 实例虚拟环境里的解释器：Windows `.venv/Scripts/python.exe`，其它 `.venv/bin/python`
pub fn venv_python(install_dir: &HostPath, os: Os) -> HostPath {
    match os {
        Os::Windows => install_dir.join(".venv/Scripts/python.exe"),
        _ => install_dir.join(".venv/bin/python"),
    }
}

/// 实例 venv 里的脚本：Windows `.venv/Scripts/<name>.exe`，其它 `.venv/bin/<name>`
pub fn venv_script(install_dir: &HostPath, os: Os, name: &str) -> HostPath {
    match os {
        Os::Windows => install_dir.join(format!(".venv/Scripts/{name}.exe")),
        _ => install_dir.join(format!(".venv/bin/{name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn venv_python_layout_per_os() {
        let dir = HostPath::from_posix("/home/u/ncd/apps/nonebot2/n1");
        assert_eq!(
            venv_python(&dir, Os::Linux).as_posix(),
            "/home/u/ncd/apps/nonebot2/n1/.venv/bin/python"
        );
        let win = HostPath::from_windows(r"C:\apps\n1");
        assert_eq!(
            venv_python(&win, Os::Windows).render(ncd_host::PathStyle::Windows),
            r"C:\apps\n1\.venv\Scripts\python.exe"
        );
        assert_eq!(
            venv_script(&dir, Os::Linux, "astrbot").as_posix(),
            "/home/u/ncd/apps/nonebot2/n1/.venv/bin/astrbot"
        );
        assert_eq!(
            venv_script(&win, Os::Windows, "astrbot").render(ncd_host::PathStyle::Windows),
            r"C:\apps\n1\.venv\Scripts\astrbot.exe"
        );
    }

    #[test]
    fn marker_path_is_hidden_file_in_instance_dir() {
        let dir = HostPath::from_posix("/x/n1");
        assert_eq!(uv_marker_path(&dir).as_posix(), "/x/n1/.ncd-uv");
    }
}
