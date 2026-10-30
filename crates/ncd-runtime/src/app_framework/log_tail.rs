//! 应用端日志尾巴:开页拉取用,不走 broadcast(启动对账推的行前端还没订阅会丢)

use ncd_host::{Host, HostCommand, HostPath, Locality, PathStyle, shell_single_quote};
use std::time::Duration;

const MAX_BYTES: u64 = 512 * 1024;

pub fn pick_first_log_path(listing: &str) -> Option<String> {
    listing
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && line.ends_with(".log"))
        .map(str::to_string)
}

pub async fn file_size(host: &dyn Host, path: &HostPath) -> Option<u64> {
    if host.os() == ncd_host::Os::Windows && host.locality() == Locality::Remote {
        return host.file_size(path).await.ok().flatten();
    }
    match host.locality() {
        Locality::Local => {
            let rendered = path.render(host_path_style(host));
            tokio::fs::metadata(rendered).await.ok().map(|m| m.len())
        }
        Locality::Remote => ncd_host::remote_file_size(host, path.as_posix()).await,
    }
}

pub async fn tail_file(host: &dyn Host, path: &HostPath, lines: usize) -> Vec<String> {
    if lines == 0 {
        return Vec::new();
    }
    if host.os() == ncd_host::Os::Windows && host.locality() == Locality::Remote {
        let quoted = path.render_for(ncd_host::Os::Windows).replace('\'', "''");
        let script = format!(
            "if (Test-Path -LiteralPath '{quoted}' -PathType Leaf) {{ Get-Content -LiteralPath '{quoted}' -Tail {} -Encoding UTF8 }}",
            lines.min(2000)
        );
        return host
            .run_to_string(
                HostCommand::new("powershell.exe")
                    .args(["-NoProfile", "-NonInteractive", "-Command"])
                    .arg(script)
                    .timeout(Duration::from_secs(10)),
            )
            .await
            .ok()
            .filter(|out| out.success())
            .map(|out| out.stdout.lines().map(str::to_string).collect())
            .unwrap_or_default();
    }
    match host.locality() {
        Locality::Local => tail_local(path, host_path_style(host), lines).await,
        Locality::Remote => tail_remote(host, path.as_posix(), lines).await,
    }
}

pub async fn newest_project_log(host: &dyn Host, install_dir: &str) -> Option<HostPath> {
    if host.os() == ncd_host::Os::Windows && host.locality() == Locality::Remote {
        let root = HostPath::from_posix(install_dir);
        let mut found = Vec::new();
        for directory in [root.clone(), root.join("logs")] {
            if let Ok(entries) = host.list_dir(&directory).await {
                found.extend(
                    entries
                        .into_iter()
                        .filter(|entry| {
                            !entry.is_dir
                                && !entry.is_symlink
                                && entry.size != 0
                                && entry.name.ends_with(".log")
                        })
                        .map(|entry| (entry.modified, directory.join(entry.name))),
                );
            }
        }
        return found
            .into_iter()
            .max_by_key(|(modified, _)| *modified)
            .map(|(_, path)| path);
    }
    match host.locality() {
        Locality::Remote => {
            let dir = shell_single_quote(install_dir);
            let cmd = HostCommand::new("sh").arg("-c").arg(format!(
                "ls -1t {dir}/.ncd-*.log {dir}/logs/*.log {dir}/*.log 2>/dev/null | head -1"
            ));
            let out = host.run_to_string(cmd).await.ok()?;
            let path = pick_first_log_path(&out.stdout)?;
            Some(HostPath::from_posix(&path))
        }
        Locality::Local => newest_local_log(install_dir, host_path_style(host)),
    }
}

