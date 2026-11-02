use ncd_domain::onebot_debug::{
    DebugCallOutcome, DebugChannelChoice, DebugRequestDraft, DebugSavedFolder, DebugSavedRequest,
};
use serde_json::{Value, json};

use super::super::params::PARAM_STRING_LIMIT;
use super::config::MAX_TABS;
use super::files::notice;
use super::history::{HISTORY_KEEP, HISTORY_RESPONSE_LIMIT, is_credential_action};
use super::history_file::{
    READ_ONLY_HISTORY_LINE_LIMIT, READ_ONLY_HISTORY_RETAINED_LIMIT, READ_ONLY_HISTORY_SCAN_LIMIT,
};
use super::*;
use ncd_domain::bot_config::BackendType;
use ncd_domain::onebot_debug::{
    DebugCallOrigin, DebugCallRequest, DebugCallResult, DebugChannelId, DebugError,
};
use std::collections::HashSet;

fn draft(id: &str) -> DebugRequestDraft {
    DebugRequestDraft {
        id: id.to_owned(),
        action: "send_msg".to_owned(),
        params_text: "{}".to_owned(),
        timeout_ms: None,
        channel: None,
    }
}

fn entry(id: &str, action: &str, bot_id: &str, ok: bool) -> DebugHistoryEntry {
    DebugHistoryEntry {
        id: id.to_owned(),
        at_ms: 1_700_000_000_000,
        bot_id: bot_id.to_owned(),
        bot_name: format!("bot-{bot_id}"),
        backend: BackendType::NapCat,
        channel: DebugChannelId::Internal,
        origin: DebugCallOrigin::Editor,
        action: action.to_owned(),
        params: json!({"group_id": 123}),
        params_truncated: false,
        ok,
        retcode: Some(if ok { 0 } else { 1400 }),
        error: None,
        elapsed_ms: 12,
        response: Some(json!({"status": "ok"})),
        response_truncated: false,
    }
}

fn query() -> DebugHistoryQuery {
    DebugHistoryQuery {
        action: None,
        bot_id: None,
        ok: None,
        text: None,
        limit: 200,
        offset: 0,
    }
}

#[tokio::test]
async fn mcp_cold_history_read_never_compacts_or_repairs_missing_newline() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join(DIR_NAME);
    std::fs::create_dir_all(&dir).unwrap();
    let mut bytes = Vec::new();
    for index in 0..=HISTORY_COMPACT_AT {
        if index > 0 {
            bytes.push(b'\n');
        }
        bytes.extend(
            serde_json::to_vec(&entry(&index.to_string(), "get_status", "1", true)).unwrap(),
        );
    }
    let path = dir.join(HISTORY_FILE);
    std::fs::write(&path, &bytes).unwrap();
    let store = DebugStore::load_read_only(root.path()).await.unwrap();
    assert_eq!(store.history(query()).await.total as usize, HISTORY_KEEP);
    assert!(
        store
            .history_entry(&HISTORY_COMPACT_AT.to_string())
            .await
            .is_some()
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
}

#[tokio::test]
async fn mcp_cold_read_bounds_oversized_history_lines_without_repair() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join(DIR_NAME);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(HISTORY_FILE);
    let bytes = vec![b' '; READ_ONLY_HISTORY_LINE_LIMIT as usize + 1];
    std::fs::write(&path, &bytes).unwrap();
    assert!(DebugStore::load_read_only(root.path()).await.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
}

#[tokio::test]
async fn mcp_cold_history_bounds_retained_params_without_modifying_file() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join(DIR_NAME);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(HISTORY_FILE);
    let mut record = entry("large-params", "send_msg", "1", true);
    record.response = None;
    record.params = json!(vec!["a".repeat(60 * 1024); 4]);
    let mut bytes = Vec::new();
    let mut index = 0;
    while bytes.len() <= READ_ONLY_HISTORY_RETAINED_LIMIT {
        record.id = index.to_string();
        let line = serde_json::to_vec(&record).unwrap();
        assert!(line.len() < READ_ONLY_HISTORY_LINE_LIMIT as usize);
        bytes.extend(line);
        bytes.push(b'\n');
        index += 1;
    }
    assert!(bytes.len() < READ_ONLY_HISTORY_SCAN_LIMIT as usize);
    std::fs::write(&path, &bytes).unwrap();
    assert!(DebugStore::load_read_only(root.path()).await.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
}

fn over_scan_history_payload() -> Vec<u8> {
    let mut line = vec![b' '; 64 * 1024];
    *line.last_mut().unwrap() = b'\n';
    line.repeat(READ_ONLY_HISTORY_SCAN_LIMIT as usize / line.len() + 1)
}

#[tokio::test]
async fn mcp_cold_history_bounds_total_scan_without_modifying_file() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join(DIR_NAME);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(HISTORY_FILE);
    let bytes = over_scan_history_payload();
    std::fs::write(&path, &bytes).unwrap();
    assert!(DebugStore::load_read_only(root.path()).await.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
}

#[tokio::test]
async fn mcp_warm_history_detail_bounds_disk_lines_and_scan() {
    for oversized_line in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let store = DebugStore::load_with_budget(root.path(), 1).await;
        let record = entry("cached", "get_status", "1", true);
        store.append_history(record.clone()).await.unwrap();
        assert_eq!(
            store.history_entry_read_only("cached").await.unwrap(),
            Some(record.clone())
        );
        let path = root.path().join(DIR_NAME).join(HISTORY_FILE);
        let mut bytes = if oversized_line {
            vec![b' '; READ_ONLY_HISTORY_LINE_LIMIT as usize + 1]
        } else {
            over_scan_history_payload()
        };
        bytes.push(b'\n');
        bytes.extend(serde_json::to_vec(&record).unwrap());
        bytes.push(b'\n');
        std::fs::write(&path, &bytes).unwrap();
        assert!(store.history_entry_read_only("cached").await.is_err());
        assert_eq!(store.history(query()).await.total, 1);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(
            std::fs::read_dir(root.path().join(DIR_NAME))
                .unwrap()
                .count(),
            1
        );
    }
}

fn folder(id: &str, name: &str, order: i32) -> DebugSavedFolder {
    DebugSavedFolder {
        id: id.to_owned(),
        name: name.to_owned(),
        order,
    }
}

fn saved(id: &str, folder_id: Option<&str>, order: i32) -> DebugSavedRequest {
    DebugSavedRequest {
        id: id.to_owned(),
        name: format!("req-{id}"),
        folder_id: folder_id.map(str::to_owned),
        action: "get_login_info".to_owned(),
        params: json!({}),
        channel: None,
        note: None,
        order,
        created_at_ms: 1,
        updated_at_ms: 2,
    }
}

fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn debug_dir(root: &Path) -> PathBuf {
    root.join(DIR_NAME)
}

fn outcome(ok: bool, raw: Value, truncated: bool) -> DebugCallOutcome {
    DebugCallOutcome {
        ok,
        status: if ok { "ok" } else { "failed" }.to_owned(),
        retcode: if ok { 0 } else { 1400 },
        data: Value::Null,
        message: String::new(),
        wording: String::new(),
        raw,
        elapsed_ms: 34,
        channel: DebugChannelId::Internal,
        size_bytes: 10,
        truncated,
    }
}

fn request(origin: DebugCallOrigin) -> DebugCallRequest {
    DebugCallRequest {
        request_id: "r1".to_owned(),
        bot_id: "1".to_owned(),
        channel: DebugChannelId::Auto,
        action: "send_msg".to_owned(),
        params: json!({"message": "hi"}),
        timeout_ms: None,
        origin,
    }
}

// --- 往返 ---------------------------------------------------------------

