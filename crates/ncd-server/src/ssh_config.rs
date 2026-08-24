//! 本机 OpenSSH 用户 config 子集解析：发现可导入的 Host。
//!
//! 只读文本与路径，不读私钥内容。不实现完整 ssh_config(5)：
//! 无 Match exec、无跳板连接、不读系统级 ssh_config。
//! Include 按插入点展开；每个参数取第一次命中（具体 Host 应写在 Host * 前面）。

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::server_manager::{AuthMethod, ServerProfile};

const SKIP_WILDCARD: &str = "通配别名不能作为档案";
const SKIP_PROXY: &str = "暂不支持跳板（ProxyJump / ProxyCommand）";
const SKIP_ADDED: &str = "已添加";

/// 从 ~/.ssh/config 发现的一条 Host，供导入 UI 勾选。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DiscoveredSshHost {
    /// config 里的 Host 别名，导入后作档案 name
    pub alias: String,
    /// HostName；缺省则用 alias
    pub host: String,
    /// SSH 端口，默认 22
    pub port: u16,
    /// User；缺省则本机用户名
    pub username: String,
    /// 展开后的 IdentityFile 绝对路径
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_file: Option<String>,
    pub auth_method: AuthMethod,
    /// 写了 IdentityFile 但本地文件都不存在
    #[serde(default)]
    pub identity_file_missing: bool,
    pub selectable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_reason: Option<String>,
    pub already_added: bool,
}

struct HostBlock {
    patterns: Vec<String>,
    entries: Vec<(String, String)>,
}

enum Directive {
    Host(Vec<String>),
    Match,
    Kv(String, String),
}

/// Windows 用 USERNAME，其它用 USER；都没有则空串。
pub fn default_ssh_username() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_default()
}

/// 解析 `config_path`。文件不存在返回空列表；主文件读失败才 Err。
pub fn discover_ssh_hosts(
    config_path: &Path,
    home: &Path,
    existing: &[ServerProfile],
    default_user: &str,
) -> Result<Vec<DiscoveredSshHost>, String> {
    if !config_path.is_file() {
        return Ok(Vec::new());
    }
    let mut visiting = HashSet::new();
    let directives = read_directives(config_path, home, &mut visiting)?;
    let blocks = group_blocks(directives);
    Ok(discover_from_blocks(&blocks, home, existing, default_user))
}

fn read_directives(
    path: &Path,
    home: &Path,
    visiting: &mut HashSet<PathBuf>,
) -> Result<Vec<Directive>, String> {
    let key = normalize_path(path);
    if !visiting.insert(key.clone()) {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path).map_err(|e| format!("读取 SSH config 失败: {e}"))?;
    let config_dir = path.parent().unwrap_or(path);
    let mut out = Vec::new();
    for line in text.lines() {
        match parse_config_line(line) {
            None => {}
            Some(Line::Include(tokens)) => {
                for token in tokens {
                    for included in expand_include(&token, config_dir, home) {
                        if let Ok(nested) = read_directives(&included, home, visiting) {
                            out.extend(nested);
                        }
                    }
                }
            }
            Some(Line::Host(patterns)) => out.push(Directive::Host(patterns)),
            Some(Line::Match) => out.push(Directive::Match),
            Some(Line::Kv(k, v)) => out.push(Directive::Kv(k, v)),
        }
    }
    visiting.remove(&key);
    Ok(out)
}

fn group_blocks(directives: Vec<Directive>) -> Vec<HostBlock> {
    let mut blocks = Vec::new();
    let mut current: Option<HostBlock> = None;
    let mut skipping_match = false;
    for d in directives {
        match d {
            Directive::Host(patterns) => {
                skipping_match = false;
                if let Some(block) = current.take() {
                    blocks.push(block);
                }
                current = Some(HostBlock {
                    patterns,
                    entries: Vec::new(),
                });
            }
            Directive::Match => {
                skipping_match = true;
                if let Some(block) = current.take() {
                    blocks.push(block);
                }
                current = None;
            }
            Directive::Kv(k, v) => {
                if skipping_match {
                    continue;
                }
                if current.is_none() {
                    current = Some(HostBlock {
                        patterns: vec!["*".to_string()],
                        entries: Vec::new(),
                    });
                }
                if let Some(block) = current.as_mut() {
                    block.entries.push((k, v));
                }
            }
        }
    }
    if let Some(block) = current {
        blocks.push(block);
    }
    blocks
}

