//! 往长期记忆里导资料：粘贴一段文字，或传本机的 txt / md / json。上游一次只跑一个导入任务，
//! 其余排队；任务表只在它内存里，重启就没了。内容和导过的一样会被跳过，勾「重复也导」才重导。

use std::path::Path;

use ncd_traits::AppFrameworkError;
use reqwest::Method;
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

use super::{BASE, call, check_id, unwrap};
use crate::maibot::resources::{MaiBotLearningChat, UpstreamChat, text};
use crate::maibot::webui_client::{MaiBotWebUi, Request};

/// 上游默认单文件 20 MB；这里按同样的数先在本机拦住，省得传一半被拒
const FILE_MAX_BYTES: u64 = 20 * 1024 * 1024;
const LOCAL_BATCH_MAX: usize = 200;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotMemoryImportKind {
    /// 让上游自己判断怎么切
    #[default]
    Auto,
    /// 故事、经历这类连着读的
    Narrative,
    /// 设定、资料这类一条条的
    Factual,
    /// 原话、语录
    Quote,
    /// 聊天记录：按叙述切，再按聊天记录抽人和时间
    ChatLog,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryImportOptions {
    pub kind: MaiBotMemoryImportKind,
    /// 空串是全局：哪个聊天里都能想起来
    pub chat_id: String,
    /// 用模型抽人物、事件和关系：更准，但慢、要花 token
    pub use_llm: bool,
    /// 和导过的内容一样也再导一遍
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "op", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotMemoryImport {
    Paste { name: String, content: String, options: MaiBotMemoryImportOptions },
    Files { paths: Vec<String>, options: MaiBotMemoryImportOptions },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryImportLimits {
    pub max_file_mb: u32,
    pub max_files: u32,
    pub max_paste_chars: u32,
    /// 上游建议的进度刷新间隔
    pub poll_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryImportSetup {
    pub limits: MaiBotMemoryImportLimits,
    /// 能挑的聊天，最近活跃的在前
    pub chats: Vec<MaiBotLearningChat>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotMemoryTaskStatus {
    Queued,
    Preparing,
    Running,
    Cancelling,
    Done,
    DoneWithErrors,
    Cancelled,
    Failed,
}

impl MaiBotMemoryTaskStatus {
    fn from_upstream(s: Option<&str>) -> Self {
        match s.unwrap_or("") {
            "queued" => Self::Queued,
            "preparing" => Self::Preparing,
            "cancel_requested" => Self::Cancelling,
            "completed" => Self::Done,
            "completed_with_errors" => Self::DoneWithErrors,
            "cancelled" => Self::Cancelled,
            "failed" => Self::Failed,
            // running / extracting / writing，以及以后新加的中间状态
            _ => Self::Running,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryTask {
    pub id: String,
    /// paste / upload；上游别的导入方式建的任务照原样给
    pub source: String,
    pub status: MaiBotMemoryTaskStatus,
    /// 0..1
    pub progress: f64,
    pub total_chunks: u32,
    pub done_chunks: u32,
    pub failed_chunks: u32,
    pub file_count: u32,
    pub error: String,
    /// 秒级时间戳
    pub created_at: f64,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryTaskFile {
    pub name: String,
    pub status: MaiBotMemoryTaskStatus,
    pub progress: f64,
    pub total_chunks: u32,
    pub done_chunks: u32,
    pub failed_chunks: u32,
    pub error: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryTaskDetail {
    pub task: MaiBotMemoryTask,
    pub files: Vec<MaiBotMemoryTaskFile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "op", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotMemoryTaskAction {
    Cancel { id: String },
    /// 只重跑失败的块，建一个新任务；参数沿用原任务的
    Retry { id: String },
}

/// 导入前在本机看一眼要导的文件
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotLocalTextFile {
    pub path: String,
    pub name: String,
    #[ts(type = "number")]
    pub size: u64,
    /// 有它就不能导
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamSettings {
    max_file_size_mb: Option<u32>,
    max_files_per_task: Option<u32>,
    max_paste_chars: Option<u32>,
    poll_interval_ms: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SettingsEnvelope {
    settings: Option<UpstreamSettings>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ChatsEnvelope {
    data: Option<Vec<UpstreamChat>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct UpstreamTask {
    task_id: Option<String>,
    source: Option<String>,
    status: Option<String>,
    progress: Option<f64>,
    total_chunks: Option<u32>,
    done_chunks: Option<u32>,
    failed_chunks: Option<u32>,
    file_count: Option<u32>,
    error: Option<String>,
    created_at: Option<f64>,
    finished_at: Option<f64>,
    files: Option<Vec<UpstreamTaskFile>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamTaskFile {
    name: Option<String>,
    status: Option<String>,
    progress: Option<f64>,
    total_chunks: Option<u32>,
    done_chunks: Option<u32>,
    failed_chunks: Option<u32>,
    error: Option<String>,
    warnings: Option<Vec<String>>,
}

impl UpstreamTask {
    pub(super) fn into_task(self) -> Option<MaiBotMemoryTask> {
        let files = self.files.as_ref().map_or(0, Vec::len) as u32;
        Some(MaiBotMemoryTask {
            id: self.task_id.filter(|s| !s.is_empty())?,
            source: text(self.source),
            status: MaiBotMemoryTaskStatus::from_upstream(self.status.as_deref()),
            progress: self.progress.unwrap_or(0.0).clamp(0.0, 1.0),
            total_chunks: self.total_chunks.unwrap_or(0),
            done_chunks: self.done_chunks.unwrap_or(0),
            failed_chunks: self.failed_chunks.unwrap_or(0),
            file_count: self.file_count.unwrap_or(files),
            error: text(self.error),
            created_at: self.created_at.unwrap_or(0.0),
            finished_at: self.finished_at,
        })
    }

    fn into_detail(mut self) -> Option<MaiBotMemoryTaskDetail> {
        let files = self
            .files
            .take()
            .unwrap_or_default()
            .into_iter()
            .map(|f| MaiBotMemoryTaskFile {
                name: text(f.name),
                status: MaiBotMemoryTaskStatus::from_upstream(f.status.as_deref()),
                progress: f.progress.unwrap_or(0.0).clamp(0.0, 1.0),
                total_chunks: f.total_chunks.unwrap_or(0),
                done_chunks: f.done_chunks.unwrap_or(0),
                failed_chunks: f.failed_chunks.unwrap_or(0),
                error: text(f.error),
                warnings: f.warnings.unwrap_or_default(),
            })
            .collect();
        Some(MaiBotMemoryTaskDetail { task: self.into_task()?, files })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct TaskEnvelope {
    task: Option<UpstreamTask>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct TasksEnvelope {
    items: Option<Vec<UpstreamTask>>,
}

pub(crate) async fn import_setup(c: &MaiBotWebUi) -> Result<MaiBotMemoryImportSetup, AppFrameworkError> {
    let s: SettingsEnvelope = call(c, Request::new(Method::GET, &format!("{BASE}/import/settings"))).await?;
    let s = s.settings.unwrap_or_default();
    // 聊天列表是普通的 FastAPI 回包，不带 A_Memorix 那层壳
    let chats: ChatsEnvelope = c.get(&format!("{BASE}/import/chat-targets"), &[]).await?;
    Ok(MaiBotMemoryImportSetup {
        limits: MaiBotMemoryImportLimits {
            max_file_mb: s.max_file_size_mb.unwrap_or(20),
            max_files: s.max_files_per_task.unwrap_or(200),
            max_paste_chars: s.max_paste_chars.unwrap_or(200_000),
            poll_ms: s.poll_interval_ms.unwrap_or(1000).max(500),
        },
        chats: chats.data.unwrap_or_default().into_iter().filter_map(UpstreamChat::into_chat).collect(),
    })
}

/// 上游页面把「聊天记录」映射成叙述切分加 chat_log，这里照做
fn options_json(o: &MaiBotMemoryImportOptions) -> serde_json::Value {
    let (strategy, chat_log) = match o.kind {
        MaiBotMemoryImportKind::Auto => ("auto", false),
        MaiBotMemoryImportKind::Narrative => ("narrative", false),
        MaiBotMemoryImportKind::Factual => ("factual", false),
        MaiBotMemoryImportKind::Quote => ("quote", false),
        MaiBotMemoryImportKind::ChatLog => ("narrative", true),
    };
    let chat_id = o.chat_id.trim();
    json!({
        "input_mode": "text",
        "strategy_override": strategy,
        "chat_log": chat_log,
        "llm_enabled": o.use_llm,
        "force": o.force,
        "dedupe_policy": "content_hash",
        "scope_type": if chat_id.is_empty() { "global" } else { "chat" },
        "chat_id": chat_id,
    })
}

pub(crate) async fn import(c: &MaiBotWebUi, req: &MaiBotMemoryImport) -> Result<MaiBotMemoryTask, AppFrameworkError> {
    let env: TaskEnvelope = match req {
        MaiBotMemoryImport::Paste { name, content, options } => {
            if content.trim().is_empty() {
                return Err(AppFrameworkError::Validation("没有要导的内容".into()));
            }
            let mut body = options_json(options);
            body["content"] = json!(content);
            if let Some(n) = Path::new(name.trim()).file_name().and_then(|n| n.to_str()).filter(|n| !n.is_empty()) {
                body["name"] = json!(n);
            }
            call(c, Request::new(Method::POST, &format!("{BASE}/import/paste")).body(&body).slow()).await?
        }
        MaiBotMemoryImport::Files { paths, options } => {
            let paths = paths.clone();
            let files = tokio::task::spawn_blocking(move || load_texts(&paths))
                .await
                .map_err(|e| AppFrameworkError::Integration(format!("读本机文件没读完：{e}")))??;
            let mut form = Form::new().text("payload_json", options_json(options).to_string());
            for f in files {
                let part = Part::bytes(f.bytes)
                    .file_name(f.name)
                    .mime_str(f.mime)
                    .map_err(|e| AppFrameworkError::Integration(e.to_string()))?;
                form = form.part("files", part);
            }
            let path = format!("{BASE}/import/upload");
            unwrap(c.send_form(&path, form).await?, &path)?
        }
    };
    env.task
        .and_then(UpstreamTask::into_task)
        .ok_or_else(|| AppFrameworkError::Integration("导入任务没建起来".into()))
}

pub(crate) async fn tasks(c: &MaiBotWebUi) -> Result<Vec<MaiBotMemoryTask>, AppFrameworkError> {
    let env: TasksEnvelope =
        call(c, Request::new(Method::GET, &format!("{BASE}/import/tasks")).query(&[("limit", "30")])).await?;
    Ok(env.items.unwrap_or_default().into_iter().filter_map(UpstreamTask::into_task).collect())
}

pub(crate) async fn task(c: &MaiBotWebUi, id: &str) -> Result<MaiBotMemoryTaskDetail, AppFrameworkError> {
    let id = check_id(id, "导入任务")?;
    let env: TaskEnvelope = call(c, Request::new(Method::GET, &format!("{BASE}/import/tasks/{id}"))).await?;
    env.task
        .and_then(UpstreamTask::into_detail)
        .ok_or_else(|| AppFrameworkError::Validation("这个导入任务没了（麦麦重启过就会清空）".into()))
}

pub(crate) async fn task_action(c: &MaiBotWebUi, a: &MaiBotMemoryTaskAction) -> Result<MaiBotMemoryTask, AppFrameworkError> {
    let (id, verb) = match a {
        MaiBotMemoryTaskAction::Cancel { id } => (id, "cancel"),
        MaiBotMemoryTaskAction::Retry { id } => (id, "retry"),
    };
    let id = check_id(id, "导入任务")?;
    // retry 不带参数：上游会拿原任务的参数重跑，带了就把原任务的设置改掉了
    let body = json!({});
    let env: TaskEnvelope =
        call(c, Request::new(Method::POST, &format!("{BASE}/import/tasks/{id}/{verb}")).body(&body)).await?;
    env.task
        .and_then(UpstreamTask::into_task)
        .ok_or_else(|| AppFrameworkError::Validation("这个导入任务没了（麦麦重启过就会清空）".into()))
}

/// 挑好、拖进来的文件先在本机过一遍，导不了的当场说
pub async fn inspect_local_texts(paths: Vec<String>) -> Vec<MaiBotLocalTextFile> {
    tokio::task::spawn_blocking(move || {
        dedupe(&paths)
            .into_iter()
            .map(|path| {
                let name = file_name(path);
                match read_text(path) {
                    Ok((size, _, _)) => MaiBotLocalTextFile { path: path.to_string(), name, size, problem: None },
                    Err((size, problem)) => MaiBotLocalTextFile { path: path.to_string(), name, size, problem: Some(problem) },
                }
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

struct LocalText {
    name: String,
    mime: &'static str,
    bytes: Vec<u8>,
}

/// 一个导入任务里的文件要么全能读，要么一个都不传：半截任务在上游不好收拾
fn load_texts(paths: &[String]) -> Result<Vec<LocalText>, AppFrameworkError> {
    let mut files = Vec::new();
    let mut bad = Vec::new();
    for path in dedupe(paths) {
        let name = file_name(path);
        match read_text(path) {
            Ok((_, mime, bytes)) => files.push(LocalText { name, mime, bytes }),
            Err((_, why)) => bad.push(format!("{name}（{why}）")),
        }
    }
    if !bad.is_empty() {
        return Err(AppFrameworkError::Validation(format!("这些文件导不了：{}", bad.join("；"))));
    }
    if files.is_empty() {
        return Err(AppFrameworkError::Validation("没挑文件".into()));
    }
    Ok(files)
}

fn dedupe(paths: &[String]) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    for p in paths.iter().map(|p| p.trim()).filter(|p| !p.is_empty()) {
        if !out.contains(&p) && out.len() < LOCAL_BATCH_MAX {
            out.push(p);
        }
    }
    out
}

fn file_name(path: &str) -> String {
    Path::new(path).file_name().map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned())
}

/// 只收本机绝对路径下的 txt / md / json，20 MB 以内，UTF-8 编码
fn read_text(path: &str) -> Result<(u64, &'static str, Vec<u8>), (u64, String)> {
    let p = Path::new(path);
    if !p.is_absolute() {
        return Err((0, "路径不对".into()));
    }
    let mime = match p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("txt") => "text/plain",
        Some("md") => "text/markdown",
        Some("json") => "application/json",
        _ => return Err((0, "只收 txt / md / json".into())),
    };
    let meta = std::fs::metadata(p).map_err(|_| (0, "读不到这个文件".to_string()))?;
    if !meta.is_file() {
        return Err((0, "不是文件".into()));
    }
    let size = meta.len();
    if size == 0 {
        return Err((0, "是个空文件".into()));
    }
    if size > FILE_MAX_BYTES {
        return Err((size, "超过 20 MB".into()));
    }
    let bytes = std::fs::read(p).map_err(|_| (size, "读不到这个文件".to_string()))?;
    // 上游按 UTF-8 读：记事本存的 GBK 文件导进去全是乱码，先拦下来
    let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    if std::str::from_utf8(body).is_err() {
        return Err((size, "不是 UTF-8 编码，另存为 UTF-8 再导".into()));
    }
    Ok((size, mime, bytes))
}
