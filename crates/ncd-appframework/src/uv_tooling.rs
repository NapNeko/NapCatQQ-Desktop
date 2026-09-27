//! Python 生态应用端共用的工具解析：找 uv，记下实例安装时用的 uv；装依赖前看一眼 Linux 主机
//! （C 库版本、剩余空间），实例要的解释器不在时先装一个 uv 托管的。
//!
//! 与 `node_tooling` 同构：候选顺序 = 调用方指定（桌面端管理 / 用户覆盖）→ 上次安装记录 → PATH。
//! 虚拟环境不在这里管，由各实例目录内的 `uv sync` 按 `pyproject.toml` 自理。

use std::time::Duration;

use ncd_component::{ActionCtx, ActionError, DownloadHelper, UvComponent};
use ncd_host::{Host, HostCommand, HostPath, Locality, Os};

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

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

/// Linux 主机的 C 库。PyPI 上的 Linux 轮子按 glibc 版本分档（manylinux_2_28 就要 2.28 以上），
/// musl 的轮子和 glibc 不通用；没有合适的轮子 uv 会退回源码编译，在服务器上基本编不过
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxLibc {
    Glibc { major: u32, minor: u32 },
    Musl,
    Unknown,
}

impl LinuxLibc {
    /// 认不出的当作够用：探测失败不该挡住安装，真不够时 uv 自己会报
    pub fn glibc_at_least(self, major: u32, minor: u32) -> bool {
        match self {
            Self::Glibc { major: a, minor: b } => (a, b) >= (major, minor),
            Self::Musl => false,
            Self::Unknown => true,
        }
    }
}

/// `getconf GNU_LIBC_VERSION`（形如 `glibc 2.35`）优先；拿不到再看 `ldd --version` 的第一行
/// （glibc 形如 `ldd (Ubuntu GLIBC 2.35-0ubuntu3.8) 2.35`，musl 形如 `musl libc (x86_64)`）
pub fn parse_linux_libc(getconf: &str, ldd: &str) -> LinuxLibc {
    let version = |s: &str| -> Option<(u32, u32)> {
        let mut it = s.trim().split('.');
        let major = it.next()?.trim().parse().ok()?;
        let minor: String = it.next()?.chars().take_while(char::is_ascii_digit).collect();
        Some((major, minor.parse().ok()?))
    };
    if let Some(rest) = getconf.trim().strip_prefix("glibc")
        && let Some((major, minor)) = version(rest)
    {
        return LinuxLibc::Glibc { major, minor };
    }
    let first = ldd.lines().next().unwrap_or("").to_ascii_lowercase();
    if first.contains("musl") {
        return LinuxLibc::Musl;
    }
    if (first.contains("glibc") || first.contains("gnu libc"))
        && let Some((major, minor)) = first.split_whitespace().last().and_then(version)
    {
        return LinuxLibc::Glibc { major, minor };
    }
    LinuxLibc::Unknown
}

pub async fn probe_linux_libc(host: &dyn Host) -> LinuxLibc {
    let cmd = HostCommand::new("sh")
        .arg("-c")
        .arg("getconf GNU_LIBC_VERSION 2>/dev/null; echo '--ncd--'; ldd --version 2>&1 | head -n 1")
        .timeout(Duration::from_secs(15));
    let Ok(out) = host.run_to_string(cmd).await else {
        return LinuxLibc::Unknown;
    };
    let (getconf, ldd) = out.stdout.split_once("--ncd--").unwrap_or((out.stdout.as_str(), ""));
    parse_linux_libc(getconf, ldd.trim_start())
}

/// 目标目录所在分区剩多少（KB）。目录还没建就往上找第一个已存在的
pub async fn probe_free_kb(host: &dyn Host, dir: &HostPath) -> Option<u64> {
    let script = format!(
        "d={}; while [ ! -d \"$d\" ] && [ \"$d\" != / ]; do d=$(dirname \"$d\"); done; df -Pk \"$d\" | tail -n 1",
        shell_quote(dir.as_posix())
    );
    let cmd = HostCommand::new("sh").arg("-c").arg(script).timeout(Duration::from_secs(15));
    let out = host.run_to_string(cmd).await.ok()?;
    parse_df_available_kb(&out.stdout)
}

/// `df -Pk` 数据行：文件系统 / 总量 / 已用 / 可用 / 百分比 / 挂载点
pub fn parse_df_available_kb(line: &str) -> Option<u64> {
    line.lines().last()?.split_whitespace().nth(3)?.parse().ok()
}

/// 主机自己下解释器的时限。uv 默认从 `releases.astral.sh` 下，二十来 MB，连不上时它自己几十秒就报错
const PYTHON_DIRECT_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const PYTHON_BUILDS_GITHUB: &str = "https://github.com/astral-sh/python-build-standalone/releases/download/";
const PYTHON_BUILDS_ASTRAL: &str = "https://releases.astral.sh/github/python-build-standalone/releases/download/";