#[tokio::test]
async fn imported_debug_config_survives_the_next_save_without_changing_history() {
    use ncd_traits::{ConfigStore, JsonTransaction};
    let root = tempfile::tempdir().unwrap();
    let store = DebugStore::load(root.path()).await;
    store
        .save_workspace(DebugWorkspace {
            selected_bot: Some("old".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    store
        .save_collections(DebugCollections {
            folders: vec![folder("old", "old", 0)],
            ..Default::default()
        })
        .await
        .unwrap();
    store
        .append_history(entry("existing-history", "send_msg", "1", true))
        .await
        .unwrap();
    let history_before = std::fs::read(store.path(HISTORY_FILE)).unwrap();
    let workspace = DebugWorkspace {
        version: 2,
        tabs: vec![draft("imported")],
        active_tab: Some("imported".into()),
        selected_bot: Some("new".into()),
        layout: ncd_domain::onebot_debug::DebugLayout {
            left_width: 0,
            right_width: 0,
            ..Default::default()
        },
        ..Default::default()
    };
    let collections = DebugCollections {
        folders: vec![folder("imported", "new", 0)],
        requests: vec![saved("imported-request", Some("imported"), 0)],
        ..Default::default()
    };
    let transaction = JsonTransaction::new()
        .write(
            store.path(WORKSPACE_FILE),
            serde_json::to_value(&workspace).unwrap(),
        )
        .write(
            store.path(COLLECTIONS_FILE),
            serde_json::to_value(&collections).unwrap(),
        );
    let report = store
        .replace_config_with(Some(workspace), Some(collections), async {
            ncd_config::store::LocalConfigStore::new(root.path())
                .apply_transaction(transaction)
                .map_err(|e| e.to_string())
        })
        .await
        .unwrap();
    assert_eq!(report.written.len(), 2);
    let mut next_workspace = store.workspace().await;
    next_workspace.recent_actions.push("get_login_info".into());
    store.save_workspace(next_workspace).await.unwrap();
    let mut next_collections = store.collections().await;
    next_collections.folders[0].name = "renamed".into();
    store.save_collections(next_collections).await.unwrap();
    let reloaded = DebugStore::load(root.path()).await;
    assert_eq!(reloaded.workspace().await.tabs[0].id, "imported");
    assert_eq!(
        reloaded.workspace().await.selected_bot.as_deref(),
        Some("new")
    );
    assert_eq!(reloaded.workspace().await.version, 2);
    assert_eq!(reloaded.workspace().await.layout.left_width, 0);
    assert_eq!(reloaded.workspace().await.layout.right_width, 0);
    assert_eq!(reloaded.collections().await.folders[0].id, "imported");
    assert_eq!(
        reloaded.collections().await.requests[0].id,
        "imported-request"
    );
    assert_eq!(
        std::fs::read(store.path(HISTORY_FILE)).unwrap(),
        history_before
    );
    assert!(store.history_entry("existing-history").await.is_some());
}

#[tokio::test]
async fn failed_debug_import_keeps_both_loaded_caches_and_saved_values() {
    let root = tempfile::tempdir().unwrap();
    let store = DebugStore::load(root.path()).await;
    let workspace = DebugWorkspace {
        selected_bot: Some("old".into()),
        ..Default::default()
    };
    let collections = DebugCollections {
        folders: vec![folder("old", "old", 0)],
        ..Default::default()
    };
    store.save_workspace(workspace.clone()).await.unwrap();
    store.save_collections(collections.clone()).await.unwrap();
    let result = store
        .replace_config_with(
            Some(DebugWorkspace::default()),
            Some(DebugCollections::default()),
            async { Err::<(), _>("transaction failed".to_owned()) },
        )
        .await;
    assert_eq!(result.unwrap_err(), "transaction failed");
    assert_eq!(store.workspace().await, workspace);
    assert_eq!(store.collections().await, collections);
    store.save_workspace(store.workspace().await).await.unwrap();
    store
        .save_collections(store.collections().await)
        .await
        .unwrap();
    let reloaded = DebugStore::load(root.path()).await;
    assert_eq!(reloaded.workspace().await, workspace);
    assert_eq!(reloaded.collections().await, collections);
}

#[tokio::test]
async fn workspace_round_trips_across_reload() {
    let root = tempfile::tempdir().unwrap();
    let store = DebugStore::load(root.path()).await;
    assert_eq!(store.workspace().await, DebugWorkspace::default());

    let mut ws = DebugWorkspace {
        tabs: vec![draft("a"), draft("b")],
        active_tab: Some("b".to_owned()),
        selected_bot: Some("42".to_owned()),
        recent_actions: vec!["send_msg".to_owned()],
        ..DebugWorkspace::default()
    };
    ws.channel_choice.insert(
        "42".to_owned(),
        DebugChannelChoice {
            call: DebugChannelId::Http { name: "h".into() },
            events: DebugChannelId::Auto,
        },
    );
    ws.layout.left_width = 300;
    store.save_workspace(ws.clone()).await.unwrap();
    assert_eq!(store.workspace().await, ws);

    let reloaded = DebugStore::load(root.path()).await;
    assert_eq!(reloaded.workspace().await, ws);
    assert!(reloaded.take_storage_notices().is_empty());
}

#[tokio::test]
async fn collections_round_trip_across_reload() {
    let root = tempfile::tempdir().unwrap();
    let store = DebugStore::load(root.path()).await;
    let collections = DebugCollections {
        version: 1,
        folders: vec![folder("f1", "常用", 0)],
        requests: vec![saved("r1", Some("f1"), 0)],
    };
    store.save_collections(collections.clone()).await.unwrap();

    let reloaded = DebugStore::load(root.path()).await;
    assert_eq!(reloaded.collections().await, collections);
    assert!(reloaded.take_storage_notices().is_empty());
}

#[tokio::test]
async fn history_round_trips_across_reload() {
    let root = tempfile::tempdir().unwrap();
    let store = DebugStore::load(root.path()).await;
    for i in 0..3 {
        store
            .append_history(entry(&format!("e{i}"), "send_msg", "1", true))
            .await
            .unwrap();
    }
    let reloaded = DebugStore::load(root.path()).await;
    let page = reloaded.history(query()).await;
    let ids: Vec<&str> = page.entries.iter().map(|e| e.id.as_str()).collect();
    // 最新的在前
    assert_eq!(ids, ["e2", "e1", "e0"]);
    assert_eq!(page.total, 3);
    assert_eq!(
        reloaded.history_entry("e1").await,
        Some(entry("e1", "send_msg", "1", true))
    );
    assert!(reloaded.history_entry("nope").await.is_none());
    assert!(reloaded.take_storage_notices().is_empty());
}

#[tokio::test]
async fn saves_leave_no_temp_files() {
    let root = tempfile::tempdir().unwrap();
    let store = DebugStore::load(root.path()).await;
    store
        .save_workspace(DebugWorkspace::default())
        .await
        .unwrap();
    store
        .save_collections(DebugCollections::default())
        .await
        .unwrap();
    store
        .append_history(entry("e", "a", "1", true))
        .await
        .unwrap();
    let names = file_names(&debug_dir(root.path()));
    assert_eq!(
        names,
        ["collections.json", "history.jsonl", "workspace.json"]
    );
}

#[tokio::test]
async fn old_workspace_file_missing_fields_still_loads() {
    // 老版本写出的文件缺新字段：缺的取默认值，不该被当成损坏挪走
    let root = tempfile::tempdir().unwrap();
    let dir = debug_dir(root.path());
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(WORKSPACE_FILE),
        r#"{"tabs":[{"id":"a","action":"x","params_text":"{}","timeout_ms":null,"channel":null}]}"#,
    )
    .unwrap();
    std::fs::write(dir.join(COLLECTIONS_FILE), r#"{"folders":[]}"#).unwrap();

    let store = DebugStore::load(root.path()).await;
    let ws = store.workspace().await;
    assert_eq!(ws.tabs.len(), 1);
    assert_eq!(ws.layout, DebugWorkspace::default().layout);
    assert_eq!(store.collections().await, DebugCollections::default());
    assert!(store.take_storage_notices().is_empty());
    assert_eq!(file_names(&dir), ["collections.json", "workspace.json"]);
}

// --- 损坏文件 -------------------------------------------------------------

#[tokio::test]
async fn corrupt_workspace_is_moved_aside_with_notice() {
    let root = tempfile::tempdir().unwrap();
    let dir = debug_dir(root.path());
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(WORKSPACE_FILE), "{ not json").unwrap();

    let store = DebugStore::load(root.path()).await;
    assert_eq!(store.workspace().await, DebugWorkspace::default());

    let notices = store.take_storage_notices();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].file, WORKSPACE_FILE);
    assert!(!notices[0].reason.is_empty());
    let moved = PathBuf::from(&notices[0].moved_to);
    assert!(
        moved
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("workspace.json.broken-")
    );
    // 原件完整留在挪走的位置，正本没了（等下次保存重建）
    assert_eq!(std::fs::read_to_string(&moved).unwrap(), "{ not json");
    assert!(!dir.join(WORKSPACE_FILE).exists());
    // 提示只取一次
    assert!(store.take_storage_notices().is_empty());

    // 之后照常保存，不会碰那份留底
    store
        .save_workspace(DebugWorkspace::default())
        .await
        .unwrap();
    assert!(moved.exists());
    assert!(dir.join(WORKSPACE_FILE).exists());
}

