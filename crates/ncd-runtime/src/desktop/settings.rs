//! app-settings.json 的写入口。设置页保存和组件装卸回写 SnowLuma 包类型都走这里：
//! 内存副本和文件在同一把写锁里一起改，谁也不会拿着旧副本把对方刚写的值盖回去。

use std::path::Path;

use ncd_config::APP_SETTINGS_FILE;
use ncd_domain::AppSettings;
use ncd_traits::ConfigStore;
use tokio::sync::RwLock;

use crate::components::infer_local_snowluma_package;
use crate::config_store_impl::LocalConfigStore;

fn write_app_settings_file(data_root: &Path, settings: &AppSettings) -> Result<(), String> {
    let store = LocalConfigStore::new(data_root);
    let path = store.config_dir().join(APP_SETTINGS_FILE);
    let payload =
        serde_json::to_value(settings).map_err(|e| format!("序列化 app 设置失败: {e}"))?;
    store
        .write_json_atomic(&path, &payload)
        .map_err(|e| format!("写入 app-settings.json 失败: {e}"))
}

/// 盘上那份;文件缺失或解析不了是 None
pub fn read_app_settings_file(data_root: &Path) -> Option<AppSettings> {
    let store = LocalConfigStore::new(data_root);
    let value = store
        .read_json(&store.config_dir().join(APP_SETTINGS_FILE))
        .ok()?;
    serde_json::from_value(value).ok()
}

/// 启动和设置页读取用;没有文件(旧用户首次进设置页)或解析不了一律当默认值
pub fn load_app_settings(data_root: &Path) -> AppSettings {
    read_app_settings_file(data_root).unwrap_or_default()
}

/// 整份替换 app-settings.json 的写法(配置导入)走这里:`write` 在写锁里跑，
/// 写完把内存换成盘上的新值，中间插不进一次拿旧副本回写的 update_app_settings。
/// 本机 SnowLuma 包类型记的是这台机器装了什么，保留内存那份并写回文件
pub async fn replace_app_settings_with<R>(
    data_root: &Path,
    current: &RwLock<AppSettings>,
    write: impl FnOnce() -> Result<R, String>,
) -> Result<R, String> {
    let mut guard = current.write().await;
    let out = write()?;
    let mut next = load_app_settings(data_root);
    if next.snowluma_package != guard.snowluma_package {
        next.snowluma_package = guard.snowluma_package;
        // 导入本身已经落盘，这里失败只让下次保存再对齐，不回头判导入失败
        if let Err(err) = write_app_settings_file(data_root, &next) {
            tracing::warn!(error = %err, "failed to keep local SnowLuma package after settings replace");
        }
    }
    *guard = next;
    Ok(out)
}

/// 在当前设置上改一处再落盘。写锁一直拿到文件写完，内存只在落盘成功后才换成新值
pub async fn update_app_settings<R>(
    data_root: &Path,
    current: &RwLock<AppSettings>,
    edit: impl FnOnce(&mut AppSettings) -> R,
) -> Result<(AppSettings, R), String> {
    let mut guard = current.write().await;
    let mut next = guard.clone();
    let out = edit(&mut next);
    write_app_settings_file(data_root, &next)?;
    *guard = next.clone();
    Ok((next, out))
}

/// 老版本装过本机 SnowLuma 却没记包类型的，启动时按目录里有没有 node.exe 补记一次。
/// 放在启动时做而不是读设置页时做：读的时候顺手写盘，内存那份没跟上，下次保存又把它盖回 None
pub fn backfill_snowluma_package(data_root: &Path, settings: &mut AppSettings) {
    if settings.snowluma_package.is_some() {
        return;
    }
    if !data_root.join("components").join("SnowLuma").is_dir() {
        return;
    }
    settings.snowluma_package = Some(infer_local_snowluma_package(data_root));
    if let Err(err) = write_app_settings_file(data_root, settings) {
        tracing::warn!(error = %err, "failed to persist backfilled SnowLuma package");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_domain::SnowLumaLinuxPackage;
    use std::sync::Arc;

    fn read_back(data_root: &Path) -> AppSettings {
        let store = LocalConfigStore::new(data_root);
        let value = store
            .read_json(&store.config_dir().join(APP_SETTINGS_FILE))
            .unwrap();
        serde_json::from_value(value).unwrap()
    }

    #[tokio::test]
    async fn concurrent_updates_keep_both_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let current = Arc::new(RwLock::new(AppSettings::default()));
        let a = {
            let current = Arc::clone(&current);
            let root = tmp.path().to_path_buf();
            tokio::spawn(async move {
                update_app_settings(&root, &current, |s| {
                    s.snowluma_package = Some(SnowLumaLinuxPackage::Lite);
                })
                .await
                .unwrap();
            })
        };
        let b = {
            let current = Arc::clone(&current);
            let root = tmp.path().to_path_buf();
            tokio::spawn(async move {
                update_app_settings(&root, &current, |s| s.launch_on_startup = true)
                    .await
                    .unwrap();
            })
        };
        a.await.unwrap();
        b.await.unwrap();

        let on_disk = read_back(tmp.path());
        assert_eq!(on_disk.snowluma_package, Some(SnowLumaLinuxPackage::Lite));
        assert!(on_disk.launch_on_startup);
        assert_eq!(*current.read().await, on_disk);
    }

    #[tokio::test]
    async fn replace_picks_up_file_but_keeps_local_package() {
        let tmp = tempfile::tempdir().unwrap();
        let current = RwLock::new(AppSettings {
            snowluma_package: Some(SnowLumaLinuxPackage::Lite),
            ..AppSettings::default()
        });
        let imported = AppSettings {
            launch_on_startup: true,
            snowluma_package: Some(SnowLumaLinuxPackage::Full),
            ..AppSettings::default()
        };

        replace_app_settings_with(tmp.path(), &current, || {
            write_app_settings_file(tmp.path(), &imported)
        })
        .await
        .unwrap();

        let now = current.read().await.clone();
        assert!(now.launch_on_startup);
        assert_eq!(now.snowluma_package, Some(SnowLumaLinuxPackage::Lite));
        assert_eq!(read_back(tmp.path()), now);

        // 之后的单项修改从导入后的值出发，不会把导入盖回去
        update_app_settings(tmp.path(), &current, |s| s.snowluma_package = None)
            .await
            .unwrap();
        assert!(read_back(tmp.path()).launch_on_startup);
    }

    #[test]
    fn backfill_only_when_snowluma_dir_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let mut settings = AppSettings::default();
        backfill_snowluma_package(tmp.path(), &mut settings);
        assert_eq!(settings.snowluma_package, None);

        std::fs::create_dir_all(tmp.path().join("components/SnowLuma")).unwrap();
        backfill_snowluma_package(tmp.path(), &mut settings);
        assert_eq!(settings.snowluma_package, Some(SnowLumaLinuxPackage::Lite));
        assert_eq!(
            read_back(tmp.path()).snowluma_package,
            Some(SnowLumaLinuxPackage::Lite)
        );
    }
}