/// 实例要的解释器没有就装一个 uv 托管的，免得 `uv sync` 在装依赖中途才去下、失败了只剩一句网络错误。
/// 先让主机自己下；远端下不动时由桌面端按 uv 给的地址镜像竞速下好传上去，
/// 用 `UV_PYTHON_INSTALL_MIRROR=file://` 装（上游支持本地目录当镜像）。`scratch` 用完就删。
/// uv 0.8 起显式安装默认往 `~/.local/bin` 放 `python3.x`、在 Windows 上还写注册表，
/// 都不该替用户做：两个开关关掉（老版本不认这两个变量，本来也不做这两件事）
pub async fn ensure_python(
    host: &dyn Host,
    uv: &HostPath,
    request: &str,
    scratch: &HostPath,
    ctx: &ActionCtx,
    step: u32,
) -> Result<(), ActionError> {
    let uv_cmd = |args: &[&str]| {
        args.iter()
            .fold(HostCommand::new(uv.as_posix()), |cmd, a| cmd.arg(*a))
            .env("UV_NO_PROGRESS", "1")
            .env("UV_PYTHON_INSTALL_BIN", "0")
            .env("UV_PYTHON_INSTALL_REGISTRY", "0")
            .cancel_token(ctx.cancel_token())
    };
    let found = host
        .run_to_string(uv_cmd(&["python", "find", request]).timeout(Duration::from_secs(30)))
        .await;
    if found.is_ok_and(|out| out.success()) {
        return Ok(());
    }
    ctx.info(format!("主机上没有 Python {request}，先装一个 uv 托管的")).await;
    let direct = host
        .run_to_string(uv_cmd(&["python", "install", request]).timeout(PYTHON_DIRECT_TIMEOUT))
        .await;
    let direct_err = match direct {
        Ok(out) if out.success() => return Ok(()),
        Ok(out) => out.stderr.trim().lines().last().unwrap_or_default().to_string(),
        Err(e) => e.to_string(),
    };
    if host.locality() != Locality::Remote {
        return Err(python_install_failed(request, &direct_err));
    }
    ctx.warn(format!("主机自己下载解释器失败（{direct_err}），改由桌面端下载后传上去")).await;

    let listed = host
        .run_to_string(
            uv_cmd(&["python", "list", request, "--only-downloads", "--output-format", "json"])
                .timeout(Duration::from_secs(30)),
        )
        .await?;
    let url = pick_python_download(&listed.stdout)
        .ok_or_else(|| python_install_failed(request, "uv 没给出可下载的解释器"))?;
    let (tag, file) = python_build_rel(&url)
        .ok_or_else(|| python_install_failed(request, &format!("认不出下载地址 {url}")))?;
    let local_tmp = std::env::temp_dir().join(format!("ncd-python-{}-{file}", std::process::id()));
    let downloaded = DownloadHelper::new()?
        .download_with_mirrors_no_chunk(&python_download_mirrors(&url), &local_tmp, None, ctx, step)
        .await;
    if let Err(e) = downloaded {
        let _ = tokio::fs::remove_file(&local_tmp).await;
        return Err(e);
    }
    let dest = scratch.join(&tag).join(&file);
    let uploaded = host.upload(&local_tmp, &dest).await;
    let _ = tokio::fs::remove_file(&local_tmp).await;
    uploaded?;
    let installed = host
        .run_to_string(
            uv_cmd(&["python", "install", request])
                .env("UV_PYTHON_INSTALL_MIRROR", format!("file://{}", scratch.as_posix()))
                .timeout(PYTHON_DIRECT_TIMEOUT),
        )
        .await;
    let _ = host.remove_dir_all(scratch).await;
    let out = installed?;
    if out.success() {
        return Ok(());
    }
    Err(python_install_failed(request, out.stderr.trim().lines().last().unwrap_or_default()))
}

fn python_install_failed(request: &str, detail: &str) -> ActionError {
    ActionError::install_step(
        "python",
        format!("装不上 Python {request}：{detail}。可以先在主机上自己装好 Python {request} 再重试"),
    )
}

/// `uv python list --only-downloads --output-format json` 里第一个标准 CPython（不要自由线程 / 调试版）
pub fn pick_python_download(json: &str) -> Option<String> {
    let entries: Vec<serde_json::Value> = serde_json::from_str(json.trim()).ok()?;
    entries.iter().find_map(|e| {
        let is_cpython = e.get("implementation").and_then(|v| v.as_str()) == Some("cpython");
        let plain = e.get("variant").and_then(|v| v.as_str()).is_none_or(|v| v == "default");
        let url = e.get("url").and_then(|v| v.as_str())?;
        (is_cpython && plain && url.starts_with("http")).then(|| url.to_string())
    })
}