#[tokio::test]
async fn corrupt_collections_is_moved_aside_with_notice() {
    let root = tempfile::tempdir().unwrap();
    let dir = debug_dir(root.path());
    std::fs::create_dir_all(&dir).unwrap();
    // 类型不对（folders 应该是数组）也算坏
    std::fs::write(dir.join(COLLECTIONS_FILE), r#"{"folders": 7}"#).unwrap();

    let store = DebugStore::load(root.path()).await;
    assert_eq!(store.collections().await, DebugCollections::default());
    let notices = store.take_storage_notices();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].file, COLLECTIONS_FILE);
    assert!(PathBuf::from(&notices[0].moved_to).exists());
}

#[tokio::test]
async fn bom_prefixed_json_is_accepted() {
    let root = tempfile::tempdir().unwrap();
    let dir = debug_dir(root.path());
    std::fs::create_dir_all(&dir).unwrap();
    let mut bytes = b"\xEF\xBB\xBF".to_vec();
    bytes.extend_from_slice(br#"{"selected_bot":"9"}"#);
    std::fs::write(dir.join(WORKSPACE_FILE), bytes).unwrap();

    let store = DebugStore::load(root.path()).await;
    assert_eq!(store.workspace().await.selected_bot.as_deref(), Some("9"));
    assert!(store.take_storage_notices().is_empty());
}

#[tokio::test]
async fn bad_history_lines_are_skipped_and_original_kept() {
    let root = tempfile::tempdir().unwrap();
    let dir = debug_dir(root.path());
    std::fs::create_dir_all(&dir).unwrap();
    let good1 = serde_json::to_string(&entry("g1", "a", "1", true)).unwrap();
    let good2 = serde_json::to_string(&entry("g2", "b", "1", true)).unwrap();
    // 中间一行坏的，末尾还有一行写到一半的（模拟崩在追加中途）
    let content = format!("{good1}\nthis is not json\n{good2}\n{{\"id\":\"half");
    std::fs::write(dir.join(HISTORY_FILE), &content).unwrap();

    let store = DebugStore::load(root.path()).await;
    let page = store.history(query()).await;
    let ids: Vec<&str> = page.entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["g2", "g1"]);

    let notices = store.take_storage_notices();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].file, HISTORY_FILE);
    assert!(notices[0].reason.contains("2 行"), "{}", notices[0].reason);
    // 原件（含坏行）整份留底
    assert_eq!(
        std::fs::read_to_string(&notices[0].moved_to).unwrap(),
        content
    );
    // 正本只剩好行，下次启动不再报
    let rewritten = std::fs::read_to_string(dir.join(HISTORY_FILE)).unwrap();
    assert_eq!(rewritten.lines().count(), 2);
    assert!(
        DebugStore::load(root.path())
            .await
            .take_storage_notices()
            .is_empty()
    );

    // 追加接在干净的正本后面
    store
        .append_history(entry("g3", "c", "1", true))
        .await
        .unwrap();
    let reloaded = DebugStore::load(root.path()).await;
    assert_eq!(reloaded.history(query()).await.total, 3);
}

#[tokio::test]
async fn blank_history_lines_are_not_bad() {
    let root = tempfile::tempdir().unwrap();
    let dir = debug_dir(root.path());
    std::fs::create_dir_all(&dir).unwrap();
    let good = serde_json::to_string(&entry("g", "a", "1", true)).unwrap();
    std::fs::write(dir.join(HISTORY_FILE), format!("\n{good}\r\n\n")).unwrap();

    let store = DebugStore::load(root.path()).await;
    assert_eq!(store.history(query()).await.total, 1);
    assert!(store.take_storage_notices().is_empty());
}

// --- 历史压缩与截断 -------------------------------------------------------------

#[tokio::test]
async fn history_compaction_keeps_newest_thousand() {
    let root = tempfile::tempdir().unwrap();
    let store = DebugStore::load(root.path()).await;
    let path = debug_dir(root.path()).join(HISTORY_FILE);

    for i in 1..=1200 {
        store
            .append_history(entry(&format!("e{i}"), "a", "1", true))
            .await
            .unwrap();
    }
    // 正好 1200 行还没到压缩线
    assert_eq!(
        std::fs::read_to_string(&path).unwrap().lines().count(),
        1200
    );

    store
        .append_history(entry("e1201", "a", "1", true))
        .await
        .unwrap();
    let content = std::fs::read_to_string(&path).unwrap();
    let ids: Vec<String> = content
        .lines()
        .map(|l| serde_json::from_str::<DebugHistoryEntry>(l).unwrap().id)
        .collect();
    assert_eq!(ids.len(), 1000);
    assert_eq!(ids.first().map(String::as_str), Some("e202"));
    assert_eq!(ids.last().map(String::as_str), Some("e1201"));

    // 内存同样只留最新 1000
    let page = store.history(query()).await;
    assert_eq!(page.total, 1000);
    assert!(store.history_entry("e201").await.is_none());
    assert!(store.history_entry("e202").await.is_some());

    // 压缩后继续追加，行数从 1000 往上数
    store
        .append_history(entry("e1202", "a", "1", true))
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap().lines().count(),
        1001
    );
}

#[tokio::test]
async fn oversized_history_file_is_compacted_on_load() {
    let root = tempfile::tempdir().unwrap();
    let dir = debug_dir(root.path());
    std::fs::create_dir_all(&dir).unwrap();
    let mut content = String::new();
    for i in 1..=1300 {
        content.push_str(&serde_json::to_string(&entry(&format!("e{i}"), "a", "1", true)).unwrap());
        content.push('\n');
    }
    std::fs::write(dir.join(HISTORY_FILE), content).unwrap();

    let store = DebugStore::load(root.path()).await;
    assert_eq!(store.history(query()).await.total, 1000);
    assert!(store.history_entry("e301").await.is_some());
    assert!(store.history_entry("e300").await.is_none());
    let lines = std::fs::read_to_string(dir.join(HISTORY_FILE))
        .unwrap()
        .lines()
        .count();
    assert_eq!(lines, 1000);
    assert!(store.take_storage_notices().is_empty());
}

#[tokio::test]
async fn oversized_response_is_dropped_and_flagged() {
    let root = tempfile::tempdir().unwrap();
    let store = DebugStore::load(root.path()).await;

    let mut big = entry("big", "get_group_member_list", "1", true);
    big.response = Some(Value::String("x".repeat(HISTORY_RESPONSE_LIMIT + 1)));
    store.append_history(big).await.unwrap();

    // 刚好在上限内的保留：字符串序列化后多两个引号，所以内容取 limit - 2
    let mut edge = entry("edge", "a", "1", true);
    edge.response = Some(Value::String("x".repeat(HISTORY_RESPONSE_LIMIT - 2)));
    store.append_history(edge).await.unwrap();

    let big = store.history_entry("big").await.unwrap();
    assert_eq!(big.response, None);
    assert!(big.response_truncated);
    let edge = store.history_entry("edge").await.unwrap();
    assert!(edge.response.is_some());
    assert!(!edge.response_truncated);

    // 落盘的也是截掉的版本，重开之后一样
    let reloaded = DebugStore::load(root.path()).await;
    let big = reloaded.history_entry("big").await.unwrap();
    assert_eq!(big.response, None);
    assert!(big.response_truncated);
}

