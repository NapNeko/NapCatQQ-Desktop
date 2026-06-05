//! App 级设置读写命令。
//!
//! 薄壳层：组合 ConfigStore（非敏感偏好，落 app-settings.json）+ SecretStore
//! （GitHub PAT，走 keyring 不明文落盘）。设置页一次 get / 一次 set，前端不需要
//! 关心两套存储的差异——DTO 把 PAT 当普通字段，command 层负责拆分落盘。
//!
//! 路径权威性：app-settings.json 落在 `LocalConfigStore::config_dir()`，即
//! `<data_root>/runtime/config/`，与 config.json / bot.json 同级，不另起数据根。

use ncd_runtime::{
    AppSettings, AppSettingsDto, ConfigStore, LocalConfigStore, SecretStore, SecretStoreImpl,
};
use tauri::State;

use crate::AppState;

/// GitHub PAT 在 SecretStore 里的 key。与 SSH 凭证的 `ssh:{id}` 命名风格一致。
const GITHUB_PAT_SECRET_KEY: &str = "app:github_pat";

/// app-settings.json 在 config_dir 下的文件名。
const APP_SETTINGS_FILE: &str = "app-settings.json";

fn config_store(state: &AppState) -> LocalConfigStore {
    LocalConfigStore::new(&state.data_root)
}

fn secret_store(state: &AppState) -> SecretStoreImpl {
    SecretStoreImpl::new(state.data_root.join("secrets"))
}

/// 读取 App 设置。
///
/// app-settings.json 不存在（旧用户首次进设置页）时返回 `AppSettings::default()`，
/// 不报错。PAT 读 SecretStore，keyring 不可用 / 未设置时回落空串。
#[tauri::command]
pub async fn get_app_settings(state: State<'_, AppState>) -> Result<AppSettingsDto, String> {
    let store = config_store(&state);
    let path = store.config_dir().join(APP_SETTINGS_FILE);

    let settings = load_app_settings_from(&store, &path);

    let github_pat = secret_store(&state)
        .get(GITHUB_PAT_SECRET_KEY)
        .ok()
        .flatten()
        .unwrap_or_default();

    Ok(AppSettingsDto {
        settings,
        github_pat,
    })
}

/// 写入 App 设置。
///
/// 非敏感偏好原子写入 app-settings.json；PAT 非空写 SecretStore，空串则删除。
/// 同时把轮询设置热更新到内存中的 BotManager，让正在运行的 Poller 下次 tick
/// 用新间隔（无需重启）。
#[tauri::command]
pub async fn set_app_settings(
    state: State<'_, AppState>,
    dto: AppSettingsDto,
) -> Result<(), String> {
    let store = config_store(&state);
    let path = store.config_dir().join(APP_SETTINGS_FILE);

    let payload = serde_json::to_value(&dto.settings)
        .map_err(|e| format!("序列化 app 设置失败: {e}"))?;
    store
        .write_json_atomic(&path, &payload)
        .map_err(|e| format!("写入 app-settings.json 失败: {e}"))?;

    // PAT：非空写 keyring，空串清除。删除失败（本就没有）忽略。
    let secrets = secret_store(&state);
    let pat = dto.github_pat.trim();
    if pat.is_empty() {
        let _ = secrets.delete(GITHUB_PAT_SECRET_KEY);
    } else {
        secrets
            .put(GITHUB_PAT_SECRET_KEY, pat)
            .map_err(|e| format!("保存 GitHub PAT 失败: {e}"))?;
    }

    // 热更新内存中的轮询设置，运行中的 Poller 下次 tick 生效。
    state
        .bot_manager
        .update_poller_settings(dto.settings.poller.clone())
        .await;

    Ok(())
}

/// 启动期把磁盘上的 closeAction 同步给前端（localStorage 偏好）。
#[tauri::command]
pub fn sync_close_action_preference(
    _app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let store = config_store(&state);
    let path = store.config_dir().join(APP_SETTINGS_FILE);
    let settings = load_app_settings_from(&store, &path);
    Ok(settings.close_action)
}

/// 从磁盘加载 AppSettings；文件缺失或解析失败一律回落 Default，不抛错。
/// 供 command 与启动期共用（启动期通过 `read_app_settings` 包装）。
fn load_app_settings_from(store: &LocalConfigStore, path: &std::path::Path) -> AppSettings {
    match store.read_json(path) {
        Ok(value) => serde_json::from_value(value).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

/// 启动期读取 AppSettings：给 lib.rs 在构造 BotManager 前加载磁盘值用。
/// 与 `get_app_settings` 共用同一份回落语义。
pub fn read_app_settings(data_root: &std::path::Path) -> AppSettings {
    let store = LocalConfigStore::new(data_root);
    let path = store.config_dir().join(APP_SETTINGS_FILE);
    load_app_settings_from(&store, &path)
}

/// 读取已保存的 GitHub PAT（SecretStore），未设置 / keyring 不可用时回 None。
/// 给 release fetcher 拉 GitHub API 时带认证头用，复用同一 secret key。
pub fn read_github_pat(data_root: &std::path::Path) -> Option<String> {
    SecretStoreImpl::new(data_root.join("secrets"))
        .get(GITHUB_PAT_SECRET_KEY)
        .ok()
        .flatten()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
