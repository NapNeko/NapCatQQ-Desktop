use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::graph::pick_core_for_test;
use super::*;

fn runtime(v: serde_json::Value) -> MaiBotMemoryStatus {
    status_from(serde_json::from_value(v).unwrap())
}

#[test]
fn runtime_config_maps_to_a_state() {
    let off = runtime(json!({"success": true, "memory_enabled": false, "disabled": true, "reason": "a_memorix_disabled"}));
    assert_eq!(off.state, MaiBotMemoryState::Disabled);
    let starting = runtime(json!({"success": true, "reason": "a_memorix_initializing", "message": "A_Memorix 正在初始化"}));
    assert_eq!(starting.state, MaiBotMemoryState::Starting);
    let failed =
        runtime(json!({"success": true, "reason": "a_memorix_initialization_failed", "message": "A_Memorix 初始化失败: boom"}));
    assert_eq!((failed.state, failed.message.as_str()), (MaiBotMemoryState::Failed, "A_Memorix 初始化失败: boom"));
    let ready = runtime(json!({
        "success": true, "memory_enabled": true, "embedding_degraded": true,
        "vector_rebuild_required": true, "vector_rebuild_message": ""
    }));
    assert_eq!(ready.state, MaiBotMemoryState::Ready);
    assert_eq!(ready.notes.len(), 2);
}

#[test]
fn envelope_failures_become_errors_with_upstream_words() {
    let off = unwrap::<serde_json::Value>(json!({"success": false, "disabled": true, "error": "x"}), "/p").unwrap_err();
    assert!(off.to_string().contains("长期记忆没开"));
    let bad = unwrap::<serde_json::Value>(json!({"success": false, "error": "任务队列已满，请稍后重试"}), "/p").unwrap_err();
    assert_eq!(bad.to_string(), "任务队列已满，请稍后重试");
    assert!(unwrap::<serde_json::Value>(json!({"success": true}), "/p").is_ok());
}

#[test]
fn graph_keeps_the_best_connected_nodes() {
    // a 连着三条边最核心；e 孤零零、f 只连 a。留 3 个点：a 和度数 2 的 b、c（同度按名字）
    let raw = r#"{"success":true,"total_nodes":6,"total_edges":5,
        "nodes":[{"id":"e"},{"id":"f"},{"id":"a"},{"id":"b"},{"id":"c"},{"id":"d"}],
        "edges":[
            {"source":"a","target":"b","label":"认识","relation_count":1,"evidence_count":2},
            {"source":"a","target":"c","predicates":["喜欢","玩"],"relation_count":2},
            {"source":"b","target":"c","label":""},
            {"source":"a","target":"f"},
            {"source":"d","target":"d"}
        ]}"#;
    let g = pick_core_for_test(raw, 3);
    let ids: Vec<_> = g.nodes.iter().map(|n| (n.id.as_str(), n.degree)).collect();
    assert_eq!(ids, [("a", 3), ("b", 2), ("c", 2)]);
    assert_eq!(g.edges.len(), 3, "a-f 的 f 没留下；自环不要");
    assert_eq!(g.edges[1].label, "喜欢、玩");
    assert_eq!((g.total_nodes, g.total_edges), (6, 5));
}

#[tokio::test]
async fn local_texts_are_checked_before_import() {
    let dir = tempfile::tempdir().unwrap();
    let ok = dir.path().join("设定.md");
    std::fs::write(&ok, "\u{feff}# 麦麦的设定\n喜欢猫".as_bytes()).unwrap();
    let gbk = dir.path().join("旧.txt");
    std::fs::write(&gbk, [0xC4u8, 0xE3, 0xBA, 0xC3, 0xFF]).unwrap();
    let doc = dir.path().join("a.docx");
    std::fs::write(&doc, b"PK").unwrap();
    let empty = dir.path().join("空.txt");
    std::fs::write(&empty, b"").unwrap();
    let s = |p: &std::path::Path| p.to_string_lossy().into_owned();
    let got = inspect_local_texts(vec![s(&ok), s(&gbk), s(&doc), s(&empty), "rel/x.txt".into()]).await;
    let problems: Vec<_> = got.iter().map(|f| f.problem.as_deref()).collect();
    assert_eq!(
        problems,
        [None, Some("不是 UTF-8 编码，另存为 UTF-8 再导"), Some("只收 txt / md / json"), Some("是个空文件"), Some("路径不对")]
    );
}