#[tokio::test]
async fn already_truncated_flag_is_preserved() {
    let root = tempfile::tempdir().unwrap();
    let store = DebugStore::load(root.path()).await;
    let mut e = entry("t", "a", "1", true);
    e.response_truncated = true;
    store.append_history(e).await.unwrap();
    let stored = store.history_entry("t").await.unwrap();
    assert!(stored.response_truncated);
    assert!(stored.response.is_some());
}

#[tokio::test]
async fn clear_history_removes_file_and_memory() {
    let root = tempfile::tempdir().unwrap();
    let store = DebugStore::load(root.path()).await;
    // 没有文件时清空也不报错
    store.clear_history().await.unwrap();
    store
        .append_history(entry("e", "a", "1", true))
        .await
        .unwrap();
    store.clear_history().await.unwrap();
    assert_eq!(store.history(query()).await.total, 0);
    assert!(!debug_dir(root.path()).join(HISTORY_FILE).exists());
    // 清空后还能继续记
    store
        .append_history(entry("e2", "a", "1", true))
        .await
        .unwrap();
    assert_eq!(
        DebugStore::load(root.path())
            .await
            .history(query())
            .await
            .total,
        1
    );
}

// --- 查询 -------------------------------------------------------------

async fn seeded_store(root: &Path) -> DebugStore {
    let store = DebugStore::load(root).await;
    let mut e1 = entry("e1", "send_group_msg", "1", true);
    e1.params = json!({"group_id": 111, "message": "Hello World"});
    let mut e2 = entry("e2", "get_login_info", "2", false);
    e2.bot_name = "小号Bot".to_owned();
    e2.params = json!({});
    let mut e3 = entry("e3", "send_group_msg", "2", false);
    e3.params = json!({"group_id": 222});
    e3.error = Some(DebugError::Timeout { ms: 5000 });
    e3.retcode = None;
    for e in [e1, e2, e3] {
        store.append_history(e).await.unwrap();
    }
    store
}

fn ids(page: &DebugHistoryPage) -> Vec<&str> {
    page.entries.iter().map(|e| e.id.as_str()).collect()
}

#[tokio::test]
async fn query_filters_by_action_bot_and_ok() {
    let root = tempfile::tempdir().unwrap();
    let store = seeded_store(root.path()).await;

    let by_action = store
        .history(DebugHistoryQuery {
            action: Some("send_group_msg".into()),
            ..query()
        })
        .await;
    assert_eq!(ids(&by_action), ["e3", "e1"]);

    // action 是精确匹配，不是子串
    let partial = store
        .history(DebugHistoryQuery {
            action: Some("send_group".into()),
            ..query()
        })
        .await;
    assert_eq!(partial.total, 0);

    let by_bot = store
        .history(DebugHistoryQuery {
            bot_id: Some("2".into()),
            ..query()
        })
        .await;
    assert_eq!(ids(&by_bot), ["e3", "e2"]);

    let failed = store
        .history(DebugHistoryQuery {
            ok: Some(false),
            ..query()
        })
        .await;
    assert_eq!(ids(&failed), ["e3", "e2"]);

    let combined = store
        .history(DebugHistoryQuery {
            action: Some("send_group_msg".into()),
            bot_id: Some("2".into()),
            ok: Some(false),
            ..query()
        })
        .await;
    assert_eq!(ids(&combined), ["e3"]);
    // 摘要带出错误类型
    assert_eq!(combined.entries[0].error_kind.as_deref(), Some("timeout"));

    // 空串筛选等同没设
    let blank = store
        .history(DebugHistoryQuery {
            action: Some(String::new()),
            bot_id: Some(String::new()),
            text: Some("  ".into()),
            ..query()
        })
        .await;
    assert_eq!(blank.total, 3);
}

#[tokio::test]
async fn query_text_matches_action_params_and_bot_name_case_insensitively() {
    let root = tempfile::tempdir().unwrap();
    let store = seeded_store(root.path()).await;
    let text = |t: &str| DebugHistoryQuery {
        text: Some(t.to_owned()),
        ..query()
    };

    // action
    assert_eq!(ids(&store.history(text("LOGIN")).await), ["e2"]);
    // params 里的值，大小写不敏感
    assert_eq!(ids(&store.history(text("hello world")).await), ["e1"]);
    // params 里的键
    assert_eq!(ids(&store.history(text("group_id")).await), ["e3", "e1"]);
    // params 里的数字
    assert_eq!(ids(&store.history(text("222")).await), ["e3"]);
    // bot 名
    assert_eq!(ids(&store.history(text("小号bot")).await), ["e2"]);
    assert_eq!(store.history(text("no-such-thing")).await.total, 0);
}

#[tokio::test]
async fn query_paginates_and_clamps_limit() {
    let root = tempfile::tempdir().unwrap();
    let store = DebugStore::load(root.path()).await;
    for i in 0..250 {
        store
            .append_history(entry(&format!("e{i}"), "a", "1", true))
            .await
            .unwrap();
    }

    // limit 0 收成 1
    let one = store
        .history(DebugHistoryQuery {
            limit: 0,
            ..query()
        })
        .await;
    assert_eq!(one.entries.len(), 1);
    assert_eq!(one.total, 250);
    assert_eq!(one.entries[0].id, "e249");

    // 过大的 limit 收成 200
    let capped = store
        .history(DebugHistoryQuery {
            limit: 9999,
            ..query()
        })
        .await;
    assert_eq!(capped.entries.len(), 200);
    assert_eq!(capped.total, 250);

    // offset 翻页；最后一页不足 limit
    let tail = store
        .history(DebugHistoryQuery {
            limit: 100,
            offset: 200,
            ..query()
        })
        .await;
    assert_eq!(tail.entries.len(), 50);
    assert_eq!(tail.entries.last().map(|e| e.id.as_str()), Some("e0"));

    let beyond = store
        .history(DebugHistoryQuery {
            offset: 10_000,
            ..query()
        })
        .await;
    assert!(beyond.entries.is_empty());
    assert_eq!(beyond.total, 250);
}

// --- 工作区上限 -------------------------------------------------------------

#[tokio::test]
async fn workspace_limits_are_enforced_on_save() {
    let root = tempfile::tempdir().unwrap();
    let store = DebugStore::load(root.path()).await;

    let ws = DebugWorkspace {
        closed_tabs: (0..15).map(|i| draft(&format!("c{i}"))).collect(),
        recent_actions: (0..30).map(|i| format!("act{i}")).collect(),
        // 60 个标签，激活的恰好是最老的那个
        tabs: (0..60).map(|i| draft(&format!("t{i}"))).collect(),
        active_tab: Some("t0".to_owned()),
        ..DebugWorkspace::default()
    };
    store.save_workspace(ws).await.unwrap();

    let saved = store.workspace().await;
    assert_eq!(saved.closed_tabs.len(), 10);
    // 新的在前，截尾
    assert_eq!(saved.closed_tabs[0].id, "c0");
    assert_eq!(saved.recent_actions.len(), 20);
    assert_eq!(saved.recent_actions[0], "act0");
    assert_eq!(saved.tabs.len(), 50);
    // 激活的保住，丢的是最老的非激活标签：t1..=t10
    assert_eq!(saved.tabs[0].id, "t0");
    assert_eq!(saved.tabs[1].id, "t11");
    assert_eq!(saved.tabs.last().map(|t| t.id.as_str()), Some("t59"));

    // 落盘的也是收过的
    let reloaded = DebugStore::load(root.path()).await;
    assert_eq!(reloaded.workspace().await, saved);
}

