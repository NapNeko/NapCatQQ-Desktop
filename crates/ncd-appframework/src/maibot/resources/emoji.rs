//! 麦麦的表情包，`/api/webui/emoji`。四种状态：收下（会发）、认识（看过、有标签、没收）、
//! 不认识（还没看出是什么）、丢弃（不发，也不再收集）。
//!
//! 标签就是上游的 description：麦麦按要回的情绪和各张的标签比，挑最像的发，没标签的永远挑不中。
//! 收下后要等下一轮表情包维护（`emoji.check_interval` 分钟）或重启才进发送池；丢弃、删掉立刻生效。

use std::path::Path;
use std::time::Duration;

use base64::Engine as _;
use ncd_traits::AppFrameworkError;
use reqwest::Method;
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

use super::{MaiBotResourceDone, UpstreamData, UpstreamMessage, UpstreamPage, text};
use crate::maibot::webui_client::{Fetched, MaiBotWebUi, Request};

const BASE: &str = "/api/webui/emoji";
/// 单张上限。上游不限，但太大的图 QQ 那边发不出去
const UPLOAD_MAX_BYTES: u64 = 10 * 1024 * 1024;
/// 上传前的本地预览只给不太大的：大 GIF 整个转 base64 过 IPC 太重
const PREVIEW_MAX_BYTES: u64 = 3 * 1024 * 1024;
/// 原图也按这个封顶，免得一张异常大的图把页面卡住
const ORIGINAL_MAX_BYTES: usize = 20 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotEmojiStatus {
    Adopted,
    Known,
    Unknown,
    Discarded,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotEmojiFilter {
    #[default]
    All,
    Adopted,
    Known,
    Unknown,
    Discarded,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotEmojiSort {
    /// 最近记下的（上传的也算）在前
    #[default]
    Newest,
    MostUsed,
    RecentlyUsed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotEmojiQuery {
    pub page: u32,
    pub page_size: u32,
    /// 搜标签（上游也匹配哈希）
    pub search: String,
    pub filter: MaiBotEmojiFilter,
    pub sort: MaiBotEmojiSort,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotEmoji {
    #[ts(type = "number")]
    pub id: i64,
    pub hash: String,
    /// 文件后缀：gif / png / webp …
    pub format: String,
    /// 情绪标签，麦麦按它挑
    pub tags: Vec<String>,
    pub status: MaiBotEmojiStatus,
    pub usage_count: u32,
    /// 以下都是秒级时间戳
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub found_at: Option<f64>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adopted_at: Option<f64>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotEmojiPage {
    pub total: u32,
    pub items: Vec<MaiBotEmoji>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotEmojiOverview {
    pub total: u32,
    pub adopted: u32,
    pub known: u32,
    pub unknown: u32,
    pub discarded: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "op", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotEmojiAction {
    /// 整组换掉；空的会让这张变回「不认识」
    Tag {
        #[ts(type = "number")]
        id: i64,
        tags: Vec<String>,
    },
    /// 开了内容审核时上游会先过一遍 VLM，没过的收不下
    Adopt {
        #[ts(type = "number[]")]
        ids: Vec<i64>,
    },
    /// 不发了，但不丢：回到「认识」
    Unadopt {
        #[ts(type = "number[]")]
        ids: Vec<i64>,
    },
    Discard {
        #[ts(type = "number[]")]
        ids: Vec<i64>,
    },
    /// 丢弃的捡回来，回到「认识」或「不认识」
    Restore {
        #[ts(type = "number[]")]
        ids: Vec<i64>,
    },
    Delete {
        #[ts(type = "number[]")]
        ids: Vec<i64>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotEmojiImage {
    /// `data:<mime>;base64,…`；文件被上游清理过时没有
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_url: Option<String>,
}

/// 上传前在本机看一眼：名字、大小、预览，传不了的说原因
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotLocalImage {
    pub path: String,
    pub name: String,
    #[ts(type = "number")]
    pub size: u64,
    /// 有它就不能传
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotEmojiUpload {
    /// 本机的图片路径（选文件、拖进来的）
    pub paths: Vec<String>,
    /// 这一批共用的情绪标签
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotUploadFailure {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotEmojiUploadDone {
    /// 新收下的
    pub uploaded: u32,
    /// 麦麦本来就有（同一张图）：重新收下，标签换成这次的
    pub existed: u32,
    pub failed: Vec<MaiBotUploadFailure>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamEmoji {
    id: Option<i64>,
    emoji_hash: Option<String>,
    format: Option<String>,
    description: Option<String>,
    emotion: Option<String>,
    usage_count: Option<u32>,
    is_registered: Option<bool>,
    is_banned: Option<bool>,
    status: Option<String>,
    record_time: Option<f64>,
    register_time: Option<f64>,
    last_used_time: Option<f64>,
}

impl UpstreamEmoji {
    fn into_item(self) -> Option<MaiBotEmoji> {
        let banned = self.is_banned.unwrap_or(false);
        let registered = self.is_registered.unwrap_or(false);
        let tags = normalize_tags(&[self.emotion.or(self.description).unwrap_or_default()]);
        let status = match self.status.as_deref() {
            Some("adopted") => MaiBotEmojiStatus::Adopted,
            Some("known") => MaiBotEmojiStatus::Known,
            Some("unknown") => MaiBotEmojiStatus::Unknown,
            Some("discarded") => MaiBotEmojiStatus::Discarded,
            // 老版本没有 status 字段，按上游同样的规则推
            _ if banned => MaiBotEmojiStatus::Discarded,
            _ if registered => MaiBotEmojiStatus::Adopted,
            _ if tags.is_empty() => MaiBotEmojiStatus::Unknown,
            _ => MaiBotEmojiStatus::Known,
        };
        Some(MaiBotEmoji {
            id: self.id?,
            hash: text(self.emoji_hash),
            format: text(self.format),
            tags,
            status,
            usage_count: self.usage_count.unwrap_or(0),
            // 上游没记时间的行给 0.0
            found_at: self.record_time.filter(|t| *t > 0.0),
            adopted_at: self.register_time,
            last_used: self.last_used_time,
        })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamStats {
    total: Option<u32>,
    adopted: Option<u32>,
    known: Option<u32>,
    unknown: Option<u32>,
    discarded: Option<u32>,
}

/// 上游拆标签用的是同一组分隔符（逗号、顿号、分号、空白），这里拆完去重
pub(crate) fn normalize_tags(raw: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for piece in raw.iter().flat_map(|s| {
        s.split(|c: char| matches!(c, ',' | '，' | '、' | ';' | '；') || c.is_whitespace())
    }) {
        let t = piece.trim();
        if !t.is_empty() && !out.iter().any(|x| x == t) {
            out.push(t.to_string());
        }
    }
    out
}

pub(crate) async fn list(
    c: &MaiBotWebUi,
    q: &MaiBotEmojiQuery,
) -> Result<MaiBotEmojiPage, AppFrameworkError> {
    let page = q.page.max(1).to_string();
    let size = q.page_size.clamp(1, 100).to_string();
    let search = q.search.trim();
    let sort_by = match q.sort {
        MaiBotEmojiSort::Newest => "record_time",
        MaiBotEmojiSort::MostUsed => "usage_count",
        MaiBotEmojiSort::RecentlyUsed => "last_used_time",
    };
    let mut query = vec![
        ("page", page.as_str()),
        ("page_size", size.as_str()),
        ("sort_by", sort_by),
        ("sort_order", "desc"),
    ];
    if !search.is_empty() {
        query.push(("search", search));
    }
    let status = match q.filter {
        MaiBotEmojiFilter::All => None,
        MaiBotEmojiFilter::Adopted => Some("adopted"),
        MaiBotEmojiFilter::Known => Some("known"),
        MaiBotEmojiFilter::Unknown => Some("unknown"),
        MaiBotEmojiFilter::Discarded => Some("discarded"),
    };
    if let Some(s) = status {
        query.push(("status", s));
    }
    let up: UpstreamPage<UpstreamEmoji> = c.get(&format!("{BASE}/list"), &query).await?;
    Ok(MaiBotEmojiPage {
        total: up.total.unwrap_or(0),
        items: up
            .data
            .unwrap_or_default()
            .into_iter()
            .filter_map(UpstreamEmoji::into_item)
            .collect(),
    })
}

pub(crate) async fn overview(c: &MaiBotWebUi) -> Result<MaiBotEmojiOverview, AppFrameworkError> {
    let stats: UpstreamData<UpstreamStats> = c.get(&format!("{BASE}/stats/summary"), &[]).await?;
    let s = stats.data.unwrap_or_default();
    Ok(MaiBotEmojiOverview {
        total: s.total.unwrap_or(0),
        adopted: s.adopted.unwrap_or(0),
        known: s.known.unwrap_or(0),
        unknown: s.unknown.unwrap_or(0),
        discarded: s.discarded.unwrap_or(0),
    })
}

pub(crate) async fn act(
    c: &MaiBotWebUi,
    a: &MaiBotEmojiAction,
) -> Result<MaiBotResourceDone, AppFrameworkError> {
    let patch = |body: serde_json::Value| {
        move |id: i64| (Method::PATCH, format!("{BASE}/{id}"), Some(body.clone()))
    };
    match a {
        MaiBotEmojiAction::Tag { id, tags } => {
            let body = json!({ "description": normalize_tags(tags).join(",") });
            let _: UpstreamMessage = c
                .call(Request::new(Method::PATCH, &format!("{BASE}/{id}")).body(&body))
                .await?;
            Ok(MaiBotResourceDone {
                affected: 1,
                message: "标签改好了".into(),
            })
        }
        MaiBotEmojiAction::Adopt { ids } => {
            each(c, ids, "收下", |id| {
                (Method::POST, format!("{BASE}/{id}/register"), None)
            })
            .await
        }
        MaiBotEmojiAction::Unadopt { ids } => {
            each(c, ids, "不再发", patch(json!({ "is_registered": false }))).await
        }
        MaiBotEmojiAction::Discard { ids } => {
            each(c, ids, "丢弃", |id| {
                (Method::POST, format!("{BASE}/{id}/ban"), None)
            })
            .await
        }
        MaiBotEmojiAction::Restore { ids } => {
            each(c, ids, "捡回", patch(json!({ "is_banned": false }))).await
        }
        MaiBotEmojiAction::Delete { ids } => match ids.as_slice() {
            [] => Ok(MaiBotResourceDone {
                affected: 0,
                message: String::new(),
            }),
            [id] => {
                let _: UpstreamMessage = c
                    .call(Request::new(Method::DELETE, &format!("{BASE}/{id}")))
                    .await?;
                Ok(MaiBotResourceDone {
                    affected: 1,
                    message: "删掉了".into(),
                })
            }
            _ => {
                let body = json!({ "emoji_ids": ids });
                let up: UpstreamMessage = c
                    .call(Request::new(Method::POST, &format!("{BASE}/batch/delete")).body(&body))
                    .await?;
                let n = up.deleted_count.unwrap_or(ids.len() as u32);
                Ok(MaiBotResourceDone {
                    affected: n,
                    message: format!("删掉了 {n} 张"),
                })
            }
        },
    }
}

/// 上游收下、丢弃这些都是一张一个请求。挨个发，错了的接着发；一张都没成就把第一个错报出去，
/// 部分成了在结果里说清几张没成、为什么
async fn each(
    c: &MaiBotWebUi,
    ids: &[i64],
    verb: &str,
    build: impl Fn(i64) -> (Method, String, Option<serde_json::Value>),
) -> Result<MaiBotResourceDone, AppFrameworkError> {
    let mut ok = 0u32;
    let mut first_err: Option<AppFrameworkError> = None;
    for &id in ids {
        let (method, path, body) = build(id);
        let mut req = Request::new(method, &path);
        if let Some(b) = &body {
            req = req.body(b);
        }
        match c.call::<UpstreamMessage>(req).await {
            Ok(_) => ok += 1,
            Err(e) => {
                first_err.get_or_insert(e);
            }
        }
    }
    let failed = ids.len() as u32 - ok;
    match first_err {
        Some(e) if ok == 0 => Err(e),
        Some(e) => Ok(MaiBotResourceDone {
            affected: ok,
            message: format!("{verb}了 {ok} 张，{failed} 张没成：{}", reason(&e)),
        }),
        None => Ok(MaiBotResourceDone {
            affected: ok,
            message: format!("{verb}了 {ok} 张"),
        }),
    }
}

/// 给人看的原因：去掉错误类型自带的前缀
fn reason(e: &AppFrameworkError) -> String {
    match e {
        AppFrameworkError::Integration(s) | AppFrameworkError::Validation(s) => s.clone(),
        other => other.to_string(),
    }
}

/// 缩略图第一次要时上游才开始生成，回 202 叫隔一秒再来；这里等几轮再放弃
pub(crate) async fn image(
    c: &MaiBotWebUi,
    id: i64,
    original: bool,
) -> Result<MaiBotEmojiImage, AppFrameworkError> {
    let path = format!("{BASE}/{id}/thumbnail");
    let query: &[(&str, &str)] = if original {
        &[("original", "true")]
    } else {
        &[]
    };
    for attempt in 0..6u64 {
        match c.fetch_bytes(&path, query).await? {
            Fetched::Ready { mime, bytes } => {
                if bytes.len() > ORIGINAL_MAX_BYTES {
                    return Err(AppFrameworkError::Validation(
                        "这张图太大，桌面端不显示".into(),
                    ));
                }
                let mime = sniff_image(&bytes).map_or(mime, str::to_string);
                return Ok(MaiBotEmojiImage {
                    data_url: Some(data_url(&mime, &bytes)),
                });
            }
            Fetched::Missing => return Ok(MaiBotEmojiImage { data_url: None }),
            Fetched::Pending => {
                tokio::time::sleep(Duration::from_millis(400 + 200 * attempt)).await
            }
        }
    }
    Err(AppFrameworkError::Integration(
        "缩略图还没生成好，过会儿再看".into(),
    ))
}

/// 一张一张传：上游的批量接口不收每张各自的标签，出错也只给文件名
pub(crate) async fn upload(
    c: &MaiBotWebUi,
    up: &MaiBotEmojiUpload,
) -> Result<MaiBotEmojiUploadDone, AppFrameworkError> {
    let paths = up.paths.clone();
    let (files, mut failed) = tokio::task::spawn_blocking(move || load_uploads(&paths))
        .await
        .map_err(|e| AppFrameworkError::Integration(format!("读本机图片没读完：{e}")))?;
    let tags = normalize_tags(&up.tags).join(",");
    let (mut uploaded, mut existed) = (0u32, 0u32);
    for f in files {
        let part = Part::bytes(f.bytes)
            .file_name(f.name.clone())
            .mime_str(f.mime)
            .map_err(|e| AppFrameworkError::Integration(e.to_string()))?;
        let form = Form::new()
            .part("file", part)
            .text("description", tags.clone())
            .text("is_registered", "true");
        match c.send_form(&format!("{BASE}/upload"), form).await {
            Ok(v) => {
                let msg: UpstreamMessage = serde_json::from_value(v).unwrap_or_default();
                // 同一张图上游已经有了：它会重新收下并换上这次的标签，回话里带「已存在」
                if msg.message.as_deref().is_some_and(|m| m.contains("已存在")) {
                    existed += 1;
                } else {
                    uploaded += 1;
                }
            }
            Err(e) => failed.push(MaiBotUploadFailure {
                name: f.name,
                reason: reason(&e),
            }),
        }
    }
    Ok(MaiBotEmojiUploadDone {
        uploaded,
        existed,
        failed,
    })
}

/// 选好、拖进来的图先在本机过一遍：传不了的当场说，能传的给预览
pub async fn inspect_local_images(paths: Vec<String>) -> Vec<MaiBotLocalImage> {
    tokio::task::spawn_blocking(move || dedupe(&paths).into_iter().map(inspect_one).collect())
        .await
        .unwrap_or_default()
}

/// 一次最多看这么多张，拖进来一整个大文件夹也不至于卡住
const LOCAL_BATCH_MAX: usize = 100;

fn dedupe(paths: &[String]) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    for p in paths.iter().map(|p| p.trim()).filter(|p| !p.is_empty()) {
        if !out.contains(&p) && out.len() < LOCAL_BATCH_MAX {
            out.push(p);
        }
    }
    out
}

fn inspect_one(path: &str) -> MaiBotLocalImage {
    let name = file_name(path);
    match read_local(path) {
        Ok((size, mime, bytes)) => MaiBotLocalImage {
            path: path.to_string(),
            name,
            size,
            problem: None,
            preview: (size <= PREVIEW_MAX_BYTES).then(|| data_url(mime, &bytes)),
        },
        Err((size, problem)) => MaiBotLocalImage {
            path: path.to_string(),
            name,
            size,
            problem: Some(problem),
            preview: None,
        },
    }
}

struct LocalFile {
    name: String,
    mime: &'static str,
    bytes: Vec<u8>,
}

fn load_uploads(paths: &[String]) -> (Vec<LocalFile>, Vec<MaiBotUploadFailure>) {
    let (mut files, mut failed) = (Vec::new(), Vec::new());
    for path in dedupe(paths) {
        let name = file_name(path);
        match read_local(path) {
            Ok((_, mime, bytes)) => files.push(LocalFile { name, mime, bytes }),
            Err((_, reason)) => failed.push(MaiBotUploadFailure { name, reason }),
        }
    }
    (files, failed)
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned())
}

/// 只收本机绝对路径下的普通文件，10 MB 以内，按文件头认得出是 PNG / JPG / GIF / WebP
/// （上游要 multipart 里的 MIME 正好是这四种之一，扩展名不可信）
fn read_local(path: &str) -> Result<(u64, &'static str, Vec<u8>), (u64, String)> {
    let p = Path::new(path);
    if !p.is_absolute() {
        return Err((0, "路径不对".into()));
    }
    let meta = std::fs::metadata(p).map_err(|_| (0, "读不到这个文件".to_string()))?;
    if !meta.is_file() {
        return Err((0, "不是文件".into()));
    }
    let size = meta.len();
    if size == 0 {
        return Err((0, "是个空文件".into()));
    }
    if size > UPLOAD_MAX_BYTES {
        return Err((size, "超过 10 MB".into()));
    }
    let bytes = std::fs::read(p).map_err(|_| (size, "读不到这个文件".to_string()))?;
    let mime = sniff_image(&bytes).ok_or((size, "不是 PNG / JPG / GIF / WebP 图片".to_string()))?;
    Ok((size, mime, bytes))
}

pub(crate) fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

fn data_url(mime: &str, bytes: &[u8]) -> String {
    let mime = if mime.is_empty() {
        "application/octet-stream"
    } else {
        mime
    };
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

#[cfg(test)]
#[path = "emoji_tests.rs"]
mod tests;