fn discover_from_blocks(
    blocks: &[HostBlock],
    home: &Path,
    existing: &[ServerProfile],
    default_user: &str,
) -> Vec<DiscoveredSshHost> {
    let mut aliases = Vec::new();
    let mut seen = HashSet::new();
    for block in blocks {
        for pattern in &block.patterns {
            if pattern.starts_with('!') || pattern == "*" {
                continue;
            }
            if seen.insert(pattern.clone()) {
                aliases.push(pattern.clone());
            }
        }
    }

    aliases
        .into_iter()
        .map(|alias| resolve_alias(&alias, blocks, home, existing, default_user))
        .collect()
}

struct Resolved {
    hostname: Option<String>,
    user: Option<String>,
    port: Option<u16>,
    identity_files: Vec<String>,
    proxy_jump: Option<String>,
    proxy_command: Option<String>,
}

fn resolve_alias(
    alias: &str,
    blocks: &[HostBlock],
    home: &Path,
    existing: &[ServerProfile],
    default_user: &str,
) -> DiscoveredSshHost {
    let mut resolved = Resolved {
        hostname: None,
        user: None,
        port: None,
        identity_files: Vec::new(),
        proxy_jump: None,
        proxy_command: None,
    };
    for block in blocks {
        if !block_matches(block, alias) {
            continue;
        }
        for (key, value) in &block.entries {
            match key.as_str() {
                "hostname" if resolved.hostname.is_none() => {
                    resolved.hostname = Some(value.clone());
                }
                "user" if resolved.user.is_none() => {
                    resolved.user = Some(value.clone());
                }
                "port" if resolved.port.is_none() => {
                    if let Ok(port) = value.parse::<u16>() {
                        if port != 0 {
                            resolved.port = Some(port);
                        }
                    }
                }
                "identityfile" => resolved.identity_files.push(value.clone()),
                "proxyjump" if resolved.proxy_jump.is_none() && !value.is_empty() => {
                    resolved.proxy_jump = Some(value.clone());
                }
                "proxycommand" if resolved.proxy_command.is_none() && !value.is_empty() => {
                    resolved.proxy_command = Some(value.clone());
                }
                _ => {}
            }
        }
    }

    let host = resolved
        .hostname
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| alias.to_string());
    let port = resolved.port.unwrap_or(22);
    let username = resolved
        .user
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| default_user.to_string());

    let (identity_file, identity_file_missing) = pick_identity(&resolved.identity_files, home);
    let auth_method = if identity_file.is_some() {
        AuthMethod::Key
    } else {
        AuthMethod::Password
    };

    let is_wildcard = is_glob_pattern(alias);
    let has_proxy = resolved.proxy_jump.is_some() || resolved.proxy_command.is_some();
    let already_added = existing.iter().any(|p| {
        p.host.eq_ignore_ascii_case(&host) && p.port == port && p.username == username
    });

    let skip_reason = if is_wildcard {
        Some(SKIP_WILDCARD.to_string())
    } else if has_proxy {
        Some(SKIP_PROXY.to_string())
    } else if already_added {
        Some(SKIP_ADDED.to_string())
    } else {
        None
    };

    DiscoveredSshHost {
        alias: alias.to_string(),
        host,
        port,
        username,
        identity_file,
        auth_method,
        identity_file_missing,
        selectable: skip_reason.is_none(),
        skip_reason,
        already_added,
    }
}

fn pick_identity(raw_paths: &[String], home: &Path) -> (Option<String>, bool) {
    if raw_paths.is_empty() {
        return (None, false);
    }
    let expanded: Vec<PathBuf> = raw_paths.iter().map(|p| expand_path(p, home)).collect();
    if let Some(found) = expanded.iter().find(|p| p.is_file()) {
        return (Some(found.to_string_lossy().into_owned()), false);
    }
    (
        Some(expanded[0].to_string_lossy().into_owned()),
        true,
    )
}

fn block_matches(block: &HostBlock, alias: &str) -> bool {
    let mut positive = false;
    for pattern in &block.patterns {
        if let Some(negated) = pattern.strip_prefix('!') {
            if glob_match(negated, alias) {
                return false;
            }
        } else if glob_match(pattern, alias) {
            positive = true;
        }
    }
    positive
}

fn is_glob_pattern(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?')
}