#[tokio::test]
async fn oversized_workspace_file_is_trimmed_on_load() {
    let root = tempfile::tempdir().unwrap();
    let dir = debug_dir(root.path());
    std::fs::create_dir_all(&dir).unwrap();
    let ws = DebugWorkspace {
        recent_actions: (0..40).map(|i| format!("act{i}")).collect(),
        ..DebugWorkspace::default()
    };
    std::fs::write(dir.join(WORKSPACE_FILE), serde_json::to_vec(&ws).unwrap()).unwrap();

    let store = DebugStore::load(root.path()).await;
    assert_eq!(store.workspace().await.recent_actions.len(), 20);
}

#[test]
fn many_tabs_sharing_the_active_id_still_fit() {
    // 畸形数据：全部标签 id 相同且都是激活的，也不能超上限或死循环
    let ws = DebugWorkspace {
        tabs: (0..80).map(|_| draft("same")).collect(),
        active_tab: Some("same".to_owned()),
        ..DebugWorkspace::default()
    };
    assert_eq!(normalize_workspace(ws).tabs.len(), MAX_TABS);
}

// --- 导入导出 -------------------------------------------------------------

#[tokio::test]
async fn import_merges_and_renames_clashing_ids() {
    let root = tempfile::tempdir().unwrap();
    let store = DebugStore::load(root.path()).await;
    store
        .save_collections(DebugCollections {
            version: 1,
            folders: vec![folder("f1", "常用", 0)],
            requests: vec![saved("r1", Some("f1"), 0), saved("r9", None, 3)],
        })
        .await
        .unwrap();

    // 导入文件里 f1 / r1 都和现有的撞了；r3 挂在导入文件里没有的文件夹上
    let incoming = DebugCollections {
        version: 1,
        folders: vec![folder("f1", "常用", 5), folder("f2", "群管理", 6)],
        requests: vec![
            saved("r1", Some("f1"), 10),
            saved("r2", Some("f2"), 11),
            saved("r3", Some("ghost"), 12),
        ],
    };
    let file = root.path().join("in.json");
    std::fs::write(&file, serde_json::to_vec_pretty(&incoming).unwrap()).unwrap();

    let merged = store.import_collections(&file).await.unwrap();

    // 现有的原样在前
    assert_eq!(merged.folders[0], folder("f1", "常用", 0));
    assert_eq!(merged.requests[0], saved("r1", Some("f1"), 0));
    assert_eq!(merged.folders.len(), 3);
    assert_eq!(merged.requests.len(), 5);

    // 文件夹名照原样保留，同名也不合并、不改名
    let names: Vec<&str> = merged.folders.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["常用", "常用", "群管理"]);

    // 全部 id 唯一
    let mut all: HashSet<&str> = HashSet::new();
    for id in merged
        .folders
        .iter()
        .map(|f| f.id.as_str())
        .chain(merged.requests.iter().map(|r| r.id.as_str()))
    {
        assert!(all.insert(id), "重复的 id: {id}");
    }

    // 撞了的换新 id，没撞的沿用
    let new_f1 = &merged.folders[1];
    assert_ne!(new_f1.id, "f1");
    assert_eq!(merged.folders[2].id, "f2");
    let imported_r1 = &merged.requests[2];
    assert_ne!(imported_r1.id, "r1");
    // 请求跟着换后的文件夹 id 走
    assert_eq!(imported_r1.folder_id.as_deref(), Some(new_f1.id.as_str()));
    assert_eq!(merged.requests[3].id, "r2");
    assert_eq!(merged.requests[3].folder_id.as_deref(), Some("f2"));
    // 挂在不存在的文件夹上的落到根
    assert_eq!(merged.requests[4].folder_id, None);

    // 导入的 order 排在现有的后面，彼此先后不变
    assert!(new_f1.order > 0);
    assert!(merged.folders[2].order > new_f1.order);
    assert!(imported_r1.order > 3);
    assert!(merged.requests[3].order > imported_r1.order);

    // 已落盘、返回值和内存一致
    assert_eq!(store.collections().await, merged);
    assert_eq!(
        DebugStore::load(root.path()).await.collections().await,
        merged
    );
}

#[tokio::test]
async fn import_into_empty_keeps_ids_and_starts_orders_at_zero() {
    let root = tempfile::tempdir().unwrap();
    let store = DebugStore::load(root.path()).await;
    let incoming = DebugCollections {
        version: 1,
        folders: vec![folder("f1", "A", 7)],
        requests: vec![saved("r1", Some("f1"), 4), saved("r2", None, 5)],
    };
    let file = root.path().join("in.json");
    std::fs::write(&file, serde_json::to_vec(&incoming).unwrap()).unwrap();

    let merged = store.import_collections(&file).await.unwrap();
    assert_eq!(merged.folders[0].id, "f1");
    assert_eq!(merged.folders[0].order, 0);
    assert_eq!(merged.requests[0].id, "r1");
    assert_eq!(merged.requests[0].order, 0);
    assert_eq!(merged.requests[1].order, 1);
    assert_eq!(merged.requests[0].folder_id.as_deref(), Some("f1"));
}

