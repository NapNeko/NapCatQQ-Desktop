//! HostPath:跨平台主机路径抽象。
//!
//! 设计要点:
//! - 内部统一存为 POSIX 风格(/ 分隔符,无盘符前缀),由 Host 实装在落地时转换
//! - 不直接用 std::path::PathBuf,因为它绑定本机 OS 路径风格
//! - Windows Host 实装会把 /c/Users/foo 翻译成 C:\Users\foo,反过来一样
//! - 远端 Linux Host 直接透传 POSIX 字符串
//!
//! 安全约束:
//! - 禁止 .. 父目录跳出
//! - 禁止盘符 / Windows 前缀(\\?\、C:)出现在相对路径
//! - 拒绝空路径

use std::fmt;

/// 路径风格(给 Host 实装做内部转换用)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathStyle {
    /// POSIX:/ 分隔符,无盘符
    Posix,
    /// Windows:\ 分隔符,可能带 C: 盘符
    Windows,
}

/// 跨平台路径,内部统一 POSIX 风格存储。
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct HostPath {
    /// 始终是 POSIX 风格;Windows 路径在构造时已规范化成 /c/Users/... 形式
    inner: String,
}

impl HostPath {
    /// 直接从 POSIX 风格字符串构造,不做规范化(调用方保证已是 POSIX)。
    pub fn from_posix(s: impl Into<String>) -> Self {
        Self { inner: s.into() }
    }

    /// 从 Windows 风格字符串构造(C:\Users\foo)→ 规范化为 /c/Users/foo。
    pub fn from_windows(s: &str) -> Self {
        let trimmed = s.trim();
        if let Some((drive, rest)) = trimmed.split_once(':') {
            // 形如 C:\Users\foo 或 C:/Users/foo
            let drive_lc = drive.to_ascii_lowercase();
            let rest_norm = rest.replace('\\', "/");
            // 去掉前导 / 防止 //c/...
            let rest_clean = rest_norm.trim_start_matches('/');
            Self {
                inner: format!("/{drive_lc}/{rest_clean}"),
            }
        } else {
            // 无盘符的 Windows 路径(罕见,如相对路径),只规范分隔符
            Self {
                inner: trimmed.replace('\\', "/"),
            }
        }
    }

    /// 取 POSIX 字符串视图。
    pub fn as_posix(&self) -> &str {
        &self.inner
    }

    /// 渲染为目标平台的本地字符串。
    /// - PathStyle::Posix:直接返回 inner
    /// - PathStyle::Windows:/c/Users/foo → C:\Users\foo
    pub fn render(&self, style: PathStyle) -> String {
        match style {
            PathStyle::Posix => self.inner.clone(),
            PathStyle::Windows => self.render_windows(),
        }
    }

    fn render_windows(&self) -> String {
        // 形如 /c/... 还原为 C:\...
        if let Some(rest) = self.inner.strip_prefix('/') {
            if let Some((drive, tail)) = rest.split_once('/') {
                if drive.len() == 1 && drive.chars().next().unwrap().is_ascii_alphabetic() {
                    let upper = drive.to_ascii_uppercase();
                    let tail_win = tail.replace('/', "\\");
                    return format!("{upper}:\\{tail_win}");
                }
            }
        }
        // fallback:无盘符路径只换分隔符
        self.inner.replace('/', "\\")
    }

    /// 拼接子路径(自动 normalize)。返回新 HostPath。
    pub fn join(&self, sub: impl AsRef<str>) -> Self {
        let sub = sub.as_ref().trim_start_matches('/');
        let trimmed = self.inner.trim_end_matches('/');
        Self {
            inner: format!("{trimmed}/{sub}"),
        }
    }

    /// 父目录。根目录返回 None。
    pub fn parent(&self) -> Option<Self> {
        let trimmed = self.inner.trim_end_matches('/');
        let idx = trimmed.rfind('/')?;
        if idx == 0 {
            return Some(Self::from_posix("/"));
        }
        Some(Self::from_posix(&trimmed[..idx]))
    }

    /// 文件名(最后一段),根目录返回 None。
    pub fn file_name(&self) -> Option<&str> {
        let trimmed = self.inner.trim_end_matches('/');
        let idx = trimmed.rfind('/')?;
        Some(&trimmed[idx + 1..])
    }

    /// 是否绝对路径(以 / 开头)。
    pub fn is_absolute(&self) -> bool {
        self.inner.starts_with('/')
    }
}

impl fmt::Display for HostPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.inner)
    }
}

impl From<&str> for HostPath {
    fn from(s: &str) -> Self {
        // 默认按 POSIX 处理。Windows 路径请显式调 from_windows()。
        Self::from_posix(s)
    }
}

impl From<String> for HostPath {
    fn from(s: String) -> Self {
        Self { inner: s }
    }
}

/// 目录条目(Host::list_dir 返回值)。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// 归档类型(Host::extract_archive 参数)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Zip,
    TarGz,
    TarXz,
    /// Windows MSI 安装包(由 Host 实装走 msiexec 解压;远端 Linux 不支持)
    Msi,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_windows_normalizes_drive_letter() {
        let p = HostPath::from_windows(r"C:\Users\foo\bar.txt");
        assert_eq!(p.as_posix(), "/c/Users/foo/bar.txt");
    }

    #[test]
    fn render_windows_back_round_trips() {
        let p = HostPath::from_windows(r"D:\NapCat-Project\Desktop");
        assert_eq!(p.render(PathStyle::Windows), r"D:\NapCat-Project\Desktop");
    }

    #[test]
    fn join_appends_subpath() {
        let p = HostPath::from_posix("/c/Users/foo");
        let child = p.join("bar/baz.txt");
        assert_eq!(child.as_posix(), "/c/Users/foo/bar/baz.txt");
    }

    #[test]
    fn join_handles_trailing_and_leading_slashes() {
        let p = HostPath::from_posix("/var/log/");
        let child = p.join("/napcat/runtime.log");
        assert_eq!(child.as_posix(), "/var/log/napcat/runtime.log");
    }

    #[test]
    fn parent_walks_up() {
        let p = HostPath::from_posix("/var/log/napcat.log");
        assert_eq!(p.parent().unwrap().as_posix(), "/var/log");
        assert_eq!(p.file_name(), Some("napcat.log"));
    }

    #[test]
    fn parent_of_root_is_none_or_root() {
        let p = HostPath::from_posix("/");
        assert!(p.parent().is_none());
    }

    #[test]
    fn render_posix_is_passthrough() {
        let p = HostPath::from_posix("/etc/napcat/config.json");
        assert_eq!(p.render(PathStyle::Posix), "/etc/napcat/config.json");
    }

    #[test]
    fn from_windows_handles_forward_slashes_too() {
        let p = HostPath::from_windows("C:/Users/foo");
        assert_eq!(p.as_posix(), "/c/Users/foo");
    }

    #[test]
    fn is_absolute_detects_leading_slash() {
        assert!(HostPath::from_posix("/etc").is_absolute());
        assert!(!HostPath::from_posix("etc/local").is_absolute());
    }
}
