//! 有界探测本机二进制与已知 npm 包入口。

use std::{
    collections::BTreeMap,
    env,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use ncd_domain::assistant::AssistantProvider;
use serde::Deserialize;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    time,
};

static VERSION_PROBES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);
const VERSION_STDOUT_LIMIT: usize = 3072;
const VERSION_STDERR_LIMIT: usize = 1024;

#[cfg(test)]
mod tests;

pub(super) struct Executable {
    pub program: PathBuf,
    pub prefix_args: Vec<String>,
    pub display_path: PathBuf,
}

pub(super) async fn resolve(
    provider: AssistantProvider,
    configured: Option<&str>,
) -> Result<Executable, String> {
    if let Some(configured) = configured.filter(|path| !path.trim().is_empty()) {
        let path = Path::new(configured);
        if !path.is_absolute() {
            return Err("Agent 可执行文件必须使用绝对路径，不能填写 shell 命令".into());
        }
        return resolve_file(provider, path).await;
    }
    if provider == AssistantProvider::Custom {
        return Err("请选择自定义 ACP Agent 的绝对可执行路径".into());
    }
    let directories = search_directories();
    let mut wrapper_error = None;
    for directory in &directories {
        for name in command_names(provider) {
            for suffix in executable_suffixes() {
                let candidate = directory.join(format!("{name}{suffix}"));
                if !tokio::fs::metadata(&candidate)
                    .await
                    .is_ok_and(|metadata| metadata.is_file())
                {
                    continue;
                }
                match resolve_file(provider, &candidate).await {
                    Ok(executable) => return Ok(executable),
                    Err(error) => wrapper_error = Some(error),
                }
            }
        }
        if let Ok(Some(executable)) = npm_entry(provider, directory).await {
            return Ok(executable);
        }
    }
    Err(wrapper_error
        .unwrap_or_else(|| "未在 PATH 或常用安装目录找到此 Agent；可在设置中指定可执行路径".into()))
}

async fn resolve_file(provider: AssistantProvider, path: &Path) -> Result<Executable, String> {
    if !tokio::fs::metadata(path)
        .await
        .is_ok_and(|metadata| metadata.is_file())
    {
        return Err("Agent 可执行文件不存在或不可访问".into());
    }
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(
        extension.as_str(),
        "cmd" | "bat" | "ps1" | "js" | "mjs" | "cjs"
    ) {
        if provider != AssistantProvider::Custom {
            // 状态页返回 npm 的真实入口；再次保存该路径时仍要验证所属包和 bin 映射。
            for directory in path.ancestors().skip(1).take(7) {
                if let Some(executable) = npm_entry(provider, directory).await? {
                    if matches!(extension.as_str(), "js" | "mjs" | "cjs") {
                        let canonical = tokio::fs::canonicalize(path)
                            .await
                            .map_err(|_| "无法解析 Agent 脚本路径".to_string())?;
                        if canonical != executable.display_path {
                            continue;
                        }
                    }
                    return Ok(executable);
                }
            }
        }
        return Err(
            "不能直接执行此脚本包装器；请选择原生 Agent 可执行文件，或使用受支持的官方 npm 安装"
                .into(),
        );
    }
    if cfg!(windows) && extension != "exe" {
        return Err("Windows Agent 必须是原生 .exe 或受支持的 npm 包入口".into());
    }
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(
        stem.as_str(),
        "cmd"
            | "powershell"
            | "pwsh"
            | "bash"
            | "sh"
            | "node"
            | "python"
            | "python3"
            | "wscript"
            | "cscript"
            | "npm"
            | "npx"
    ) {
        return Err("请选择 Agent 本身，不能将 shell、解释器或包管理器作为 ACP Agent".into());
    }
    let path = tokio::fs::canonicalize(path)
        .await
        .map_err(|_| "无法解析 Agent 可执行路径".to_string())?;
    Ok(Executable {
        program: path.clone(),
        prefix_args: Vec::new(),
        display_path: path,
    })
}

#[derive(Deserialize)]
#[serde(untagged)]
enum PackageBin {
    Single(String),
    Named(BTreeMap<String, String>),
}

#[derive(Deserialize)]
struct Package {
    name: String,
    bin: PackageBin,
}