#[tokio::test]
async fn import_rejects_bad_files_without_touching_state() {
    let root = tempfile::tempdir().unwrap();
    let store = DebugStore::load(root.path()).await;
    let before = DebugCollections {
        version: 1,
        folders: vec![folder("f1", "常用", 0)],
        requests: Vec::new(),
    };
    store.save_collections(before.clone()).await.unwrap();

    let cases: [(&str, &str); 4] = [
        ("garbage.json", "not json at all"),
        ("array.json", "[1,2,3]"),
        // 合法 JSON 对象但不是收藏夹（比如误选了 workspace.json）
        ("workspace.json", r#"{"tabs":[],"version":1}"#),
        ("wrong-type.json", r#"{"folders": 7}"#),
    ];
    for (name, content) in cases {
        let file = root.path().join(name);
        std::fs::write(&file, content).unwrap();
        let err = store.import_collections(&file).await.unwrap_err();
        assert!(!err.is_empty(), "{name}");
        // 用户的文件不能被挪走
        assert!(file.exists(), "{name}");
    }
    let missing = store
        .import_collections(&root.path().join("nope.json"))
        .await
        .unwrap_err();
    assert!(missing.contains("读取导入文件失败"));

    assert_eq!(store.collections().await, before);
    assert!(store.take_storage_notices().is_empty());
}

#[tokio::test]
async fn export_then_import_round_trips() {
    let root = tempfile::tempdir().unwrap();
    let src = DebugStore::load(&root.path().join("a")).await;
    let collections = DebugCollections {
        version: 1,
        folders: vec![folder("f1", "常用", 0)],
        requests: vec![saved("r1", Some("f1"), 0)],
    };
    src.save_collections(collections.clone()).await.unwrap();

    let out = root.path().join("export.json");
    // 覆盖已有文件也行
    std::fs::write(&out, "old").unwrap();
    src.export_collections(&out).await.unwrap();
    let exported: DebugCollections = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    assert_eq!(exported, collections);
    // 导出目录里不留临时文件
    assert_eq!(file_names(root.path()), ["a", "export.json"]);

    let dst = DebugStore::load(&root.path().join("b")).await;
    let merged = dst.import_collections(&out).await.unwrap();
    assert_eq!(merged, collections);
}

#[tokio::test]
async fn export_to_missing_directory_fails_cleanly() {
    let root = tempfile::tempdir().unwrap();
    let store = DebugStore::load(root.path()).await;
    let err = store
        .export_collections(&root.path().join("no-dir").join("x.json"))
        .await
        .unwrap_err();
    assert!(err.contains("创建临时文件失败"), "{err}");
}

// --- 调用 -> 历史 -------------------------------------------------------------

#[test]
fn only_editor_and_composer_calls_are_recorded() {
    let result = DebugCallResult::Ok {
        outcome: outcome(true, json!({"status": "ok"}), false),
    };
    let channel = DebugChannelId::Http { name: "h".into() };
    for origin in [DebugCallOrigin::Editor, DebugCallOrigin::Composer] {
        let e = history_entry_from_call(
            "1",
            "小号",
            BackendType::SnowLuma,
            &request(origin.clone()),
            &channel,
            &result,
        )
        .unwrap();
        assert_eq!(e.origin, origin);
        assert_eq!(e.bot_id, "1");
        assert_eq!(e.bot_name, "小号");
        assert_eq!(e.backend, BackendType::SnowLuma);
        // 记的是实际用到的通道，不是请求里的 auto
        assert_eq!(e.channel, channel);
        assert_eq!(e.action, "send_msg");
        assert_eq!(e.params, json!({"message": "hi"}));
        assert!(e.ok);
        assert_eq!(e.retcode, Some(0));
        assert_eq!(e.error, None);
        assert_eq!(e.elapsed_ms, 34);
        assert_eq!(e.response, Some(json!({"status": "ok"})));
        assert!(!e.response_truncated);
        assert!(!e.id.is_empty());
    }
    for origin in [DebugCallOrigin::Picker, DebugCallOrigin::Other] {
        assert!(
            history_entry_from_call(
                "1",
                "x",
                BackendType::NapCat,
                &request(origin),
                &channel,
                &result
            )
            .is_none()
        );
    }
}

#[test]
fn failed_calls_are_recorded_with_error() {
    let req = request(DebugCallOrigin::Editor);
    let channel = DebugChannelId::Internal;

    let timeout = history_entry_from_call(
        "1",
        "b",
        BackendType::NapCat,
        &req,
        &channel,
        &DebugCallResult::Err {
            error: DebugError::Timeout { ms: 5000 },
        },
    )
    .unwrap();
    assert!(!timeout.ok);
    assert_eq!(timeout.retcode, None);
    assert_eq!(timeout.error, Some(DebugError::Timeout { ms: 5000 }));
    assert_eq!(timeout.elapsed_ms, 5000);
    assert_eq!(timeout.response, None);

    let cancelled = history_entry_from_call(
        "1",
        "b",
        BackendType::NapCat,
        &req,
        &channel,
        &DebugCallResult::Err {
            error: DebugError::Cancelled,
        },
    )
    .unwrap();
    assert_eq!(cancelled.elapsed_ms, 0);

    // OneBot 回包里 retcode 非 0：拿到了回包，ok 为假，没有 error，回包照存
    let failed = history_entry_from_call(
        "1",
        "b",
        BackendType::NapCat,
        &req,
        &channel,
        &DebugCallResult::Ok {
            outcome: outcome(false, json!({"retcode": 1400}), true),
        },
    )
    .unwrap();
    assert!(!failed.ok);
    assert_eq!(failed.retcode, Some(1400));
    assert_eq!(failed.error, None);
    assert!(failed.response_truncated);
}

// --- 内存预算 -------------------------------------------------------------

fn entry_with_response(id: &str, bytes: usize) -> DebugHistoryEntry {
    let mut e = entry(id, "get_group_member_list", "1", true);
    e.response = Some(Value::String("x".repeat(bytes)));
    e
}

#[tokio::test]
async fn response_budget_evicts_oldest_and_history_entry_reads_disk() {
    let root = tempfile::tempdir().unwrap();
    // 每条回包序列化后 20482 字节，预算 100 KiB 只装得下 4 条
    let budget = 100 * 1024;
    let store = DebugStore::load_with_budget(root.path(), budget).await;
    for i in 0..20 {
        store
            .append_history(entry_with_response(&format!("e{i}"), 20 * 1024))
            .await
            .unwrap();
    }
    {
        let mem = store.history.lock().await;
        let in_memory = (0..20)
            .filter(|index| matches!(mem.lookup(&format!("e{index}")), Some(HistoryLookup::Complete(entry)) if entry.response.is_some()))
            .count();
        assert_eq!(in_memory, 4);
        // 最老的需要补读，最新的已经完整；不依赖缓存队列或预算字段。
        assert!(
            matches!(mem.lookup("e0"), Some(HistoryLookup::NeedsResponse(entry)) if entry.response.is_none())
        );
        assert!(
            matches!(mem.lookup("e19"), Some(HistoryLookup::Complete(entry)) if entry.response.is_some())
        );
    }

    // 列表摘要不受影响；被丢的回包从文件里读回全文
    assert_eq!(store.history(query()).await.total, 20);
    let full = store.history_entry("e0").await.unwrap();
    assert_eq!(full.response, Some(Value::String("x".repeat(20 * 1024))));
    assert!(!full.response_truncated);
    assert_eq!(full.params, json!({"group_id": 123}));
    // 内存里还留着的直接返回
    let newest = store.history_entry("e19").await.unwrap();
    assert!(newest.response.is_some());
}

#[tokio::test]
async fn disk_lookup_uses_newest_duplicate_id_after_response_eviction() {
    let root = tempfile::tempdir().unwrap();
    let store = DebugStore::load_with_budget(root.path(), 1).await;
    let older = entry("duplicate", "old_action", "old_bot", true);
    let mut newer = entry("duplicate", "new_action", "new_bot", false);
    newer.response = Some(json!({"version": "newest"}));
    store.append_history(older).await.unwrap();
    store.append_history(newer.clone()).await.unwrap();

    // ID 查找已经按最新条目选择，回包被预算淘汰后也必须返回同一份记录。
    assert!(matches!(
        store.history.lock().await.lookup("duplicate"),
        Some(HistoryLookup::NeedsResponse(_))
    ));
    assert_eq!(store.history_entry("duplicate").await, Some(newer.clone()));
    assert_eq!(
        store.history_entry_read_only("duplicate").await.unwrap(),
        Some(newer.clone())
    );
    let reloaded = DebugStore::load_with_budget(root.path(), 1).await;
    assert_eq!(reloaded.history_entry("duplicate").await, Some(newer));
}

#[tokio::test]
async fn disk_lookup_accepts_valid_history_json_field_order_and_spacing() {
    let record = entry("formatted", "get_status", "1", true);
    let canonical = serde_json::to_string(&record).unwrap();
    let fields = canonical.strip_prefix("{\"id\":\"formatted\",").unwrap();
    let reordered = format!(
        "{{{},\"id\":\"formatted\"}}",
        fields.strip_suffix('}').unwrap()
    );
    let spaced = canonical.replacen("{\"id\":", "{ \"id\" : ", 1);
    for row in [reordered, spaced] {
        let root = tempfile::tempdir().unwrap();
        let path = debug_dir(root.path()).join(HISTORY_FILE);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("{row}\n")).unwrap();
        let store = DebugStore::load_with_budget(root.path(), 1).await;
        assert_eq!(store.history(query()).await.total, 1);
        assert_eq!(store.history_entry("formatted").await, Some(record.clone()));
        assert_eq!(
            store.history_entry_read_only("formatted").await.unwrap(),
            Some(record.clone())
        );
        assert!(store.take_storage_notices().is_empty());
    }
}

#[tokio::test]
async fn evicted_response_missing_from_disk_is_flagged_truncated() {
    let root = tempfile::tempdir().unwrap();
    let store = DebugStore::load_with_budget(root.path(), 30 * 1024).await;
    for i in 0..3 {
        store
            .append_history(entry_with_response(&format!("e{i}"), 20 * 1024))
            .await
            .unwrap();
    }
    std::fs::remove_file(debug_dir(root.path()).join(HISTORY_FILE)).unwrap();

    let gone = store.history_entry("e0").await.unwrap();
    assert_eq!(gone.response, None);
    // 不能让界面把「拿不到」当成「这次调用没有回包」
    assert!(gone.response_truncated);
}

#[tokio::test]
async fn load_respects_response_budget() {
    let root = tempfile::tempdir().unwrap();
    let budget = 100 * 1024;
    {
        // 写的时候不设限，文件里每条都有全文
        let store = DebugStore::load(root.path()).await;
        for i in 0..20 {
            store
                .append_history(entry_with_response(&format!("e{i}"), 20 * 1024))
                .await
                .unwrap();
        }
    }
    let reloaded = DebugStore::load_with_budget(root.path(), budget).await;
    {
        let mem = reloaded.history.lock().await;
        let in_memory = (0..20)
            .filter(|index| matches!(mem.lookup(&format!("e{index}")), Some(HistoryLookup::Complete(entry)) if entry.response.is_some()))
            .count();
        assert_eq!(in_memory, 4);
        assert_eq!(mem.page(&query()).total, 20);
    }
    let oldest = reloaded.history_entry("e0").await.unwrap();
    assert_eq!(oldest.response, Some(Value::String("x".repeat(20 * 1024))));
    assert!(reloaded.take_storage_notices().is_empty());
}

#[tokio::test]
async fn streaming_compaction_keeps_responses_evicted_from_memory() {
    let root = tempfile::tempdir().unwrap();
    // 预算只够留十几条回包，其余 1000 多条的全文只在文件里
    let store = DebugStore::load_with_budget(root.path(), 64 * 1024).await;
    for i in 1..=1201 {
        store
            .append_history(entry_with_response(&format!("e{i}"), 2 * 1024))
            .await
            .unwrap();
    }
    // 第 1201 条触发了压缩，只剩最新 1000 行，而且每一行的回包都还在
    let path = debug_dir(root.path()).join(HISTORY_FILE);
    let content = std::fs::read_to_string(&path).unwrap();
    let entries: Vec<DebugHistoryEntry> = content
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(entries.len(), 1000);
    assert_eq!(entries[0].id, "e202");
    assert_eq!(entries[999].id, "e1201");
    assert!(entries.iter().all(|e| e.response.is_some()));

    // 内存里早就丢了回包的条目，压缩后仍能读回全文
    {
        let mem = store.history.lock().await;
        assert!(matches!(
            mem.lookup("e202"),
            Some(HistoryLookup::NeedsResponse(_))
        ));
    }
    let full = store.history_entry("e202").await.unwrap();
    assert_eq!(full.response, Some(Value::String("x".repeat(2 * 1024))));
    assert!(store.history_entry("e201").await.is_none());
    // 压缩后继续追加
    store
        .append_history(entry_with_response("e1202", 16))
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap().lines().count(),
        1001
    );
}

