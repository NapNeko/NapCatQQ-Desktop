use std::path::{Path, PathBuf};

use ncd_domain::{BootstrapSnapshot, DataLayoutConsolidateSnapshot, LocalVersionSnapshot};
use ncd_runtime::{LocalConfigStore, LocalPathProbe, MigrationOrchestrator, SecretStoreImpl};

use crate::product_registry;

const APP_DATA_DIR_NAME: &str = "NapCatQQ Desktop";

/// 解析权威数据根(单一入口;业务模块禁止再硬编码 ProgramData)。
///
/// 优先级:
/// 1. 环境变量 `NCD_DATA_ROOT`(开发/排障覆盖)
/// 2. Windows `HKCU\SOFTWARE\NapCatQQ-Desktop\DataRoot`(用户迁移,无 UAC)
/// 3. Windows `HKLM\SOFTWARE\NapCatQQ-Desktop\DataRoot`(MSI 默认 / 启动补写)
/// 4. `%ProgramData%\NapCatQQ Desktop`(生产默认)
/// 5. LocalAppData / cwd 兜底(无 ProgramData 的非 Windows 或开发机)
///
/// `read_data_root` 已合并 2+3;此处只再调一次。
pub(crate) fn resolve_data_root() -> PathBuf {
    if let Some(from_env) = data_root_from_env() {
        return from_env;
    }

    #[cfg(windows)]
    if let Some(from_reg) = product_registry::read_data_root() {
        return from_reg;
    }

    #[cfg(windows)]
    let program_data = std::env::var_os("ProgramData").map(PathBuf::from);
    #[cfg(not(windows))]
    let program_data = None;
    let local_data = dirs::data_local_dir();
    resolve_data_root_from_candidates(program_data, local_data)
}

fn data_root_from_env() -> Option<PathBuf> {
    let raw = std::env::var_os(product_registry::DATA_ROOT_ENV)?;
    if raw.is_empty() {
        return None;
    }
    let path = product_registry::normalize_registered_path(PathBuf::from(raw));
    product_registry::is_usable_absolute_path(&path).then_some(path)
}

pub(crate) fn resolve_data_root_from_candidates(
    program_data: Option<PathBuf>,
    local_data: Option<PathBuf>,
) -> PathBuf {
    if let Some(program_data) = program_data {
        return program_data.join(APP_DATA_DIR_NAME);
    }

    local_data
        .unwrap_or_else(default_base_without_system_dirs)
        .join(APP_DATA_DIR_NAME)
}

fn default_base_without_system_dirs() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir())
}

pub fn build_snapshot() -> BootstrapSnapshot {
    let data_root = resolve_data_root();
    build_snapshot_for_data_root(&data_root)
}

pub fn build_snapshot_for_data_root(data_root: &Path) -> BootstrapSnapshot {
    // 先收敛到布局 v1,再跑 schema 迁移;失败只记日志,不阻断启动(保留原目录)
    let layout_consolidate = match ncd_runtime::consolidate_data_root(data_root) {
        Ok(report) => {
            if report.performed {
                tracing::info!(
                    target: "ncd_tauri::bootstrap",
                    backup = ?report.backup_path.as_ref().map(|p| p.display().to_string()),
                    moved = report.moved.len(),
                    "data_root 布局收敛完成"
                );
            }
            Some(DataLayoutConsolidateSnapshot {
                performed: report.performed,
                skipped_reason: report.skipped_reason,
                backup_path: report.backup_path.map(|p| p.to_string_lossy().into_owned()),
                moved_count: report.moved.len() as u32,
                warnings: report.warnings,
                error: None,
            })
        }
        Err(err) => {
            tracing::warn!(
                target: "ncd_tauri::bootstrap",
                err = %err,
                "data_root 布局收敛失败,继续使用现有目录"
            );
            Some(DataLayoutConsolidateSnapshot {
                performed: false,
                skipped_reason: None,
                backup_path: None,
                moved_count: 0,
                warnings: Vec::new(),
                error: Some(err.to_string()),
            })
        }
    };

    let store = LocalConfigStore::new(data_root);
    let probe = LocalPathProbe::new();
    let secrets = SecretStoreImpl::new(data_root.join("secrets"));

    let mut snapshot = MigrationOrchestrator::new(&store, &probe, &secrets).bootstrap();
    snapshot.data_root = data_root.to_string_lossy().into_owned();
    snapshot.local_versions = detect_local_versions(data_root);
    snapshot.layout_consolidate = layout_consolidate;
    snapshot
}

