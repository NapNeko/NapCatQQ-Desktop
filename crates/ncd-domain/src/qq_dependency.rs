//! QQ 系统依赖管理的领域模型。
//!
//! 定义了跨发行版的依赖项、检测报告、安装结果等核心类型。
//! 遵守 Layer 1 原则：零运行时依赖，纯数据结构。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// QQ 依赖项分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/qq/")]
#[serde(rename_all = "snake_case")]
pub enum DependencyCategory {
    /// 核心运行时库（NSS, GBM, GTK, ALSA 等）
    Runtime,
    /// X11 图形栈（Xvfb, x11vnc, openbox）
    Graphics,
    /// 工具链（curl, jq, unzip, screen）
    Toolchain,
}

/// 跨发行版依赖项定义。
///
/// 用通用名（canonical_name）作为主键，映射到各发行版的具体包名。
/// t64 变体通过运行时检测选择。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/qq/")]
#[serde(rename_all = "camelCase")]
pub struct SystemDependency {
    /// 通用名（人类可读的语义标识）。
    pub canonical_name: String,
    /// Debian/Ubuntu 包名。
    pub debian_package: String,
    /// RHEL/CentOS/Fedora 包名。
    pub rhel_package: String,
    /// 是否支持 t64 变体（仅 Debian/Ubuntu）。
    pub has_t64_variant: bool,
    /// 依赖分类。
    pub category: DependencyCategory,
    /// 人类可读描述。
    pub description: String,
}

/// 发行版信息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/qq/")]
#[serde(rename_all = "camelCase")]
pub struct DistroInfo {
    pub family: DistroFamily,
    pub name: String,
    pub version: String,
}

/// 发行版族。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/qq/")]
#[serde(rename_all = "snake_case")]
pub enum DistroFamily {
    Debian,
    Rhel,
    Arch,
    Unknown,
}

/// 包状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/qq/")]
#[serde(rename_all = "camelCase")]
pub struct PackageStatus {
    pub name: String,
    pub installed_version: Option<String>,
    pub detection_method: DetectionMethod,
}

/// 检测方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/qq/")]
#[serde(rename_all = "snake_case")]
pub enum DetectionMethod {
    /// ldd 检测动态库
    Ldd,
    /// 包管理器查询
    PackageManager,
    /// 二进制文件存在性
    Binary,
}

/// QQ 依赖检测报告。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/qq/")]
#[serde(rename_all = "camelCase")]
pub struct QqDependencyReport {
    /// 已满足的包
    pub satisfied: Vec<PackageStatus>,
    /// 缺失的包
    pub missing: Vec<PackageStatus>,
    /// 发行版信息
    pub distro_info: DistroInfo,
    /// 用户可执行的安装命令（用于复制粘贴）
    pub install_command: Option<String>,
}

/// 依赖安装结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/qq/")]
#[serde(rename_all = "camelCase")]
pub struct InstallDependenciesResult {
    pub success: bool,
    /// 成功安装的包
    pub installed: Vec<String>,
    /// 安装失败的包
    pub failed: Vec<FailedPackage>,
    /// 是否需要用户提供 sudo 密码
    pub elevation_required: bool,
}

/// 失败的包详情。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/qq/")]
#[serde(rename_all = "camelCase")]
pub struct FailedPackage {
    pub name: String,
    pub reason: String,
}

/// 依赖安装错误（前端用于决策 UI 交互）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/qq/")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DependencyInstallError {
    /// 权限不足（sudo 失败 / 密码错误）
    PermissionDenied {
        /// 是否可以重试输入密码
        can_retry_password: bool,
        detail: String,
    },
    /// 网络问题（仓库不可达 / 超时）
    NetworkFailure {
        /// 是否建议用户切换镜像源
        suggest_mirror: bool,
        detail: String,
    },
    /// 包管理器锁定
    PackageManagerLocked {
        /// 锁定者进程名（如 "unattended-upgrades"）
        locked_by: Option<String>,
        /// 建议等待时间（秒）
        suggest_wait_seconds: u32,
    },
    /// 磁盘空间不足
    DiskSpaceFull {
        required_mb: u32,
        available_mb: u32,
    },
    /// 包不存在（仓库中找不到）
    PackageNotFound {
        package_name: String,
        /// 是否需要先刷新包索引
        suggest_refresh: bool,
    },
    /// 其他错误
    Other { message: String },
}