#[test]
fn unpersisted_responses_are_never_evicted() {
    // 磁盘写失败的条目只在内存里有回包，丢了就找不回来
    let mut mem = HistoryMem::new(100);
    mem.push(entry_with_response("a", 200), 202, false);
    mem.push(entry_with_response("b", 200), 202, true);
    assert!(
        matches!(mem.lookup("a"), Some(HistoryLookup::Complete(entry)) if entry.response == Some(Value::String("x".repeat(200))))
    );
    assert!(
        matches!(mem.lookup("b"), Some(HistoryLookup::NeedsResponse(entry)) if entry.response.is_none())
    );
}

#[test]
fn response_budget_survives_history_rotation_and_clear() {
    let mut mem = HistoryMem::new(20);
    // 超过保留上限后游标必须随队列一起前移，仍只保留最新一份回包。
    for index in 0..HISTORY_KEEP + 10 {
        mem.push(entry_with_response(&index.to_string(), 18), 20, true);
    }
    assert_eq!(mem.page(&query()).total as usize, HISTORY_KEEP);
    assert!(mem.lookup("9").is_none());
    assert!(
        matches!(mem.lookup(&(HISTORY_KEEP + 9).to_string()), Some(HistoryLookup::Complete(entry)) if entry.response.is_some())
    );
    assert!((10..HISTORY_KEEP + 9).all(|index| matches!(
        mem.lookup(&index.to_string()),
        Some(HistoryLookup::NeedsResponse(_))
    )));

    mem.clear();
    assert_eq!(mem.page(&query()).total, 0);
    mem.push(entry_with_response("fresh", 30), 32, true);
    assert!(matches!(
        mem.lookup("fresh"),
        Some(HistoryLookup::NeedsResponse(_))
    ));
}

// --- 参数与凭据 -------------------------------------------------------------

#[tokio::test]
async fn oversized_param_strings_are_replaced_by_placeholder() {
    let root = tempfile::tempdir().unwrap();
    let store = DebugStore::load(root.path()).await;
    let mut e = entry("p", "send_msg", "1", true);
    e.params = json!({
        "file": "a".repeat(PARAM_STRING_LIMIT + 1),
        "edge": "b".repeat(PARAM_STRING_LIMIT),
        "nested": [{"deep": "c".repeat(100_000)}],
        // 多字节字符按字节数算
        "cjk": "汉".repeat(30_000),
        "n": 1,
        "s": "small",
    });
    store.append_history(e).await.unwrap();

    let stored = store.history_entry("p").await.unwrap();
    assert_eq!(stored.params["file"], "<已省略 65537 字节>");
    assert_eq!(stored.params["nested"][0]["deep"], "<已省略 100000 字节>");
    assert_eq!(stored.params["cjk"], "<已省略 90000 字节>");
    // 被瘦身的要标出来：前端据此拦住「把占位文字当参数重放」
    assert!(stored.params_truncated);
    // 刚好在上限内的不动
    assert_eq!(
        stored.params["edge"].as_str().map(str::len),
        Some(PARAM_STRING_LIMIT)
    );
    assert_eq!(stored.params["n"], 1);
    assert_eq!(stored.params["s"], "small");

    // 落盘的也是换过的版本
    let reloaded = DebugStore::load(root.path()).await;
    let stored = reloaded.history_entry("p").await.unwrap();
    assert_eq!(stored.params["file"], "<已省略 65537 字节>");
    assert!(stored.params_truncated);
}

#[tokio::test]
async fn untrimmed_params_stay_unflagged_and_old_entries_default_false() {
    let root = tempfile::tempdir().unwrap();
    let store = DebugStore::load(root.path()).await;
    store
        .append_history(entry("small", "send_msg", "1", true))
        .await
        .unwrap();
    let stored = store.history_entry("small").await.unwrap();
    assert!(!stored.params_truncated);

    // 老版本写下的历史里没有这个字段：按没瘦过读，不报错
    let path = root.path().join("onebot-debug").join("history.jsonl");
    let raw = std::fs::read_to_string(&path).unwrap();
    let cleaned = raw
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let mut v: serde_json::Value = serde_json::from_str(l).unwrap();
            v.as_object_mut().unwrap().remove("params_truncated");
            serde_json::to_string(&v).unwrap()
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&path, cleaned).unwrap();
    let reloaded = DebugStore::load(root.path()).await;
    let stored = reloaded.history_entry("small").await.unwrap();
    assert!(!stored.params_truncated);
}

#[tokio::test]
async fn credential_action_responses_are_not_stored() {
    let root = tempfile::tempdir().unwrap();
    let store = DebugStore::load(root.path()).await;
    let names = [
        "get_cookies",
        "get_credentials",
        "get_csrf_token",
        "get_clientkey",
        "get_rkey",
        "nc_get_rkey",
        "get_rkey_server",
        "get_cookies_async",
        "get_credentials_async",
        "get_csrf_token_async",
        "get_clientkey_async",
        "nc_get_rkey_async",
    ];
    for name in names {
        let mut e = entry(name, name, "1", true);
        e.response = Some(json!({"data": {"cookies": "uin=o123; skey=SECRET"}}));
        // 就算调用层带了「被截断」的标记，也不置位：这条本来就不该有回包
        e.response_truncated = true;
        store.append_history(e).await.unwrap();
    }
    let mut normal = entry("normal", "get_login_info", "1", true);
    normal.response = Some(json!({"data": {"nickname": "x"}}));
    store.append_history(normal).await.unwrap();

    async fn check(store: &DebugStore, names: &[&str]) {
        for name in names {
            let stored = store.history_entry(name).await.unwrap();
            assert_eq!(stored.response, None, "{name}");
            assert!(!stored.response_truncated, "{name}");
            assert_eq!(stored.action, *name);
        }
        assert!(
            store
                .history_entry("normal")
                .await
                .unwrap()
                .response
                .is_some()
        );
    }
    check(&store, &names).await;

    // 文件里也不能有明文
    let content = std::fs::read_to_string(debug_dir(root.path()).join(HISTORY_FILE)).unwrap();
    assert!(!content.contains("SECRET"));
    check(&DebugStore::load(root.path()).await, &names).await;
}

