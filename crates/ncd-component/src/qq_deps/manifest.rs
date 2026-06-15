//! QQ Linux 运行时依赖清单定义。

use ncd_domain::{DependencyCategory, SystemDependency};
use std::collections::HashMap;

/// QQ Linux 运行时完整依赖清单。
pub struct QQDependencyManifest {
    pub schema_version: u32,
    pub for_qq_version: String,
    pub dependencies: Vec<SystemDependency>,
}

impl QQDependencyManifest {
    pub fn group_by_category(&self) -> HashMap<DependencyCategory, Vec<&SystemDependency>> {
        let mut groups: HashMap<DependencyCategory, Vec<&SystemDependency>> = HashMap::new();
        for dep in &self.dependencies {
            groups.entry(dep.category).or_default().push(dep);
        }
        groups
    }
}

/// 获取 QQ 3.2.25 的官方依赖清单。
pub fn qq_qqnt_dependencies_v3_2_25() -> QQDependencyManifest {
    QQDependencyManifest {
        schema_version: 1,
        for_qq_version: "3.2.25-45758".to_string(),
        dependencies: vec![
            // Runtime 分类 - 第一部分
            SystemDependency {
                canonical_name: "nss".to_string(),
                debian_package: "libnss3".to_string(),
                rhel_package: "nss".to_string(),
                has_t64_variant: false,
                category: DependencyCategory::Runtime,
                description: "Network Security Services".to_string(),
            },
            SystemDependency {
                canonical_name: "gbm".to_string(),
                debian_package: "libgbm1".to_string(),
                rhel_package: "mesa-libgbm".to_string(),
                has_t64_variant: false,
                category: DependencyCategory::Runtime,
                description: "Generic Buffer Management".to_string(),
            },
            SystemDependency {
                canonical_name: "glib".to_string(),
                debian_package: "libglib2.0-0".to_string(),
                rhel_package: "glib2".to_string(),
                has_t64_variant: true,
                category: DependencyCategory::Runtime,
                description: "GLib runtime library".to_string(),
            },
            SystemDependency {
                canonical_name: "atk".to_string(),
                debian_package: "libatk1.0-0".to_string(),
                rhel_package: "atk".to_string(),
                has_t64_variant: true,
                category: DependencyCategory::Runtime,
                description: "Accessibility Toolkit".to_string(),
            },
            SystemDependency {
                canonical_name: "atspi".to_string(),
                debian_package: "libatspi2.0-0".to_string(),
                rhel_package: "at-spi2-atk".to_string(),
                has_t64_variant: true,
                category: DependencyCategory::Runtime,
                description: "Assistive Technology SPI".to_string(),
            },
            SystemDependency {
                canonical_name: "gtk3".to_string(),
                debian_package: "libgtk-3-0".to_string(),
                rhel_package: "gtk3".to_string(),
                has_t64_variant: true,
                category: DependencyCategory::Runtime,
                description: "GTK+ 3 toolkit".to_string(),
            },
            SystemDependency {
                canonical_name: "alsa".to_string(),
                debian_package: "libasound2".to_string(),
                rhel_package: "alsa-lib".to_string(),
                has_t64_variant: true,
                category: DependencyCategory::Runtime,
                description: "ALSA sound library".to_string(),
            },
            SystemDependency {
                canonical_name: "xvfb".to_string(),
                debian_package: "xvfb".to_string(),
                rhel_package: "xorg-x11-server-Xvfb".to_string(),
                has_t64_variant: false,
                category: DependencyCategory::Graphics,
                description: "X virtual framebuffer".to_string(),
            },
            SystemDependency {
                canonical_name: "xauth".to_string(),
                debian_package: "xauth".to_string(),
                rhel_package: "xorg-x11-xauth".to_string(),
                has_t64_variant: false,
                category: DependencyCategory::Graphics,
                description: "X authority utilities".to_string(),
            },
            SystemDependency {
                canonical_name: "curl".to_string(),
                debian_package: "curl".to_string(),
                rhel_package: "curl".to_string(),
                has_t64_variant: false,
                category: DependencyCategory::Toolchain,
                description: "HTTP client".to_string(),
            },
            SystemDependency {
                canonical_name: "jq".to_string(),
                debian_package: "jq".to_string(),
                rhel_package: "jq".to_string(),
                has_t64_variant: false,
                category: DependencyCategory::Toolchain,
                description: "JSON processor".to_string(),
            },
            SystemDependency {
                canonical_name: "cpio".to_string(),
                debian_package: "cpio".to_string(),
                rhel_package: "cpio".to_string(),
                has_t64_variant: false,
                category: DependencyCategory::Toolchain,
                description: "CPIO archiver".to_string(),
            }
        ],
    }
}
