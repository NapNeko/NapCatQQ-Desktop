//! `PackageManager`:包管理器抽象。
//!
//! 设计要点:
//! - 不是所有 Host 都有 PackageManager(LocalWindows 默认 None,可选启用 winget)
//! - 各 PackageManager 自己知道怎么探测包是否已装、怎么 install
//! - 上层 Component(如 `LinuxQQComponent`)只问 host.pkg_manager() 拿一个,然后调用 trait
//!
//! 当前(M3.1)只有 trait 定义和 5 个空 stub 实装,具体行为在 M3.2/M3.3 落地。

use async_trait::async_trait;

use crate::error::HostError;

/// PackageManager 类型枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PackageManagerKind {
    Apt,
    Dnf,
    Pacman,
    Winget,
    Choco,
}

/// 包查询结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageInfo {
    pub name: String,
    pub version: Option<String>,
    pub installed: bool,
}

/// 包管理器统一接口。
#[async_trait]
pub trait PackageManager: Send + Sync {
    fn kind(&self) -> PackageManagerKind;

    /// 查询包是否已安装,以及版本号。
    async fn query(&self, name: &str) -> Result<PackageInfo, HostError>;

    /// 安装包(可能触发 sudo / UAC,取决于 Host 实装)。
    async fn install(&self, name: &str) -> Result<(), HostError>;

    /// 卸载包。
    async fn uninstall(&self, name: &str) -> Result<(), HostError>;

    /// 刷新包索引(apt update / dnf check-update / winget source update)。
    async fn refresh(&self) -> Result<(), HostError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_manager_kind_distinct() {
        // 编译期保证 5 个 variant 不重复
        let all = [
            PackageManagerKind::Apt,
            PackageManagerKind::Dnf,
            PackageManagerKind::Pacman,
            PackageManagerKind::Winget,
            PackageManagerKind::Choco,
        ];
        let unique: std::collections::HashSet<_> = all.iter().collect();
        assert_eq!(all.len(), unique.len());
    }

    #[test]
    fn package_info_struct_works() {
        let info = PackageInfo {
            name: "nodejs".to_string(),
            version: Some("20.10.0".to_string()),
            installed: true,
        };
        assert_eq!(info.name, "nodejs");
        assert!(info.installed);
    }
}