/// 探测 data_root 下已安装的 NapCat / SnowLuma 版本号任何错误(IO /
/// 解析)一律返回 None:UI 把 None 显示为"未安装",不需要把错误细节
/// 暴露给用户
fn detect_local_versions(data_root: &Path) -> LocalVersionSnapshot {
    LocalVersionSnapshot {
        napcat: detect_napcat_version(data_root),
        snowluma: detect_snowluma_version(data_root),
    }
}

/// 复用 ncd_component::napcat::parse_napcat_version,从
/// <data_root>/components/NapCatQQ/napcat.mjs grep 版本号(兼容旧 runtime/NapCatQQ)
fn detect_napcat_version(data_root: &Path) -> Option<String> {
    let paths = ncd_runtime::DataPaths::new(data_root);
    for mjs_path in [
        paths.napcat_install_dir().join("napcat.mjs"),
        paths.legacy_napcat_install_dir().join("napcat.mjs"),
    ] {
        if let Ok(content) = std::fs::read_to_string(&mjs_path) {
            if let Some(v) = ncd_component::napcat::parse_napcat_version(&content) {
                return Some(v);
            }
        }
    }
    None
}

/// 从 SnowLuma daemon 安装根的 package.json 读 version 字段
fn detect_snowluma_version(data_root: &Path) -> Option<String> {
    let paths = ncd_runtime::DataPaths::new(data_root);
    for pkg_path in [
        paths.snowluma_install_dir().join("package.json"),
        paths.legacy_snowluma_install_dir().join("package.json"),
    ] {
        if let Ok(content) = std::fs::read_to_string(&pkg_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(v) = json.get("version").and_then(|x| x.as_str()) {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }

    #[test]
    fn data_root_prefers_programdata_primary_name() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let program_data = temp.path().join("ProgramData");
        let local_data = temp.path().join("LocalData");
        touch(
            &local_data
                .join("NapCatQQ-Desktop")
                .join("runtime")
                .join("config")
                .join("bot.json"),
        );

        let resolved =
            resolve_data_root_from_candidates(Some(program_data.clone()), Some(local_data));

        assert_eq!(resolved, program_data.join(APP_DATA_DIR_NAME));
    }

    #[test]
    fn data_root_uses_primary_programdata_when_legacy_exists() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let program_data = temp.path().join("ProgramData");
        let legacy = program_data.join("NapCatQQ-Desktop");
        std::fs::create_dir_all(&legacy).unwrap();

        let resolved = resolve_data_root_from_candidates(
            Some(program_data.clone()),
            Some(temp.path().join("LocalData")),
        );

        assert_eq!(resolved, program_data.join(APP_DATA_DIR_NAME));
    }

    #[test]
    fn data_root_prefers_programdata_primary_with_napcat_runtime() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let program_data = temp.path().join("ProgramData");
        let primary = program_data.join(APP_DATA_DIR_NAME);
        let legacy = program_data.join("NapCatQQ-Desktop");
        touch(
            &primary
                .join("runtime")
                .join("NapCatQQ")
                .join("NapCatWinBootMain.exe"),
        );
        std::fs::create_dir_all(&legacy).unwrap();

        let resolved = resolve_data_root_from_candidates(
            Some(program_data.clone()),
            Some(temp.path().join("LocalData")),
        );

        assert_eq!(resolved, primary);
    }

    #[test]
    fn data_root_uses_primary_programdata_even_when_legacy_has_runtime() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let program_data = temp.path().join("ProgramData");
        let primary = program_data.join(APP_DATA_DIR_NAME);
        let legacy = program_data.join("NapCatQQ-Desktop");
        std::fs::create_dir_all(&primary).unwrap();
        touch(
            &legacy
                .join("runtime")
                .join("NapCatQQ")
                .join("NapCatWinBootMain.exe"),
        );

        let resolved = resolve_data_root_from_candidates(
            Some(program_data.clone()),
            Some(temp.path().join("LocalData")),
        );

        assert_eq!(resolved, primary);
    }

    #[test]
    fn data_root_prefers_existing_programdata_primary_over_legacy_config() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let program_data = temp.path().join("ProgramData");
        let primary = program_data.join(APP_DATA_DIR_NAME);
        let legacy = program_data.join("NapCatQQ-Desktop");
        std::fs::create_dir_all(&primary).unwrap();
        touch(&legacy.join("runtime").join("config").join("bot.json"));

        let resolved = resolve_data_root_from_candidates(
            Some(program_data.clone()),
            Some(temp.path().join("LocalData")),
        );

        assert_eq!(resolved, primary);
    }

    #[test]
    fn data_root_ignores_local_data_when_programdata_exists() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let program_data = temp.path().join("ProgramData");
        let local_data = temp.path().join("LocalData");
        touch(
            &local_data
                .join(APP_DATA_DIR_NAME)
                .join("runtime")
                .join("NapCatQQ")
                .join("NapCatWinBootMain.exe"),
        );

        let resolved =
            resolve_data_root_from_candidates(Some(program_data.clone()), Some(local_data));

        assert_eq!(resolved, program_data.join(APP_DATA_DIR_NAME));
    }

    #[test]
    fn data_root_defaults_to_programdata_primary_when_candidates_are_missing() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let program_data = temp.path().join("ProgramData");

        let resolved = resolve_data_root_from_candidates(
            Some(program_data.clone()),
            Some(temp.path().join("LocalData")),
        );

        assert_eq!(resolved, program_data.join(APP_DATA_DIR_NAME));
    }

    #[test]
    fn data_root_falls_back_without_programdata() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let local_data = temp.path().join("LocalData");
        let legacy = local_data.join("NapCatQQ-Desktop");
        std::fs::create_dir_all(&legacy).unwrap();

        let resolved = resolve_data_root_from_candidates(None, Some(local_data.clone()));

        assert_eq!(resolved, local_data.join(APP_DATA_DIR_NAME));
    }

    /// data_root 字段必须装到 BootstrapSnapshot 上:UI StatusBar 直接消费
    /// 这个字符串,是 Home 页 v1 的强契约
    #[test]
    fn data_root_field_is_populated() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let snapshot = build_snapshot_for_data_root(temp.path());

        assert_eq!(snapshot.data_root, temp.path().to_string_lossy());
    }

    /// 没装过 NapCat / SnowLuma 时 local_versions 全为 None;UI 显示
    /// "未安装",不抛错
    #[test]
    fn local_versions_default_when_files_missing() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let snapshot = build_snapshot_for_data_root(temp.path());

        assert_eq!(snapshot.local_versions.napcat, None);
        assert_eq!(snapshot.local_versions.snowluma, None);
    }

    /// 装好 NapCat(有 napcat.mjs)时应当解析出版本号;锁定本地版本探测
    /// 与 ncd_component::napcat::parse_napcat_version 是同一条链路
    #[test]
    fn local_versions_parses_napcat_when_mjs_exists() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let mjs = temp
            .path()
            .join("runtime")
            .join("NapCatQQ")
            .join("napcat.mjs");
        std::fs::create_dir_all(mjs.parent().unwrap()).unwrap();
        std::fs::write(
            &mjs,
            r#"const napCatVersion = typeof (__vite_import_meta_env__) !== "undefined" && "4.18.1" || "1.0.0-dev";"#,
        )
        .unwrap();

        let snapshot = build_snapshot_for_data_root(temp.path());

        assert_eq!(snapshot.local_versions.napcat.as_deref(), Some("4.18.1"));
    }

    /// SnowLuma 路径下有合法 package.json 时应当解析 version 字段
    #[test]
    fn local_versions_parses_snowluma_when_package_json_exists() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let pkg = temp
            .path()
            .join("runtime")
            .join("SnowLuma")
            .join("package.json");
        std::fs::create_dir_all(pkg.parent().unwrap()).unwrap();
        std::fs::write(
            &pkg,
            r#"{"name":"snowluma","version":"0.3.2","main":"daemon.js"}"#,
        )
        .unwrap();

        let snapshot = build_snapshot_for_data_root(temp.path());

        assert_eq!(snapshot.local_versions.snowluma.as_deref(), Some("0.3.2"));
    }

    /// SnowLuma package.json 损坏(非合法 JSON)应当回落到 None,不 panic
    #[test]
    fn local_versions_snowluma_falls_back_on_malformed_json() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let pkg = temp
            .path()
            .join("runtime")
            .join("SnowLuma")
            .join("package.json");
        std::fs::create_dir_all(pkg.parent().unwrap()).unwrap();
        std::fs::write(&pkg, "not a json {").unwrap();

        let snapshot = build_snapshot_for_data_root(temp.path());

        assert_eq!(snapshot.local_versions.snowluma, None);
    }

    #[test]
    fn registered_path_helpers_reject_relative() {
        assert!(!product_registry::is_usable_absolute_path(Path::new(
            "relative-data"
        )));
        #[cfg(windows)]
        {
            let abs = PathBuf::from(r"C:\Custom\NapCatQQ Desktop");
            assert!(product_registry::is_usable_absolute_path(&abs));
            let norm = product_registry::normalize_registered_path(PathBuf::from(
                r"C:\Custom\NapCatQQ Desktop\",
            ));
            assert_eq!(norm, abs);
        }
    }
}