#[test]
fn credential_action_matching_is_exact() {
    assert!(is_credential_action("get_cookies"));
    assert!(is_credential_action("get_clientkey_async"));
    assert!(!is_credential_action("get_cookies_extra"));
    assert!(!is_credential_action("get_login_info"));
    assert!(!is_credential_action("_async"));
}

// --- 写盘失败、临时文件 -------------------------------------------------------------

#[tokio::test]
async fn failed_writes_leave_memory_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let dir = debug_dir(root.path());
    let store = DebugStore::load(root.path()).await;

    let ws_a = DebugWorkspace {
        selected_bot: Some("a".to_owned()),
        ..DebugWorkspace::default()
    };
    let col_a = DebugCollections {
        version: 1,
        folders: vec![folder("f1", "常用", 0)],
        requests: Vec::new(),
    };
    store.save_workspace(ws_a.clone()).await.unwrap();
    store.save_collections(col_a.clone()).await.unwrap();
    store
        .append_history(entry("h1", "a", "1", true))
        .await
        .unwrap();

    // 把目标文件换成同名目录：临时文件写得成，rename 必然失败
    for file in [WORKSPACE_FILE, COLLECTIONS_FILE, HISTORY_FILE] {
        std::fs::remove_file(dir.join(file)).unwrap();
        std::fs::create_dir(dir.join(file)).unwrap();
    }

    let ws_b = DebugWorkspace {
        selected_bot: Some("b".to_owned()),
        ..DebugWorkspace::default()
    };
    let err = store.save_workspace(ws_b).await.unwrap_err();
    assert!(err.contains("替换"), "{err}");
    assert_eq!(store.workspace().await, ws_a);

    let col_b = DebugCollections {
        version: 1,
        folders: vec![folder("f2", "别的", 0)],
        requests: Vec::new(),
    };
    store.save_collections(col_b).await.unwrap_err();
    assert_eq!(store.collections().await, col_a);

    let incoming = root.path().join("in.json");
    std::fs::write(
        &incoming,
        serde_json::to_vec(&DebugCollections {
            version: 1,
            folders: vec![folder("f9", "导入", 0)],
            requests: Vec::new(),
        })
        .unwrap(),
    )
    .unwrap();
    store.import_collections(&incoming).await.unwrap_err();
    assert_eq!(store.collections().await, col_a);

    // 清空历史：文件删不掉就不清内存
    store.clear_history().await.unwrap_err();
    assert_eq!(store.history(query()).await.total, 1);

    // 失败的写盘不留临时文件（现在目录里只有那三个占位目录）
    assert_eq!(
        file_names(&dir),
        ["collections.json", "history.jsonl", "workspace.json"]
    );
}

#[tokio::test]
async fn failed_append_keeps_entry_in_memory_and_reports_error() {
    let root = tempfile::tempdir().unwrap();
    let dir = debug_dir(root.path());
    let store = DebugStore::load_with_budget(root.path(), 1).await;
    // 加载之后再让 history.jsonl 变成目录，打开追加必然失败
    // （加载前就是目录的话，会被当成读不出来的文件挪开）
    std::fs::create_dir_all(dir.join(HISTORY_FILE)).unwrap();

    let err = store
        .append_history(entry("h1", "a", "1", true))
        .await
        .unwrap_err();
    assert!(err.contains("打开调用历史文件失败"), "{err}");
    // 本次运行里仍能查到
    assert_eq!(store.history(query()).await.total, 1);
    assert_eq!(
        store.history_entry("h1").await.unwrap().response,
        Some(json!({"status": "ok"}))
    );
}

#[tokio::test]
async fn orphan_tmp_files_are_removed_on_load() {
    let root = tempfile::tempdir().unwrap();
    let dir = debug_dir(root.path());
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("workspace.json.tmp-1111"), "x").unwrap();
    std::fs::write(dir.join("collections.json.tmp-2222"), "x").unwrap();
    std::fs::write(dir.join("history.jsonl.tmp-3333"), "x").unwrap();
    // 不是我们的文件，不能碰
    std::fs::write(dir.join("notes.tmp-4444"), "keep").unwrap();
    std::fs::write(dir.join(WORKSPACE_FILE), "{}").unwrap();

    let store = DebugStore::load(root.path()).await;
    assert_eq!(file_names(&dir), ["notes.tmp-4444", "workspace.json"]);
    assert!(store.take_storage_notices().is_empty());
}

#[tokio::test]
async fn valid_last_line_without_newline_is_repaired_on_load() {
    // 崩在追加中途、恰好整行都写进去了只缺换行：读得出来，但不修的话下一条会和它粘成一行
    let root = tempfile::tempdir().unwrap();
    let dir = debug_dir(root.path());
    std::fs::create_dir_all(&dir).unwrap();
    let g1 = serde_json::to_string(&entry("g1", "a", "1", true)).unwrap();
    let g2 = serde_json::to_string(&entry("g2", "a", "1", true)).unwrap();
    std::fs::write(dir.join(HISTORY_FILE), format!("{g1}\n{g2}")).unwrap();

    let store = DebugStore::load(root.path()).await;
    assert_eq!(store.history(query()).await.total, 2);
    assert!(store.take_storage_notices().is_empty());
    store
        .append_history(entry("g3", "a", "1", true))
        .await
        .unwrap();
    let reloaded = DebugStore::load(root.path()).await;
    assert_eq!(reloaded.history(query()).await.total, 3);
    assert!(reloaded.take_storage_notices().is_empty());
    assert_eq!(
        std::fs::read_to_string(dir.join(HISTORY_FILE))
            .unwrap()
            .lines()
            .count(),
        3
    );
}

#[test]
fn notice_explains_why_file_was_not_moved() {
    let moved = notice(
        "workspace.json",
        Err("拒绝访问".to_owned()),
        "EOF".to_owned(),
    );
    assert_eq!(moved.moved_to, "");
    assert!(moved.reason.contains("EOF"));
    assert!(moved.reason.contains("拒绝访问"));

    let ok = notice("workspace.json", Ok(PathBuf::from("a/b")), "EOF".to_owned());
    assert_eq!(ok.moved_to, PathBuf::from("a/b").display().to_string());
    assert_eq!(ok.reason, "EOF");
}

// Windows 上用「不允许别人删除 / 替换」的方式打开文件，模拟杀软占着 history.jsonl，
// 让压缩的 rename 一直失败
#[cfg(windows)]
#[tokio::test]
async fn failed_compaction_does_not_fail_append_and_backs_off() {
    use std::os::windows::fs::OpenOptionsExt;

    let root = tempfile::tempdir().unwrap();
    let store = DebugStore::load(root.path()).await;
    let path = debug_dir(root.path()).join(HISTORY_FILE);
    store
        .append_history(entry("e1", "a", "1", true))
        .await
        .unwrap();

    // FILE_SHARE_READ | FILE_SHARE_WRITE，不含 FILE_SHARE_DELETE
    let blocker = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&path)
        .unwrap();
    for i in 2..=1201 {
        // 第 1201 条触发压缩，压缩失败也不能让追加失败
        store
            .append_history(entry(&format!("e{i}"), "a", "1", true))
            .await
            .unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(&path).unwrap().lines().count(),
        1201
    );
    {
        let gate = store.write_gate.lock().await;
        assert_eq!(gate.compact_retry_at, 1201 + COMPACT_BACKOFF);
    }
    // 退避期内不再重试：到 1401 行为止文件只增不减
    for i in 1202..=1401 {
        store
            .append_history(entry(&format!("e{i}"), "a", "1", true))
            .await
            .unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(&path).unwrap().lines().count(),
        1401
    );

    // 占用解除后，退避期一过就压缩成功
    drop(blocker);
    store
        .append_history(entry("e1402", "a", "1", true))
        .await
        .unwrap();
    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content.lines().count(), 1000);
    assert!(content.lines().last().unwrap().contains("\"e1402\""));
    let gate = store.write_gate.lock().await;
    assert_eq!(gate.compact_retry_at, 0);
    assert_eq!(gate.history_lines, 1000);
}