/// 下载地址 → （发布 tag，盘上文件名）。uv 拿镜像根拼出 `<tag>/<文件名>`，读本地镜像时把 `%2B` 还原成 `+`
pub fn python_build_rel(url: &str) -> Option<(String, String)> {
    let rest = url
        .strip_prefix(PYTHON_BUILDS_ASTRAL)
        .or_else(|| url.strip_prefix(PYTHON_BUILDS_GITHUB))?;
    let (tag, file) = rest.split_once('/')?;
    if tag.is_empty() || file.is_empty() || file.contains('/') {
        return None;
    }
    Some((tag.to_string(), file.replace("%2B", "+").replace("%2b", "+")))
}

/// 原地址先上，再补 GitHub 原站和它的镜像
fn python_download_mirrors(url: &str) -> Vec<String> {
    let mut out = vec![url.to_string()];
    if let Some(rest) = url.strip_prefix(PYTHON_BUILDS_ASTRAL) {
        for m in ncd_network::build_mirror_urls(&format!("{PYTHON_BUILDS_GITHUB}{rest}"), None) {
            if !out.contains(&m) {
                out.push(m);
            }
        }
    }
    out
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

    #[test]
    fn libc_is_read_from_getconf_then_ldd() {
        assert_eq!(parse_linux_libc("glibc 2.35\n", ""), LinuxLibc::Glibc { major: 2, minor: 35 });
        assert_eq!(
            parse_linux_libc("", "ldd (Ubuntu GLIBC 2.35-0ubuntu3.8) 2.35\n"),
            LinuxLibc::Glibc { major: 2, minor: 35 }
        );
        assert_eq!(
            parse_linux_libc("", "ldd (GNU libc) 2.17\nCopyright (C) 2012"),
            LinuxLibc::Glibc { major: 2, minor: 17 }
        );
        assert_eq!(parse_linux_libc("", "musl libc (x86_64)\nVersion 1.2.4"), LinuxLibc::Musl);
        assert_eq!(parse_linux_libc("", "sh: ldd: not found"), LinuxLibc::Unknown);

        assert!(!LinuxLibc::Glibc { major: 2, minor: 27 }.glibc_at_least(2, 28), "Ubuntu 18.04");
        assert!(LinuxLibc::Glibc { major: 2, minor: 28 }.glibc_at_least(2, 28), "Debian 10");
        assert!(LinuxLibc::Glibc { major: 2, minor: 36 }.glibc_at_least(2, 28), "Debian 12");
        assert!(!LinuxLibc::Musl.glibc_at_least(2, 28));
        assert!(LinuxLibc::Unknown.glibc_at_least(2, 28), "认不出不拦");
    }

    #[test]
    fn df_available_column() {
        let out = "Filesystem     1024-blocks     Used Available Capacity Mounted on\n/dev/vda1         41152812 10485760  28571428      27% /\n";
        assert_eq!(parse_df_available_kb(out), Some(28_571_428));
        assert_eq!(parse_df_available_kb("/dev/vda1 41152812 10485760 28571428 27% /"), Some(28_571_428));
        assert_eq!(parse_df_available_kb(""), None);
    }

    #[test]
    fn python_download_is_picked_and_mapped_to_a_local_mirror_layout() {
        let json = r#"[
            {"key":"cpython-3.12.13+freethreaded-linux-x86_64-gnu","implementation":"cpython","variant":"freethreaded","url":"https://x/ft.tar.gz"},
            {"key":"cpython-3.12.13-linux-x86_64-gnu","implementation":"cpython","variant":"default","url":"https://releases.astral.sh/github/python-build-standalone/releases/download/20260623/cpython-3.12.13%2B20260623-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz"},
            {"key":"graalpy-3.12.0-linux-x86_64-gnu","implementation":"graalpy","variant":"default","url":"https://g/graalpy.tar.gz"}
        ]"#;
        let url = pick_python_download(json).unwrap();
        assert!(url.contains("x86_64-unknown-linux-gnu"), "{url}");
        assert_eq!(pick_python_download("not json"), None);

        let (tag, file) = python_build_rel(&url).unwrap();
        assert_eq!(tag, "20260623");
        assert_eq!(file, "cpython-3.12.13+20260623-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz");
        assert_eq!(
            python_build_rel("https://github.com/astral-sh/python-build-standalone/releases/download/t/a%2Bb.tar.gz"),
            Some(("t".into(), "a+b.tar.gz".into()))
        );
        assert_eq!(python_build_rel("https://example.com/a.tar.gz"), None);

        let mirrors = python_download_mirrors(&url);
        assert_eq!(mirrors[0], url, "原地址先试");
        assert!(
            mirrors.iter().any(|m| m == &format!("{PYTHON_BUILDS_GITHUB}20260623/cpython-3.12.13%2B20260623-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz")),
            "GitHub 原站兜底：{mirrors:?}"
        );
    }
}