fn glob_match(pat: &str, text: &str) -> bool {
    glob_match_bytes(pat.as_bytes(), text.as_bytes())
}

fn glob_match_bytes(pat: &[u8], text: &[u8]) -> bool {
    let mut pi = 0;
    let mut ti = 0;
    let mut star_p: Option<usize> = None;
    let mut star_t = 0;
    while ti < text.len() {
        if pi < pat.len() && (pat[pi] == b'?' || pat[pi] == text[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < pat.len() && pat[pi] == b'*' {
            star_p = Some(pi);
            star_t = ti;
            pi += 1;
        } else if let Some(sp) = star_p {
            pi = sp + 1;
            star_t += 1;
            ti = star_t;
        } else {
            return false;
        }
    }
    while pi < pat.len() && pat[pi] == b'*' {
        pi += 1;
    }
    pi == pat.len()
}

enum Line {
    Include(Vec<String>),
    Host(Vec<String>),
    Match,
    Kv(String, String),
}

fn parse_config_line(line: &str) -> Option<Line> {
    let line = strip_unquoted_comment(line);
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let (kw, rest) = split_keyword(line)?;
    let kw_l = kw.to_ascii_lowercase();
    match kw_l.as_str() {
        "include" => {
            let tokens = split_values(&rest);
            if tokens.is_empty() {
                None
            } else {
                Some(Line::Include(tokens))
            }
        }
        "host" => {
            let patterns = split_values(&rest);
            if patterns.is_empty() {
                None
            } else {
                Some(Line::Host(patterns))
            }
        }
        "match" => Some(Line::Match),
        _ => {
            let value = unquote(rest.trim());
            if value.is_empty() {
                None
            } else {
                Some(Line::Kv(kw_l, value))
            }
        }
    }
}

fn strip_unquoted_comment(line: &str) -> String {
    let mut out = String::new();
    let mut quote: Option<char> = None;
    for c in line.chars() {
        if quote.is_none() && c == '#' {
            break;
        }
        match quote {
            None if c == '"' || c == '\'' => quote = Some(c),
            Some(q) if c == q => quote = None,
            _ => {}
        }
        out.push(c);
    }
    out
}

fn split_keyword(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let mut kw_end = 0;
    for (i, c) in line.char_indices() {
        if c.is_whitespace() || c == '=' {
            kw_end = i;
            break;
        }
        kw_end = i + c.len_utf8();
    }
    if kw_end == 0 {
        return None;
    }
    let kw = line[..kw_end].to_string();
    let mut rest = line[kw_end..].trim_start();
    if let Some(stripped) = rest.strip_prefix('=') {
        rest = stripped.trim_start();
    }
    Some((kw, rest.to_string()))
}

fn split_values(rest: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut quote: Option<char> = None;
    for c in rest.chars() {
        match quote {
            Some(q) if c == q => {
                quote = None;
            }
            Some(_) => buf.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
            }
            None if c.is_whitespace() => {
                if !buf.is_empty() {
                    out.push(std::mem::take(&mut buf));
                }
            }
            None => buf.push(c),
        }
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    out
}

fn unquote(value: &str) -> String {
    let v = value.trim();
    if v.len() >= 2 {
        let bytes = v.as_bytes();
        if (bytes[0] == b'"' && bytes[v.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[v.len() - 1] == b'\'')
        {
            return v[1..v.len() - 1].to_string();
        }
    }
    v.to_string()
}

fn expand_include(token: &str, config_dir: &Path, home: &Path) -> Vec<PathBuf> {
    let path = if token.starts_with('~') {
        expand_path(token, home)
    } else {
        let p = PathBuf::from(token);
        if p.is_absolute() {
            p
        } else {
            config_dir.join(token)
        }
    };
    glob_files(&path)
}

fn expand_path(raw: &str, home: &Path) -> PathBuf {
    let raw = raw.trim();
    if raw == "~" {
        return home.to_path_buf();
    }
    if let Some(rest) = raw
        .strip_prefix("~/")
        .or_else(|| raw.strip_prefix("~\\"))
    {
        return join_normalized(home, rest);
    }
    let p = PathBuf::from(raw);
    if p.is_absolute() {
        p
    } else {
        join_normalized(home, raw)
    }
}

/// 把 `a/b` 与 `a\b` 都拆成组件再 join，避免 Windows 上留下 `.ssh/id_ed25519` 这种混用分隔符。
fn join_normalized(base: &Path, rel: &str) -> PathBuf {
    let mut out = base.to_path_buf();
    for part in rel.split(['/', '\\']) {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            let _ = out.pop();
            continue;
        }
        out.push(part);
    }
    out
}

fn glob_files(pattern_path: &Path) -> Vec<PathBuf> {
    let Some(name) = pattern_path.file_name().and_then(|s| s.to_str()) else {
        return Vec::new();
    };
    if !is_glob_pattern(name) {
        return if pattern_path.is_file() {
            vec![pattern_path.to_path_buf()]
        } else {
            Vec::new()
        };
    }
    let dir = pattern_path.parent().unwrap_or_else(|| Path::new("."));
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        let fname = entry.file_name();
        let Some(s) = fname.to_str() else {
            continue;
        };
        if glob_match(name, s) && entry.path().is_file() {
            out.push(entry.path());
        }
    }
    out.sort();
    out
}

fn normalize_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write_config(home: &Path, body: &str) -> PathBuf {
        let ssh = home.join(".ssh");
        fs::create_dir_all(&ssh).unwrap();
        let path = ssh.join("config");
        fs::write(&path, body).unwrap();
        path
    }

    fn empty_profile(host: &str, port: u16, username: &str) -> ServerProfile {
        ServerProfile {
            id: "s1".into(),
            name: host.into(),
            host: host.into(),
            port,
            username: username.into(),
            auth_method: AuthMethod::Password,
            private_key_path: None,
            remember_credential: true,
            state: crate::server_manager::ServerState::Disconnected,
            health: None,
            webui_url: None,
        }
    }

    #[test]
    fn missing_config_returns_empty() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let cfg = home.join(".ssh").join("config");
        let hosts = discover_ssh_hosts(&cfg, home, &[], "me").unwrap();
        assert!(hosts.is_empty());
    }

    #[test]
    fn parses_named_host_with_key() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let key = home.join(".ssh").join("id_ed25519");
        fs::create_dir_all(key.parent().unwrap()).unwrap();
        fs::write(&key, "dummy").unwrap();
        let cfg = write_config(
            home,
            r#"
Host prod
    HostName 10.0.0.8
    User ubuntu
    Port 22022
    IdentityFile ~/.ssh/id_ed25519
"#,
        );
        let hosts = discover_ssh_hosts(&cfg, home, &[], "me").unwrap();
        assert_eq!(hosts.len(), 1);
        let h = &hosts[0];
        assert_eq!(h.alias, "prod");
        assert_eq!(h.host, "10.0.0.8");
        assert_eq!(h.port, 22022);
        assert_eq!(h.username, "ubuntu");
        assert_eq!(h.auth_method, AuthMethod::Key);
        assert!(!h.identity_file_missing);
        assert!(h.selectable);
        assert_eq!(
            h.identity_file.as_deref(),
            Some(key.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn host_star_merges_when_after_specific() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let cfg = write_config(
            home,
            r#"
Host prod
    HostName 1.2.3.4
    User ubuntu

Host *
    Port 2222
    User ignored
"#,
        );
        let hosts = discover_ssh_hosts(&cfg, home, &[], "me").unwrap();
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].port, 2222);
        assert_eq!(hosts[0].username, "ubuntu");
        assert!(!hosts.iter().any(|h| h.alias == "*"));
    }

    #[test]
    fn first_match_wins_when_star_comes_first() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let cfg = write_config(
            home,
            r#"
Host *
    User defaultuser

Host prod
    HostName 1.2.3.4
    User ubuntu
"#,
        );
        let hosts = discover_ssh_hosts(&cfg, home, &[], "me").unwrap();
        assert_eq!(hosts[0].username, "defaultuser");
        assert_eq!(hosts[0].host, "1.2.3.4");
    }

    #[test]
    fn include_is_expanded() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let ssh = home.join(".ssh");
        fs::create_dir_all(ssh.join("config.d")).unwrap();
        fs::write(
            ssh.join("config.d").join("extra.conf"),
            "Host extra\n    HostName 9.9.9.9\n    User bob\n",
        )
        .unwrap();
        let cfg = write_config(home, "Include config.d/*.conf\n");
        let hosts = discover_ssh_hosts(&cfg, home, &[], "me").unwrap();
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].alias, "extra");
        assert_eq!(hosts[0].host, "9.9.9.9");
        assert_eq!(hosts[0].username, "bob");
    }

    #[test]
    fn wildcard_alias_is_listed_but_not_selectable() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let cfg = write_config(
            home,
            "Host *.lab\n    User root\nHost box\n    HostName 10.1.1.1\n    User root\n",
        );
        let hosts = discover_ssh_hosts(&cfg, home, &[], "me").unwrap();
        let wild = hosts.iter().find(|h| h.alias == "*.lab").unwrap();
        assert!(!wild.selectable);
        assert_eq!(wild.skip_reason.as_deref(), Some(SKIP_WILDCARD));
        let box_h = hosts.iter().find(|h| h.alias == "box").unwrap();
        assert!(box_h.selectable);
    }

    #[test]
    fn proxy_jump_is_not_selectable() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let cfg = write_config(
            home,
            "Host jump\n    HostName 10.0.0.1\n    User root\n    ProxyJump bastion\n",
        );
        let hosts = discover_ssh_hosts(&cfg, home, &[], "me").unwrap();
        assert_eq!(hosts.len(), 1);
        assert!(!hosts[0].selectable);
        assert_eq!(hosts[0].skip_reason.as_deref(), Some(SKIP_PROXY));
    }

    #[test]
    fn already_added_matches_host_port_user() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let cfg = write_config(
            home,
            "Host prod\n    HostName 10.0.0.8\n    User ubuntu\n    Port 22\n",
        );
        let existing = vec![empty_profile("10.0.0.8", 22, "ubuntu")];
        let hosts = discover_ssh_hosts(&cfg, home, &existing, "me").unwrap();
        assert!(!hosts[0].selectable);
        assert!(hosts[0].already_added);
        assert_eq!(hosts[0].skip_reason.as_deref(), Some(SKIP_ADDED));
    }

    #[test]
    fn missing_user_uses_default() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let cfg = write_config(home, "Host prod\n    HostName example.com\n");
        let hosts = discover_ssh_hosts(&cfg, home, &[], "localme").unwrap();
        assert_eq!(hosts[0].username, "localme");
        assert_eq!(hosts[0].host, "example.com");
        assert_eq!(hosts[0].port, 22);
        assert_eq!(hosts[0].auth_method, AuthMethod::Password);
    }

    #[test]
    fn host_foo_bar_splits_into_two_aliases() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let cfg = write_config(
            home,
            "Host foo bar\n    HostName 1.1.1.1\n    User a\n",
        );
        let hosts = discover_ssh_hosts(&cfg, home, &[], "me").unwrap();
        let names: Vec<_> = hosts.iter().map(|h| h.alias.as_str()).collect();
        assert_eq!(names, vec!["foo", "bar"]);
        assert_eq!(hosts[0].host, "1.1.1.1");
        assert_eq!(hosts[1].host, "1.1.1.1");
    }

    #[test]
    fn missing_identity_file_still_selectable_with_flag() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let cfg = write_config(
            home,
            "Host prod\n    HostName 1.2.3.4\n    User u\n    IdentityFile ~/.ssh/nope\n",
        );
        let hosts = discover_ssh_hosts(&cfg, home, &[], "me").unwrap();
        assert!(hosts[0].selectable);
        assert!(hosts[0].identity_file_missing);
        assert_eq!(hosts[0].auth_method, AuthMethod::Key);
    }

    #[test]
    fn match_block_is_ignored() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let cfg = write_config(
            home,
            r#"
Match host evil
    User hacked

Host prod
    HostName 1.2.3.4
    User ubuntu
"#,
        );
        let hosts = discover_ssh_hosts(&cfg, home, &[], "me").unwrap();
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].username, "ubuntu");
    }

    #[test]
    fn equals_and_tilde_identity() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let key = home.join(".ssh").join("id_rsa");
        fs::create_dir_all(key.parent().unwrap()).unwrap();
        fs::write(&key, "k").unwrap();
        let cfg = write_config(
            home,
            "Host prod\n    HostName=box.example\n    Port = 2222\n    IdentityFile ~/.ssh/id_rsa\n    User=git\n",
        );
        let hosts = discover_ssh_hosts(&cfg, home, &[], "me").unwrap();
        assert_eq!(hosts[0].host, "box.example");
        assert_eq!(hosts[0].port, 2222);
        assert_eq!(hosts[0].username, "git");
        assert!(!hosts[0].identity_file_missing);
    }
}
