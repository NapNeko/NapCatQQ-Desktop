// 两个原生窗口测试共用的离线聊天数据与 IPC。
use ncd_domain::chat_archive::ChatArchive;
use ncd_domain::chat_desktop::*;
use ncd_domain::onebot_debug::{DebugHost, DebugReceiverState, DebugTarget};
use ncd_domain::{AppSettings, AppSettingsDto};
use std::{
    collections::BTreeMap,
    sync::{Mutex, atomic::AtomicBool},
};
use tauri::State;

fn target() -> DebugTarget {
    DebugTarget {
        bot_id: "smoke-account".into(),
        name: "窗口测试账号".into(),
        qq_id: 10001,
        backend: ncd_domain::BackendType::NapCat,
        host: DebugHost::Local,
        running: false,
        online: Some(false),
    }
}
fn view() -> ChatViewState {
    ChatViewState {
        v: 1,
        revision: 0,
        selected_bot: Some("smoke-account".into()),
        accounts: vec![ChatAccountView {
            bot_id: "smoke-account".into(),
            self_id: "10001".into(),
            active: Some("private:22".into()),
            drafts: BTreeMap::new(),
            scroll: BTreeMap::new(),
            reading: BTreeMap::new(),
        }],
    }
}
#[derive(Default)]
pub(crate) struct Fixture {
    view: Mutex<Option<ChatViewState>>,
    preference: Mutex<ChatAccountPreference>,
    pub(crate) finished: AtomicBool,
}
#[tauri::command]
pub(crate) fn get_app_settings() -> AppSettingsDto {
    AppSettingsDto {
        settings: AppSettings::default(),
        github_pat: String::new(),
    }
}
#[tauri::command]
pub(crate) fn chat_targets() -> Vec<DebugTarget> {
    vec![target()]
}
#[tauri::command]
pub(crate) fn chat_desktop_status(state: State<'_, Fixture>) -> ChatDesktopStatus {
    let mut preference = state.preference.lock().unwrap().clone();
    preference.bot_id = "smoke-account".into();
    preference.self_id = "10001".into();
    ChatDesktopStatus {
        v: 1,
        accounts: vec![ChatAccountStatus {
            target: target(),
            preference,
            unread: 0,
            notification_unread: 0,
            groups: vec![],
            connection: DebugReceiverState::Stopped {
                reason: "测试账号离线".into(),
            },
            error: None,
        }],
    }
}
#[tauri::command]
pub(crate) fn chat_set_preference(
    state: State<'_, Fixture>,
    preference: ChatAccountPreference,
) -> Result<(), String> {
    preference.validate().map_err(str::to_owned)?;
    *state.preference.lock().unwrap() = preference;
    Ok(())
}
#[tauri::command]
pub(crate) fn chat_view_load(state: State<'_, Fixture>) -> ChatViewState {
    state.view.lock().unwrap().clone().unwrap_or_else(view)
}
#[tauri::command]
pub(crate) fn chat_view_save(state: State<'_, Fixture>, view: ChatViewState) -> Result<(), String> {
    view.validate().map_err(str::to_owned)?;
    *state.view.lock().unwrap() = Some(view);
    Ok(())
}
#[tauri::command]
pub(crate) fn chat_archive_load() -> ChatArchive {
    serde_json::from_value(serde_json::json!({"v":1,"selfId":"10001","conversations":[{"key":"private:22","type":"private","id":"22","name":"测试联系人","unread":0,"pinned":false,"lastAt":1000,"preview":"窗口资源测试","boxed":false}],"messages":[{"key":"private:22/1","session":"private:22","id":"1","senderId":"22","senderName":"测试联系人","at":1000,"mine":false,"segments":[{"type":"text","data":{"text":"窗口资源测试；没有连接真实账号。"}}],"status":"sent"}]})).unwrap()
}
#[tauri::command]
pub(crate) fn chat_archive_save() {}
#[tauri::command]
pub(crate) fn chat_set_reading() {}
#[tauri::command]
pub(crate) fn chat_release_account() {}
#[tauri::command]
pub(crate) fn chat_flush() {}
#[tauri::command]
pub(crate) fn reveal_chat_window() {} // 保持隐藏，不抢用户焦点。