async fn npm_entry(
    provider: AssistantProvider,
    prefix: &Path,
) -> Result<Option<Executable>, String> {
    let Some(package_name) = package_name(provider) else {
        return Ok(None);
    };
    let package_root = prefix.join("node_modules").join(package_name);
    let package_path = package_root.join("package.json");
    let file = match tokio::fs::File::open(&package_path).await {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("无法读取 Agent 安装包元数据".into()),
    };
    let mut bytes = Vec::new();
    file.take(65537)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| "无法读取 Agent 安装包元数据".to_string())?;
    if bytes.len() > 65536 {
        return Err("Agent 安装包元数据过大".into());
    }
    let package: Package =
        serde_json::from_slice(&bytes).map_err(|_| "Agent 安装包元数据无效".to_string())?;
    if package.name != package_name {
        return Err("Agent npm 包名称与预置不符".into());
    }
    let bin = match package.bin {
        PackageBin::Single(bin) => bin,
        PackageBin::Named(bins) => command_names(provider)
            .iter()
            .find_map(|name| bins.get(*name))
            .cloned()
            .ok_or_else(|| "Agent npm 包未提供预期入口".to_string())?,
    };
    let relative = Path::new(&bin);
    if relative.is_absolute()
        || relative.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir | std::path::Component::Prefix(_)
            )
        })
    {
        return Err("Agent npm 入口不能越出安装包目录".into());
    }
    let root = tokio::fs::canonicalize(&package_root)
        .await
        .map_err(|_| "Agent npm 安装目录不可访问".to_string())?;
    let entry = tokio::fs::canonicalize(root.join(relative))
        .await
        .map_err(|_| "Agent npm 入口不存在".to_string())?;
    if !entry.starts_with(&root)
        || !tokio::fs::metadata(&entry)
            .await
            .is_ok_and(|metadata| metadata.is_file())
    {
        return Err("Agent npm 入口越出安装包目录或不是文件".into());
    }
    let extension = entry
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "exe" => Ok(Some(Executable {
            program: entry.clone(),
            prefix_args: Vec::new(),
            display_path: entry,
        })),
        "js" | "mjs" | "cjs" => {
            let node = find_node(prefix).await.ok_or_else(|| {
                "Agent npm 包需要 Node.js；未找到原生 node 可执行文件".to_string()
            })?;
            let argument = node_script_argument(&entry)?;
            if tokio::fs::canonicalize(&argument)
                .await
                .map_err(|_| "无法确认 Node 入口路径".to_string())?
                != entry
            {
                return Err("Node 入口路径转换后指向了不同文件".into());
            }
            Ok(Some(Executable {
                program: node,
                prefix_args: vec![argument],
                display_path: entry,
            }))
        }
        _ => Err("Agent npm 入口不是受支持的 JavaScript 或原生可执行文件".into()),
    }
}

fn node_script_argument(path: &Path) -> Result<String, String> {
    if !path.is_absolute() {
        return Err("Agent JavaScript 入口必须使用绝对路径".into());
    }
    #[cfg(windows)]
    let normalized = {
        use std::path::{Component, Prefix};
        let mut components = path.components();
        let Some(Component::Prefix(prefix)) = components.next() else {
            return Err("Agent JavaScript 入口缺少 Windows 路径前缀".into());
        };
        let mut normalized = match prefix.kind() {
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                PathBuf::from(format!("{}:\\", char::from(letter)))
            }
            Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => {
                let mut root = std::ffi::OsString::from("\\\\");
                root.push(server);
                root.push("\\");
                root.push(share);
                root.push("\\");
                PathBuf::from(root)
            }
            _ => return Err("Agent JavaScript 入口不支持设备或卷标路径".into()),
        };
        if !matches!(components.next(), Some(Component::RootDir)) {
            return Err("Agent JavaScript 入口必须是 Windows 绝对路径".into());
        }
        for component in components {
            let Component::Normal(component) = component else {
                return Err("Agent JavaScript 入口不是已规范化的路径".into());
            };
            normalized.push(component);
        }
        normalized
    };
    #[cfg(not(windows))]
    let normalized = path.to_owned();
    // 包边界比较仍使用 canonical 路径；只在 Node argv 边界移除它不支持的扩展前缀。
    normalized
        .into_os_string()
        .into_string()
        .map_err(|_| "Agent JavaScript 入口不是有效文本路径".into())
}

async fn find_node(prefix: &Path) -> Option<PathBuf> {
    let mut directories = vec![prefix.to_owned()];
    directories.extend(search_directories());
    for directory in directories {
        let candidate = directory.join(if cfg!(windows) { "node.exe" } else { "node" });
        if tokio::fs::metadata(&candidate)
            .await
            .is_ok_and(|metadata| metadata.is_file())
        {
            if let Ok(path) = tokio::fs::canonicalize(candidate).await {
                return Some(path);
            }
        }
    }
    None
}

