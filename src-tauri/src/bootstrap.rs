use std::path::{Path, PathBuf};

use ncd_runtime::{
    BootstrapSnapshot, LocalConfigStore, LocalPathProbe, LocalVersionSnapshot,
    MigrationOrchestrator, SecretStoreImpl,
};

const APP_DATA_DIR_NAME: &str = "NapCatQQ Desktop";

pub(crate) fn resolve_data_root() -> PathBuf {
    #[cfg(windows)]
    let program_data = std::env::var_os("ProgramData").map(PathBuf::from);
    #[cfg(not(windows))]
    let program_data = None;
    let local_data = dirs::data_local_dir();
    resolve_data_root_from_candidates(program_data, local_data)
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
    let store = LocalConfigStore::new(data_root);
    let probe = LocalPathProbe::new();
    let secrets = SecretStoreImpl::new(data_root.join("secrets"));

    let mut snapshot = MigrationOrchestrator::new(&store, &probe, &secrets).bootstrap();
    snapshot.data_root = data_root.to_string_lossy().into_owned();
    snapshot.local_versions = detect_local_versions(data_root);
    snapshot
}

/// 探测 data_root 下已安装的 NapCat / SnowLuma 版本号。任何错误（IO /
/// 解析）一律返回 None：UI 把 None 显示为"未安装"，不需要把错误细节
/// 暴露给用户。
fn detect_local_versions(data_root: &Path) -> LocalVersionSnapshot {
    LocalVersionSnapshot {
        napcat: detect_napcat_version(data_root),
        snowluma: detect_snowluma_version(data_root),
    }
}

/// 复用 ncd_component::napcat::parse_napcat_version，从
/// <data_root>/runtime/NapCatQQ/napcat.mjs grep 版本号。
fn detect_napcat_version(data_root: &Path) -> Option<String> {
    let mjs_path = data_root
        .join("runtime")
        .join("NapCatQQ")
        .join("napcat.mjs");
    let content = std::fs::read_to_string(&mjs_path).ok()?;
    ncd_component::napcat::parse_napcat_version(&content)
}

/// 从 SnowLuma daemon 安装根的 package.json 读 version 字段。
/// 路径与 lib.rs::run 中 SnowLuma daemon 的安装根保持一致：
/// <data_root>/runtime/SnowLuma/package.json。
fn detect_snowluma_version(data_root: &Path) -> Option<String> {
    let pkg_path = data_root
        .join("runtime")
        .join("SnowLuma")
        .join("package.json");
    let content = std::fs::read_to_string(&pkg_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json.get("version")?.as_str().map(|s| s.to_string())
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

    /// data_root 字段必须装到 BootstrapSnapshot 上：UI StatusBar 直接消费
    /// 这个字符串，是 Home 页 v1 的强契约。
    #[test]
    fn data_root_field_is_populated() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let snapshot = build_snapshot_for_data_root(temp.path());

        assert_eq!(snapshot.data_root, temp.path().to_string_lossy());
    }

    /// 没装过 NapCat / SnowLuma 时 local_versions 全为 None；UI 显示
    /// "未安装"，不抛错。
    #[test]
    fn local_versions_default_when_files_missing() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let snapshot = build_snapshot_for_data_root(temp.path());

        assert_eq!(snapshot.local_versions.napcat, None);
        assert_eq!(snapshot.local_versions.snowluma, None);
    }

    /// 装好 NapCat（有 napcat.mjs）时应当解析出版本号；锁定本地版本探测
    /// 与 ncd_component::napcat::parse_napcat_version 是同一条链路。
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

    /// SnowLuma 路径下有合法 package.json 时应当解析 version 字段。
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

    /// SnowLuma package.json 损坏（非合法 JSON）应当回落到 None，不 panic。
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
}