pub async fn tail_journal(
    host: &dyn Host,
    units: &[String],
    pid: Option<u32>,
    lines: usize,
) -> Vec<String> {
    if host.locality() != Locality::Remote || host.os() != ncd_host::Os::Linux || lines == 0 {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    if let Some(pid) = pid {
        let out = run_sh(
            host,
            &format!("journalctl _PID={pid} -n {lines} --no-pager -o cat 2>/dev/null || true"),
        )
        .await;
        if !out.is_empty() {
            chunks = out;
        }
    }
    for unit in units {
        let name = unit
            .trim()
            .trim_end_matches(".service")
            .replace(['\'', ';', '|', '&', '$', '`'], "");
        if name.is_empty() {
            continue;
        }
        let out = run_sh(
            host,
            &format!(
                "journalctl -u {name} -u {name}.service -n {lines} --no-pager -o cat 2>/dev/null || true"
            ),
        )
        .await;
        if !out.is_empty() {
            chunks = out;
            break;
        }
    }
    chunks
}

fn host_path_style(host: &dyn Host) -> PathStyle {
    match host.os() {
        ncd_host::Os::Windows => PathStyle::Windows,
        _ => PathStyle::Posix,
    }
}

fn newest_local_log(install_dir: &str, style: PathStyle) -> Option<HostPath> {
    let root = HostPath::from_posix(install_dir);
    let rendered = root.render(style);
    let dir = std::path::Path::new(&rendered);
    let mut best: Option<(std::time::SystemTime, std::path::PathBuf)> = None;
    let candidates = [dir.to_path_buf(), dir.join("logs")];
    for folder in candidates {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("log") {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if !meta.is_file() || meta.len() == 0 {
                continue;
            }
            let modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            if best.as_ref().map(|(t, _)| modified > *t).unwrap_or(true) {
                best = Some((modified, path));
            }
        }
    }
    best.map(|(_, path)| match style {
        PathStyle::Windows => HostPath::from_windows(&path.to_string_lossy()),
        PathStyle::Posix => HostPath::from_posix(path.to_string_lossy().into_owned()),
    })
}

async fn tail_local(path: &HostPath, style: PathStyle, lines: usize) -> Vec<String> {
    let rendered = path.render(style);
    let Ok(meta) = tokio::fs::metadata(&rendered).await else {
        return Vec::new();
    };
    let start = meta.len().saturating_sub(MAX_BYTES);
    let Ok(mut file) = tokio::fs::File::open(&rendered).await else {
        return Vec::new();
    };
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    if file.seek(std::io::SeekFrom::Start(start)).await.is_err() {
        return Vec::new();
    }
    let mut buf = Vec::new();
    if file.read_to_end(&mut buf).await.is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&buf);
    let all: Vec<&str> = text.lines().collect();
    let skip = all.len().saturating_sub(lines);
    all.into_iter().skip(skip).map(str::to_string).collect()
}

// 开页拉取的远端命令上限:SSH 卡住时宁可先给空尾巴,也不让日志页一直转圈
const REMOTE_TIMEOUT: Duration = Duration::from_secs(15);

async fn tail_remote(host: &dyn Host, path: &str, lines: usize) -> Vec<String> {
    ncd_host::remote_tail_lines(host, path, lines, Some(REMOTE_TIMEOUT))
        .await
        .map(drop_blank_lines)
        .unwrap_or_default()
}

async fn run_sh(host: &dyn Host, script: &str) -> Vec<String> {
    let out = host
        .run_to_string(
            HostCommand::new("sh")
                .arg("-c")
                .arg(script)
                .timeout(REMOTE_TIMEOUT),
        )
        .await;
    match out {
        Ok(o) => drop_blank_lines(o.stdout.lines().map(str::to_string).collect()),
        Err(_) => Vec::new(),
    }
}

fn drop_blank_lines(lines: Vec<String>) -> Vec<String> {
    lines.into_iter().filter(|l| !l.trim().is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_first_skips_blank_and_non_log() {
        assert_eq!(
            pick_first_log_path("\n/tmp/a.pid\n/root/bot/.ncd-nonebot2.log\n"),
            Some("/root/bot/.ncd-nonebot2.log".into())
        );
        assert_eq!(pick_first_log_path(""), None);
        assert_eq!(pick_first_log_path("/tmp/notes.txt"), None);
    }
}