fn search_directories() -> Vec<PathBuf> {
    let mut directories = env::var_os("PATH")
        .map(|path| {
            env::split_paths(&path)
                .filter(|path| path.is_absolute())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if let Some(home) = dirs::home_dir() {
        directories.extend([
            home.join(".local/bin"),
            home.join(".kimi-code/bin"),
            home.join(".minimax-code"),
            home.join(".qoder/bin"),
            home.join(".trae/bin"),
            home.join(".codebuddy/bin"),
        ]);
    }
    if let Some(appdata) = env::var_os("APPDATA") {
        let appdata = PathBuf::from(appdata);
        if appdata.is_absolute() {
            directories.push(appdata.join("npm"));
        }
    }
    directories.dedup();
    directories
}

fn executable_suffixes() -> &'static [&'static str] {
    if cfg!(windows) {
        &[".exe", ".cmd", ".ps1"]
    } else {
        &[""]
    }
}

fn command_names(provider: AssistantProvider) -> &'static [&'static str] {
    match provider {
        AssistantProvider::Dsh => &["dsh"],
        AssistantProvider::Qwen => &["qwen"],
        AssistantProvider::Kimi => &["kimi"],
        AssistantProvider::MiniMax => &["mcode"],
        AssistantProvider::CodeBuddy => &["codebuddy"],
        AssistantProvider::Qoder => &["qoder"],
        AssistantProvider::Trae => &["traecli"],
        AssistantProvider::Builtin | AssistantProvider::Custom => &[],
    }
}

fn package_name(provider: AssistantProvider) -> Option<&'static str> {
    match provider {
        AssistantProvider::Dsh => Some("@deepseek-ai/dsh"),
        AssistantProvider::Qwen => Some("@qwen-code/qwen-code"),
        AssistantProvider::MiniMax => Some("@minimax-ai/code"),
        AssistantProvider::CodeBuddy => Some("@tencent-ai/codebuddy-code"),
        _ => None,
    }
}

pub(super) async fn version(executable: &Executable) -> Result<String, String> {
    let _permit = VERSION_PROBES
        .acquire()
        .await
        .map_err(|_| "Agent 版本探测不可用".to_string())?;
    let mut command = Command::new(&executable.program);
    command
        .args(&executable.prefix_args)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for name in [
        "NODE_OPTIONS",
        "NODE_PATH",
        "ELECTRON_RUN_AS_NODE",
        "BASH_ENV",
        "ENV",
    ] {
        command.env_remove(name);
    }
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command
        .spawn()
        .map_err(|_| "无法启动 Agent 版本探测".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "无法读取 Agent 版本输出".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "无法读取 Agent 版本输出".to_string())?;
    let output = probe_deadline(async {
        let (stdout, stderr, status) = tokio::join!(
            read_bounded(stdout, VERSION_STDOUT_LIMIT),
            read_bounded(stderr, VERSION_STDERR_LIMIT),
            child.wait()
        );
        Ok::<_, String>((
            stdout?,
            stderr?,
            status.map_err(|_| "Agent 版本探测失败".to_string())?,
        ))
    })
    .await;
    let (stdout, stderr, status) = match output {
        Ok(output) => output,
        Err(ProbeFailure::Failed(error)) => return Err(error),
        Err(ProbeFailure::TimedOut) => {
            let _ = child.kill().await;
            return Err("Agent 版本探测超过 5 秒，已停止".into());
        }
    };
    if !status.success() {
        return Err("Agent --version 执行失败；请检查该 Agent 的安装".into());
    }
    let text = format!(
        "{} {}",
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&stderr)
    );
    extract_version(&text).ok_or_else(|| "Agent 未返回可识别的版本号".into())
}

enum ProbeFailure {
    TimedOut,
    Failed(String),
}

async fn probe_deadline<T>(
    operation: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, ProbeFailure> {
    time::timeout(VERSION_TIMEOUT, operation)
        .await
        .map_err(|_| ProbeFailure::TimedOut)?
        .map_err(ProbeFailure::Failed)
}

async fn read_bounded(mut input: impl AsyncRead + Unpin, limit: usize) -> Result<Vec<u8>, String> {
    let mut stored = Vec::with_capacity(limit);
    let mut buffer = [0; 1024];
    loop {
        let read = input
            .read(&mut buffer)
            .await
            .map_err(|_| "无法读取 Agent 版本输出".to_string())?;
        if read == 0 {
            break;
        }
        let retain = read.min(limit.saturating_sub(stored.len()));
        stored.extend_from_slice(&buffer[..retain]);
    }
    Ok(stored)
}

fn extract_version(text: &str) -> Option<String> {
    text.split(|ch: char| !ch.is_ascii_alphanumeric() && !".-+".contains(ch))
        .find_map(|token| {
            let token = token.strip_prefix('v').unwrap_or(token);
            let mut parts = token.split(['.', '-', '+']);
            let major = parts.next()?;
            let minor = parts.next()?;
            let patch = parts.next()?;
            if [major, minor, patch]
                .into_iter()
                .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
                && token.len() <= 96
            {
                Some(token.to_owned())
            } else {
                None
            }
        })
}