fn client(server: &MockServer) -> MaiBotWebUi {
    MaiBotWebUi::connect(server.address().port(), "tok").unwrap()
}

fn options(kind: MaiBotMemoryImportKind, chat_id: &str) -> MaiBotMemoryImportOptions {
    MaiBotMemoryImportOptions { kind, chat_id: chat_id.into(), use_llm: true, force: false }
}

#[tokio::test]
async fn chat_log_paste_goes_as_narrative_scoped_to_the_chat() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/webui/memory/import/paste"))
        .and(body_partial_json(json!({
            "content": "小林：早\n麦麦：早呀",
            "name": "群聊.txt",
            "strategy_override": "narrative",
            "chat_log": true,
            "scope_type": "chat",
            "chat_id": "c1",
            "llm_enabled": true
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "task": {"task_id": "t1", "source": "paste", "status": "queued", "progress": 0, "file_count": 1, "created_at": 1.0}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let req = MaiBotMemoryImport::Paste {
        name: "群聊.txt".into(),
        content: "小林：早\n麦麦：早呀".into(),
        options: options(MaiBotMemoryImportKind::ChatLog, "c1"),
    };
    let task = import(&client(&server), &req).await.unwrap();
    assert_eq!((task.id.as_str(), task.status), ("t1", MaiBotMemoryTaskStatus::Queued));
}

#[tokio::test]
async fn upstream_rejections_come_back_as_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/webui/memory/import/paste"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"success": false, "error": "任务队列已满，请稍后重试"})))
        .mount(&server)
        .await;
    let req = MaiBotMemoryImport::Paste { name: String::new(), content: "x".into(), options: options(MaiBotMemoryImportKind::Auto, "") };
    let err = import(&client(&server), &req).await.unwrap_err();
    assert_eq!(err.to_string(), "任务队列已满，请稍后重试");
}

#[tokio::test]
async fn source_delete_preview_selects_by_source_name() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/webui/memory/delete/preview"))
        .and(body_partial_json(json!({"mode": "source", "selector": {"sources": ["设定.md"]}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "counts": {"paragraphs": 3, "relations": 2, "entities": 0, "sources": 1},
            "items": [{"item_type": "paragraph", "item_hash": "h1", "label": "设定.md", "preview": "麦麦喜欢猫"}],
            "item_count": 5
        })))
        .mount(&server)
        .await;
    let target = MaiBotMemoryDeleteTarget { kind: MaiBotMemoryDeleteKind::Source, ids: vec!["设定.md".into()] };
    let r = delete(&client(&server), &MaiBotMemoryDeleteAction::Preview { target }).await.unwrap();
    assert_eq!((r.counts.paragraphs, r.counts.relations), (3, 2));
    assert_eq!(r.samples[0].preview, "麦麦喜欢猫");
}

#[tokio::test]
async fn task_list_reads_summaries() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/webui/memory/import/tasks"))
        .and(query_param("limit", "30"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "items": [
                {"task_id": "t2", "source": "upload", "status": "extracting", "progress": 0.4, "total_chunks": 10,
                 "done_chunks": 4, "file_count": 2, "created_at": 2.0, "finished_at": null},
                {"task_id": "t1", "source": "paste", "status": "completed_with_errors", "progress": 1.5, "failed_chunks": 1,
                 "created_at": 1.0, "finished_at": 3.0},
                {"source": "paste"}
            ]
        })))
        .mount(&server)
        .await;
    let list = tasks(&client(&server)).await.unwrap();
    assert_eq!(list.len(), 2, "没有 task_id 的不给");
    assert_eq!(list[0].status, MaiBotMemoryTaskStatus::Running);
    assert_eq!((list[1].status, list[1].progress), (MaiBotMemoryTaskStatus::DoneWithErrors, 1.0));
}
