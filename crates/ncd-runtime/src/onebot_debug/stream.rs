//! 流式接口的调用编排：把本机文件分块传到 Bot 一侧（`upload_file_stream` 本体，或作为
//! 其它动作文件参数的预置步骤）、把 Bot 一侧的文件分块收到本机。全程一拍一拍推进度、
//! 随时可取消。
//!
//! 分块传输只走内部通道（NapCat WebUI 适配器 / SnowLuma 调试接口）：用户自己的
//! HTTP / WS 通道给「该通道不支持」，不硬发。预置文件传输只是文件参数的一步，不产生
//! 历史；真正调目标动作的那笔走 `call_with_token`，历史 / 事件流 / 通道状态与普通调用
//! 完全一致。上传和下载整笔自成一条记录，回包取完成帧。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use futures_util::{StreamExt as _, stream};
use ncd_domain::bot_config::BackendType;
use ncd_domain::onebot_debug::{
    DEBUG_STREAM_VERSION, DebugCallRequest, DebugCallResponse, DebugCallResult, DebugChannelId,
    DebugError, DebugHost, DebugLocalFile, DebugStreamCallRequest, DebugStreamProgress,
    DebugStreamStage, LOCAL_FILE_TOKEN_PREFIX,
};
use ncd_onebot::client::{RawCall, SseParser, connect_ws, outcome_from, parse_ob11_reply};
use serde_json::{Map as JsonMap, Value, json};
use sha2::Digest as _;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::calls::{
    CallScope, ChannelResult, InflightGuard, InternalClient, effective_timeout, normalize_params,
};
use super::errors::{Failure, duration_ms, from_client, from_napcat, from_snowluma};
use super::port::DebugBotView;
use super::session::INTERNAL_KEY;
use super::{DebugManager, Epoch, debug_host};

/// 流式调用的进度出口。一拍 `false` 表示对面（调试台的窗口）已经走了，本次调用直接收手
pub trait DebugStreamSink: Send + Sync {
    fn send(&self, progress: &DebugStreamProgress) -> bool;
}

/// 分块协议的固定动作名。不走目录：目录合并前这些路由判断就要成立
const UPLOAD_ACTION: &str = "upload_file_stream";
/// 「一次请求、多帧回答」的下载动作。`clean_stream_temp_file` 是普通的单帧调用，不在列
const DOWNLOAD_ACTIONS: &[&str] = &[
    "download_file_stream",
    "download_file_image_stream",
    "download_file_record_stream",
    "test_download_stream",
];
/// 一次分块的原始字节数：base64 膨胀 1/3 后每帧约 680 KiB，HTTP / WS 都吃得下
const CHUNK_BYTES: usize = 512 * 1024;
/// 本机文件上限：再大就不是调试台试接口的场景了，走桌面端自己的文件传输渠道
const MAX_LOCAL_FILE_BYTES: u64 = 256 * 1024 * 1024;
/// 分块下载的收拢上限，和上游（SnowLuma / NapCat 流式接口）自己的 4 GiB 对齐
const MAX_DOWNLOAD_BYTES: u64 = 4 * 1024 * 1024 * 1024;
/// 传完没下文的合并文件在 Bot 一侧留多久：照旧默认 5 分钟，够紧接着的调用引用
const PUSH_RETENTION_MS: u64 = 300_000;
/// 上传中途断掉时通知上游回收流的那次 reset 调用：无视刚把调用掐掉的取消令牌，单独给时限
const RESET_TIMEOUT: Duration = Duration::from_secs(5);
/// 不支持分块传输的通道上给的理由
const STREAM_CHANNEL_REASON: &str = "该通道不支持流式接口；分块传输只在内部通道上支持";
/// 本机文件的占位标记和路径对不上的提示
const TOKEN_MISMATCH_REASON: &str = "本机文件的占位标记和路径对不上";
/// 下载物在本机落盘时的兜底文件名
const DOWNLOAD_FALLBACK_NAME: &str = "download.bin";

enum Route<'a> {
    /// 没有本机文件也不是下载：手填分块参数、`clean_stream_temp_file` 等普通单帧调用
    Plain,
    /// 动作本体就是上传：一个本机文件，按客户端驱动的分块协议传完
    StandaloneUpload(&'a DebugLocalFile),
    /// 先把本机文件送到 Bot 一侧、把占位参数换成那边路径，再发起真正的调用
    PushThenCall(&'a [DebugLocalFile]),
    /// 一次请求、多帧回答：帧里的分块收到本机拼成文件
    Download,
}

fn stream_route(req: &DebugStreamCallRequest) -> Result<Route<'_>, DebugError> {
    let is_download = DOWNLOAD_ACTIONS.contains(&req.action.as_str());
    match (UPLOAD_ACTION == req.action.as_str(), is_download, req.local_files.len()) {
        (true, false, 1) => Ok(Route::StandaloneUpload(&req.local_files[0])),
        // 不带本机文件的 upload_file_stream 不拦：手填分块参数走普通单帧调用（落到下面的 Plain）
        (true, false, n) if n > 1 => Err(invalid("upload_file_stream 只能带一个本机文件")),
        (_, true, n) if n > 0 => Err(invalid(
            "下载动作的参数是 Bot 一侧的地址（URL / 文件 ID / 路径），不带本机文件",
        )),
        (_, false, n) if n > 0 => Ok(Route::PushThenCall(&req.local_files)),
        (_, true, _) => Ok(Route::Download),
        (_, false, _) => Ok(Route::Plain),
    }
}

fn invalid(message: &str) -> DebugError {
    DebugError::InvalidParams {
        message: message.to_owned(),
    }
}

fn unavailable() -> DebugError {
    DebugError::ChannelUnavailable {
        reason: STREAM_CHANNEL_REASON.to_owned(),
    }
}

struct LocalFileMeta {
    size: u64,
    sha256: String,
    total_chunks: u32,
}

/// 整笔上传的收场
enum UploadOutcome {
    /// file_complete 帧的 data
    Complete(Value),
    /// 要当整笔结果记下来的样子（拿到失败回包是 `Ok`，自己掐断 / 坏了是 `Err`）
    Failed(DebugCallResult),
}

/// 一拍进度的来源信息：request_id + 出口 + 时限边界，散落各处不用个个传
struct Progress<'a> {
    request_id: &'a str,
    sink: &'a Arc<dyn DebugStreamSink>,
    scope: &'a CallScope,
}

impl Progress<'_> {
    fn send(
        &self,
        stage: DebugStreamStage,
        file_name: &str,
        done_bytes: u64,
        total_bytes: Option<u64>,
        done_chunks: u32,
        total_chunks: Option<u32>,
    ) -> Result<(), DebugError> {
        let ok = self.sink.send(&DebugStreamProgress {
            v: DEBUG_STREAM_VERSION,
            request_id: self.request_id.to_owned(),
            stage,
            file_name: file_name.to_owned(),
            done_bytes,
            total_bytes,
            done_chunks,
            total_chunks,
        });
        // 对面（窗口）走了，继续传已经没有看头
        if ok {
            Ok(())
        } else {
            Err(DebugError::Cancelled)
        }
    }

    fn remaining(&self) -> Duration {
        self.scope.deadline.saturating_duration_since(Instant::now())
    }
}

impl DebugManager {
    /// 发一次流式调用。和 `call` 共用同一份 inflight 登记，`onebot_debug_cancel` 同样收得停
    pub async fn call_stream(
        &self,
        req: DebugStreamCallRequest,
        sink: Arc<dyn DebugStreamSink>,
    ) -> DebugCallResponse {
        let token = CancellationToken::new();
        let result = match InflightGuard::register(self, &req.request_id, token.clone()) {
            Err(error) => DebugCallResult::Err { error },
            Ok(_inflight) => {
                let timeout = effective_timeout(req.timeout_ms);
                let scope = CallScope {
                    epoch: self.epoch(),
                    token,
                    deadline: Instant::now() + timeout,
                    timeout,
                    request_id: req.request_id.clone(),
                };
                self.run_stream_call(&req, &sink, &scope).await
            }
        };
        DebugCallResponse {
            request_id: req.request_id,
            result,
        }
    }

    async fn run_stream_call(
        &self,
        req: &DebugStreamCallRequest,
        sink: &Arc<dyn DebugStreamSink>,
        scope: &CallScope,
    ) -> DebugCallResult {
        let plain = plain_request(req, req.params.clone());
        let view = match self.guarded(scope, self.bot_for_call(&plain)).await {
            Ok(Ok(view)) => view,
            Ok(Err(error)) | Err(error) => return DebugCallResult::Err { error },
        };
        if !view.running() {
            return DebugCallResult::Err {
                error: DebugError::BotNotRunning,
            };
        }
        let mut params = match normalize_params(&req.params) {
            Ok(params) => params,
            Err(error) => return DebugCallResult::Err { error },
        };
        let route = match stream_route(req) {
            Ok(route) => route,
            Err(error) => return DebugCallResult::Err { error },
        };

        match route {
            Route::Plain => {
                let response = self.call_with_token(&plain, scope.token.clone()).await;
                response.result
            }
            Route::StandaloneUpload(local) => {
                self.upload_stream_call(&view, &plain, &params, local, sink, scope)
                    .await
            }
            Route::PushThenCall(files) => {
                for local in files {
                    if let Err(error) = check_token(local) {
                        return DebugCallResult::Err { error };
                    }
                    match self.push_local_file(&view, local, sink, scope).await {
                        Ok(remote_path) => {
                            if substitute_token(&mut params, &local.token, &remote_path) == 0 {
                                return DebugCallResult::Err {
                                    error: invalid("本机文件的占位标记没有出现在参数里"),
                                };
                            }
                        }
                        Err(result) => {
                            // 真正的那笔没发出去，但整笔在预置那步失败了：照常进历史
                            self.record_call(
                                &view,
                                &plain,
                                &DebugChannelId::Internal,
                                &result,
                                &scope.epoch,
                            );
                            return result;
                        }
                    }
                }
                // 传输吃掉了一段时间：真正的动作拿剩下的时限去等，不吃满整份超时
                let mut call = plain_request(req, params);
                call.timeout_ms = Some(remaining_ms(scope));
                let response = self.call_with_token(&call, scope.token.clone()).await;
                response.result
            }
            Route::Download => {
                self.download_stream_call(&view, &plain, &params, sink, scope)
                    .await
            }
        }
    }

    /// 一次传输要不要从「这条通道不行」就停住：点名的 HTTP / WS 直接拒；「自动」
    /// 落在内部通道上才放行（内部太老时自动绕道，也没有分块路可走）
    async fn require_internal(
        &self,
        view: &DebugBotView,
        requested: &DebugChannelId,
    ) -> Option<DebugError> {
        match requested {
            DebugChannelId::Internal => None,
            DebugChannelId::Auto => self.refresh_session(view).await.then(unavailable),
            DebugChannelId::Http { .. } | DebugChannelId::Ws { .. } => Some(unavailable()),
        }
    }

    /// 记一次内部通道的状态：传输走完了记可用，失败若真说明通道状况则按传来的记
    async fn note_internal(&self, view: &DebugBotView, failure: Option<&Failure>, epoch: &Epoch) {
        let result = match failure {
            None => ChannelResult::Worked,
            Some(failure) => match &failure.status {
                Some(status) => ChannelResult::Failed(status.clone()),
                None => return,
            },
        };
        self.apply_status(view, INTERNAL_KEY, result, epoch).await;
    }

    // -------------------------------------------------------------------
    // 预置：把本机文件送到 Bot 一侧，返回那边认的路径
    // -------------------------------------------------------------------

    async fn push_local_file(
        &self,
        view: &DebugBotView,
        local: &DebugLocalFile,
        sink: &Arc<dyn DebugStreamSink>,
        scope: &CallScope,
    ) -> Result<String, DebugCallResult> {
        // 本机 Bot：主机同一台，进程直接读绝对路径，传传反而多一道
        if matches!(debug_host(&view.config), DebugHost::Local) {
            return Ok(local.path.clone());
        }
        match self.ensure_internal(view, &scope.epoch).await {
            Ok(InternalClient::SnowLuma(client)) => match self
                .snowluma_push(&client, local, sink, scope)
                .await
            {
                Ok(path) => {
                    self.note_internal(view, None, &scope.epoch).await;
                    Ok(path)
                }
                Err(failure) => {
                    self.note_internal(view, Some(&failure), &scope.epoch).await;
                    Err(DebugCallResult::Err {
                        error: failure.error,
                    })
                }
            },
            Ok(InternalClient::NapCat(_)) => {
                let base = json!({ "file_retention": PUSH_RETENTION_MS });
                match self
                    .run_upload_chunks(view, &scope.request_id, local, &base, sink, scope)
                    .await
                {
                    UploadOutcome::Complete(data) => {
                        match data.get("file_path").and_then(Value::as_str) {
                            Some(path) => Ok(path.to_owned()),
                            None => Err(DebugCallResult::Err {
                                error: invalid("上传完成帧里没有 file_path 字段"),
                            }),
                        }
                    }
                    UploadOutcome::Failed(result) => Err(result),
                }
            }
            Err(failure) => {
                self.note_internal(view, Some(&failure), &scope.epoch).await;
                Err(DebugCallResult::Err {
                    error: failure.error,
                })
            }
        }
    }

    /// SnowLuma 的文件预置走 `/api/debug/upload`：单趟原始字节，不进 OneBot 调用流，
    /// 事件流也不会被分块回放刷一屏
    async fn snowluma_push(
        &self,
        client: &Arc<ncd_backend_snowluma::SnowLumaDebugClient>,
        local: &DebugLocalFile,
        sink: &Arc<dyn DebugStreamSink>,
        scope: &CallScope,
    ) -> Result<String, Failure> {
        let meta = self.read_local_file(local, sink, scope).await?;
        let name = file_name_of(&local.path);
        let file = tokio::fs::File::open(&local.path)
            .await
            .map_err(failure_invalid)?;
        // 进度闭包要随请求体一起活：自持出口和 request_id，不借 scope。
        // 取消由外面 guarded 把整趟丢弃实现，到那时请求体连同闭包一起没了
        let done = Arc::new(AtomicU64::new(0));
        let reporter = {
            let sink = Arc::clone(sink);
            let done = Arc::clone(&done);
            let name = name.clone();
            let request_id = scope.request_id.clone();
            move |n: u64| {
                let now = done.fetch_add(n, AtomicOrdering::Relaxed) + n;
                let _ = sink.send(&DebugStreamProgress {
                    v: DEBUG_STREAM_VERSION,
                    request_id: request_id.clone(),
                    stage: DebugStreamStage::Uploading,
                    file_name: name.clone(),
                    done_bytes: now,
                    total_bytes: Some(meta.size),
                    done_chunks: 0,
                    total_chunks: None,
                });
            }
        };
        let body = reqwest::Body::wrap_stream(file_body(file, reporter));
        let upload = self
            .guarded(scope, client.upload_file(&name, body))
            .await
            .map_err(Failure::new)?
            .map_err(|e| from_snowluma(e, scope.timeout))?;
        Ok(upload.path)
    }

    // -------------------------------------------------------------------
    // `upload_file_stream` 本体：按分块协议传一个本机文件
    // -------------------------------------------------------------------

    async fn upload_stream_call(
        &self,
        view: &DebugBotView,
        plain: &DebugCallRequest,
        base: &Value,
        local: &DebugLocalFile,
        sink: &Arc<dyn DebugStreamSink>,
        scope: &CallScope,
    ) -> DebugCallResult {
        let started = Instant::now();
        if let Some(error) = self.require_internal(view, &plain.channel).await {
            let result = DebugCallResult::Err { error };
            self.record_call(view, plain, &DebugChannelId::Internal, &result, &scope.epoch);
            return result;
        }
        match self
            .run_upload_chunks(view, &plain.request_id, local, base, sink, scope)
            .await
        {
            UploadOutcome::Complete(data) => {
                // 完成帧就是这次调用的回包：长成普通回包的样子交上去
                let raw = json!({
                    "status": "ok", "retcode": 0, "data": data,
                    "message": "", "wording": "",
                });
                let text = raw.to_string();
                let outcome = outcome_from(&text, raw, started.elapsed(), DebugChannelId::Internal);
                let result = DebugCallResult::Ok { outcome };
                self.record_call(view, plain, &DebugChannelId::Internal, &result, &scope.epoch);
                result
            }
            UploadOutcome::Failed(result) => {
                self.record_call(view, plain, &DebugChannelId::Internal, &result, &scope.epoch);
                result
            }
        }
    }

    /// 分块循环：两家的 `upload_file_stream` 都是客户端逐块调同一个动作，每次一次
    /// 普通请求 / 回包，最后再来一次不携块的 `is_complete` 合并出文件
    async fn run_upload_chunks(
        &self,
        view: &DebugBotView,
        request_id: &str,
        local: &DebugLocalFile,
        base: &Value,
        sink: &Arc<dyn DebugStreamSink>,
        scope: &CallScope,
    ) -> UploadOutcome {
        let progress = Progress {
            request_id,
            sink,
            scope,
        };
        let meta = match self.read_local_file(local, sink, scope).await {
            Ok(meta) => meta,
            Err(failure) => {
                self.note_internal(view, Some(&failure), &scope.epoch).await;
                return UploadOutcome::Failed(DebugCallResult::Err {
                    error: failure.error,
                });
            }
        };
        let name = file_name_of(&local.path);
        if let Err(error) = progress.send(
            DebugStreamStage::Uploading,
            &name,
            0,
            Some(meta.size),
            0,
            Some(meta.total_chunks),
        ) {
            return UploadOutcome::Failed(DebugCallResult::Err { error });
        }
        let stream_id = Uuid::new_v4().to_string();
        let mut file = match tokio::fs::File::open(&local.path).await {
            Ok(file) => file,
            Err(e) => {
                return UploadOutcome::Failed(DebugCallResult::Err {
                    error: invalid(&format!("打不开本机文件：{e}")),
                })
            }
        };
        let mut sent_bytes = 0_u64;
        for index in 0..meta.total_chunks {
            let take = (meta.size - sent_bytes).min(CHUNK_BYTES as u64) as usize;
            let mut buf = vec![0_u8; take];
            if let Err(e) = file.read_exact(&mut buf).await {
                self.reset_upload(view, &stream_id, scope).await;
                return UploadOutcome::Failed(DebugCallResult::Err {
                    error: DebugError::Transport {
                        message: format!("读本机文件失败：{e}"),
                    },
                });
            }
            let Value::Object(mut params) = upload_base(base, &name, &meta, &stream_id) else {
                unreachable!("upload_base 恒为对象");
            };
            params.insert("chunk_index".to_owned(), json!(index));
            params.insert("chunk_data".to_owned(), json!(B64.encode(&buf)));
            let params = Value::Object(params);
            if is_snowluma(view) {
                // SnowLuma 的事件流回放每次 invoke：登记自己的块，不然分块在聊天里刷一屏
                self.note_own_call(&view.bot_id(), UPLOAD_ACTION, &params);
            }
            let call = self.call_internal(
                view,
                UPLOAD_ACTION,
                &params,
                progress.remaining(),
                scope.timeout,
                &scope.epoch,
            );
            let (raw, _) = match self.guarded(scope, call).await {
                Ok(Ok(pair)) => pair,
                Ok(Err(failure)) => {
                    self.note_internal(view, Some(&failure), &scope.epoch).await;
                    self.reset_upload(view, &stream_id, scope).await;
                    return UploadOutcome::Failed(DebugCallResult::Err {
                        error: failure.error,
                    });
                }
                Err(error) => {
                    self.reset_upload(view, &stream_id, scope).await;
                    return UploadOutcome::Failed(DebugCallResult::Err { error });
                }
            };
            if !chunk_ack_ok(&raw) {
                self.reset_upload(view, &stream_id, scope).await;
                return UploadOutcome::Failed(frame_outcome(raw, started_for_chunks(&scope)));
            }
            sent_bytes += buf.len() as u64;
            if let Err(error) = progress.send(
                DebugStreamStage::Uploading,
                &name,
                sent_bytes,
                Some(meta.size),
                index + 1,
                Some(meta.total_chunks),
            ) {
                self.reset_upload(view, &stream_id, scope).await;
                return UploadOutcome::Failed(DebugCallResult::Err { error });
            }
        }
        // 完成调用：不携块、只给确认。上游合并之后那份回包的 data 里有 file_path
        let Value::Object(mut params) = upload_base(base, &name, &meta, &stream_id) else {
            unreachable!("upload_base 恒为对象");
        };
        params.insert("is_complete".to_owned(), Value::Bool(true));
        let params = Value::Object(params);
        if is_snowluma(view) {
            self.note_own_call(&view.bot_id(), UPLOAD_ACTION, &params);
        }
        let call = self.call_internal(
            view,
            UPLOAD_ACTION,
            &params,
            progress.remaining(),
            scope.timeout,
            &scope.epoch,
        );
        let (raw, _) = match self.guarded(scope, call).await {
            Ok(Ok(pair)) => pair,
            Ok(Err(failure)) => {
                self.note_internal(view, Some(&failure), &scope.epoch).await;
                self.reset_upload(view, &stream_id, scope).await;
                return UploadOutcome::Failed(DebugCallResult::Err {
                    error: failure.error,
                });
            }
            Err(error) => {
                self.reset_upload(view, &stream_id, scope).await;
                return UploadOutcome::Failed(DebugCallResult::Err { error });
            }
        };
        let reply = parse_ob11_reply(&raw.value);
        let ok = reply.retcode == 0 && !reply.status.eq_ignore_ascii_case("failed");
        if !ok {
            self.reset_upload(view, &stream_id, scope).await;
            return UploadOutcome::Failed(frame_outcome(raw, started_for_chunks(&scope)));
        }
        match reply.data.get("status").and_then(Value::as_str) {
            Some("file_complete") => {
                self.note_internal(view, None, &scope.epoch).await;
                UploadOutcome::Complete(reply.data)
            }
            // 按协议这里必是 file_complete；不是就当协议变了，明说而不猜
            _ => UploadOutcome::Failed(DebugCallResult::Err {
                error: invalid("上传完成帧不是 file_complete"),
            }),
        }
    }

    /// 中途断掉时告诉上游把这条流回收掉（不然分块和挂着的状态会白占 10 分钟）
    async fn reset_upload(&self, view: &DebugBotView, stream_id: &str, scope: &CallScope) {
        let body = json!({ "stream_id": stream_id, "reset": true, "file_retention": 0 });
        let call = self.call_internal(
            view,
            UPLOAD_ACTION,
            &body,
            RESET_TIMEOUT,
            RESET_TIMEOUT,
            &scope.epoch,
        );
        let _ = tokio::time::timeout(RESET_TIMEOUT, call).await;
    }

    // -------------------------------------------------------------------
    // 下载动作：一次请求多帧回答，帧里的分块收拢成本机文件
    // -------------------------------------------------------------------

    async fn download_stream_call(
        &self,
        view: &DebugBotView,
        plain: &DebugCallRequest,
        params: &Value,
        sink: &Arc<dyn DebugStreamSink>,
        scope: &CallScope,
    ) -> DebugCallResult {
        let started = Instant::now();
        if let Some(error) = self.require_internal(view, &plain.channel).await {
            let result = DebugCallResult::Err { error };
            self.record_call(view, plain, &DebugChannelId::Internal, &result, &scope.epoch);
            return result;
        }
        let mut collector = match Collector::new(
            self.store.downloads_dir(),
            plain.request_id.clone(),
            &plain.action,
        )
        .await
        {
            Ok(collector) => collector,
            Err(error) => return DebugCallResult::Err { error },
        };
        let outcome = match self.ensure_internal(view, &scope.epoch).await {
            Ok(InternalClient::NapCat(client)) => {
                self.napcat_download(&client, plain, params, &mut collector, sink, scope)
                    .await
            }
            Ok(InternalClient::SnowLuma(client)) => {
                self.snowluma_download(view, &client, plain, params, &mut collector, sink, scope)
                    .await
            }
            Err(failure) => {
                self.note_internal(view, Some(&failure), &scope.epoch).await;
                Err(failure.error)
            }
        };
        match outcome {
            Ok(mut final_envelope) => {
                self.note_internal(view, None, &scope.epoch).await;
                if let Some(path) = collector.keep() {
                    let data = final_envelope
                        .as_object_mut()
                        .and_then(|env| {
                            env.entry("data".to_owned())
                                .or_insert_with(|| Value::Object(JsonMap::new()))
                                .as_object_mut()
                        });
                    if let Some(data) = data {
                        data.insert(
                            "local_path".to_owned(),
                            Value::String(path.to_string_lossy().into_owned()),
                        );
                    }
                }
                let text = final_envelope.to_string();
                let result = DebugCallResult::Ok {
                    outcome: outcome_from(
                        &text,
                        final_envelope,
                        started.elapsed(),
                        DebugChannelId::Internal,
                    ),
                };
                self.record_call(view, plain, &DebugChannelId::Internal, &result, &scope.epoch);
                result
            }
            Err(error) => {
                collector.abandon().await;
                let result = DebugCallResult::Err { error };
                self.record_call(view, plain, &DebugChannelId::Internal, &result, &scope.epoch);
                result
            }
        }
    }

    /// NapCat 内部的流式调用走适配器 WS：HTTP `/api/Debug/call` 那边中间帧给了空工具包
    /// 全丢了，只有 WS 上才会一帧帧推回来。连独立连接：事件广播照推，这边事件出口直接丢
    async fn napcat_download(
        &self,
        client: &Arc<ncd_backend_napcat::NapCatDebugClient>,
        plain: &DebugCallRequest,
        params: &Value,
        collector: &mut Collector,
        sink: &Arc<dyn DebugStreamSink>,
        scope: &CallScope,
    ) -> Result<Value, DebugError> {
        let adapter = client
            .create_adapter()
            .await
            .map_err(|e| from_napcat(e, scope.timeout).error)?;
        let (events_tx, _ignored) = mpsc::channel(1);
        let ws = connect_ws(&client.ws_url(&adapter), None, events_tx)
            .await
            .map_err(|e| from_client(e, scope.timeout).error)?;
        let mut stream = ws
            .call_stream(&plain.action, params)
            .await
            .map_err(|e| from_client(e, scope.timeout).error)?;
        loop {
            let next = self
                .guarded(scope, stream.next(scope.deadline.saturating_duration_since(Instant::now())))
                .await;
            let frame = match next {
                Ok(Ok(raw)) => raw.value,
                Ok(Err(e)) => return Err(from_client(e, scope.timeout).error),
                Err(error) => return Err(error),
            };
            match collector.handle(&frame, sink).await? {
                Verdict::Continue => {}
                Verdict::Final => return Ok(frame),
            }
        }
    }

    /// SnowLuma 内部的流式调用走 `invoke-stream`：帧按 SSE 推回，不丢帧
    async fn snowluma_download(
        &self,
        view: &DebugBotView,
        client: &Arc<ncd_backend_snowluma::SnowLumaDebugClient>,
        plain: &DebugCallRequest,
        params: &Value,
        collector: &mut Collector,
        sink: &Arc<dyn DebugStreamSink>,
        scope: &CallScope,
    ) -> Result<Value, DebugError> {
        // SSE 事件流回放这个账号的每次 invoke：先登记，回放到达时事件接收器会跳过
        self.note_own_call(&view.bot_id(), &plain.action, params);
        let uin = view.config.bot.qq_id.to_string();
        let resp = client
            .invoke_stream(&uin, &plain.action, params)
            .await
            .map_err(|e| from_snowluma(e, scope.timeout).error)?;
        let mut body = resp.bytes_stream();
        let mut parser = SseParser::new();
        loop {
            let next = match self.guarded(scope, body.next()).await {
                Ok(next) => next,
                Err(error) => return Err(error),
            };
            match next {
                Some(Ok(bytes)) => {
                    let frames = parser.try_push(&bytes).map_err(|e| DebugError::Transport {
                        message: format!("下载流里有一帧过大：{e}"),
                    })?;
                    for frame in frames {
                        let value: Value = serde_json::from_str(&frame.data).map_err(|e| {
                            DebugError::Transport {
                                message: format!("下载流里有一帧不是 JSON：{e}"),
                            }
                        })?;
                        if matches!(collector.handle(&value, sink).await?, Verdict::Final) {
                            return Ok(value);
                        }
                    }
                }
                Some(Err(e)) => {
                    return Err(DebugError::Transport {
                        message: format!("下载流中断：{e}"),
                    });
                }
                None => {
                    // 终帧之后 SSE 自然结束；没看到终帧就结束了，说明上游半途断了
                    return Err(DebugError::Transport {
                        message: "下载流提前结束（没看到完成帧）".to_owned(),
                    });
                }
            }
        }
    }

    // -------------------------------------------------------------------
    // 本机文件与进度
    // -------------------------------------------------------------------

    /// 读本机文件元信息并算整份 SHA-256（上游合并完和它对）。读不了、超上限都在
    /// 这一步说，不传到一半才报
    async fn read_local_file(
        &self,
        local: &DebugLocalFile,
        sink: &Arc<dyn DebugStreamSink>,
        scope: &CallScope,
    ) -> Result<LocalFileMeta, Failure> {
        let meta = tokio::fs::metadata(&local.path)
            .await
            .map_err(failure_invalid)?;
        if !meta.is_file() {
            return Err(Failure::new(invalid("这个路径不是文件")));
        }
        if meta.len() > MAX_LOCAL_FILE_BYTES {
            return Err(Failure::new(invalid(&format!(
                "本机文件超过 {} MiB，调试台不塞这么大的",
                MAX_LOCAL_FILE_BYTES / (1024 * 1024)
            ))));
        }
        let name = file_name_of(&local.path);
        let progress = Progress {
            request_id: &scope.request_id,
            sink,
            scope,
        };
        progress
            .send(DebugStreamStage::Reading, &name, 0, Some(meta.len()), 0, None)
            .map_err(Failure::new)?;
        let mut file = tokio::fs::File::open(&local.path)
            .await
            .map_err(failure_invalid)?;
        let mut hasher = sha2::Sha256::new();
        let mut buf = vec![0_u8; 64 * 1024];
        loop {
            let n = self
                .guarded(scope, file.read(&mut buf))
                .await
                .map_err(Failure::new)?
                .map_err(failure_invalid)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let size = meta.len();
        let total_chunks = u32::try_from(size.div_ceil(CHUNK_BYTES as u64)).unwrap_or(u32::MAX);
        Ok(LocalFileMeta {
            size,
            sha256: hex::encode(hasher.finalize()),
            total_chunks: total_chunks.max(1),
        })
    }
}

fn is_snowluma(view: &DebugBotView) -> bool {
    view.config.bot.backend_type == BackendType::SnowLuma
}

fn started_for_chunks(scope: &CallScope) -> Instant {
    scope.deadline - scope.timeout
}

fn failure_invalid(err: std::io::Error) -> Failure {
    Failure::new(DebugError::Transport {
        message: format!("本机文件不可读：{err}"),
    })
}

/// 前端给的占位标记必须是 前缀 + 路径：两份编码对上才信，手碰巧遇上的不算数
fn check_token(local: &DebugLocalFile) -> Result<(), DebugError> {
    if local.token == format!("{LOCAL_FILE_TOKEN_PREFIX}{}", local.path) {
        Ok(())
    } else {
        Err(invalid(TOKEN_MISMATCH_REASON))
    }
}

fn plain_request(req: &DebugStreamCallRequest, params: Value) -> DebugCallRequest {
    DebugCallRequest {
        request_id: req.request_id.clone(),
        bot_id: req.bot_id.clone(),
        channel: req.channel.clone(),
        action: req.action.clone(),
        params,
        timeout_ms: req.timeout_ms,
        origin: req.origin.clone(),
    }
}

fn remaining_ms(scope: &CallScope) -> u32 {
    let ms = duration_ms(scope.deadline.saturating_duration_since(Instant::now()));
    ms.max(1_000)
}

/// 上传参数的公共部分：协议字段我们算了我们收（chunk_data / chunk_index 由调用点补），
/// 别的（尤其 file_retention）照用户写的来
fn upload_base(base: &Value, filename: &str, meta: &LocalFileMeta, stream_id: &str) -> Value {
    let mut body = match base {
        Value::Object(map) => map.clone(),
        _ => JsonMap::new(),
    };
    for key in [
        "chunk_data",
        "chunk_index",
        "is_complete",
        "reset",
        "verify_only",
    ] {
        body.remove(key);
    }
    body.insert("stream_id".to_owned(), json!(stream_id));
    body.insert("total_chunks".to_owned(), json!(meta.total_chunks));
    body.insert("file_size".to_owned(), json!(meta.size));
    body.insert("expected_sha256".to_owned(), json!(meta.sha256));
    body.entry("filename".to_owned())
        .or_insert_with(|| json!(filename));
    body.entry("file_retention".to_owned())
        .or_insert_with(|| json!(PUSH_RETENTION_MS));
    Value::Object(body)
}

/// 一块块的回包确认：拿到失败回包或确认没说 chunk_received 的都按提前收场处理
fn chunk_ack_ok(raw: &RawCall) -> bool {
    let reply = parse_ob11_reply(&raw.value);
    let ok = reply.retcode == 0 && !reply.status.eq_ignore_ascii_case("failed");
    if !ok {
        return false;
    }
    match reply.data.get("status").and_then(Value::as_str) {
        Some("chunk_received") | None => true,
        _ => false,
    }
}

/// 中间拿到失败回包：整笔结果就是这份回包（和普通调用同款待遇），耗时按从发起到现在
fn frame_outcome(raw: RawCall, started: Instant) -> DebugCallResult {
    let outcome = outcome_from(
        &raw.text,
        raw.value,
        started.elapsed(),
        DebugChannelId::Internal,
    );
    DebugCallResult::Ok { outcome }
}

enum Verdict {
    Continue,
    Final,
}

/// 下载流的收拢状态：file_info 给总量，file_chunk 逐块落盘，终帧收成
struct Collector {
    request_id: String,
    dir: PathBuf,
    fallback_name: String,
    out: Option<(tokio::fs::File, PathBuf)>,
    file_name: Option<String>,
    total_bytes: Option<u64>,
    done_bytes: u64,
    done_chunks: u32,
}

impl Collector {
    async fn new(dir: PathBuf, request_id: String, action: &str) -> Result<Self, DebugError> {
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| DebugError::Transport {
                message: format!("建下载目录失败：{e}"),
            })?;
        Ok(Self {
            request_id,
            dir,
            fallback_name: safe_name(action),
            out: None,
            file_name: None,
            total_bytes: None,
            done_bytes: 0,
            done_chunks: 0,
        })
    }

    /// 处理一帧回答：中间帧记进度（file_chunk 顺带落盘），终帧让调用方收尾
    async fn handle(
        &mut self,
        value: &Value,
        sink: &Arc<dyn DebugStreamSink>,
    ) -> Result<Verdict, DebugError> {
        let reply = parse_ob11_reply(value);
        let failed = reply.retcode != 0 || reply.status.eq_ignore_ascii_case("failed");
        let dtype = reply.data.get("type").and_then(Value::as_str);
        if failed || dtype != Some("stream") {
            return Ok(Verdict::Final);
        }
        self.done_chunks += 1;
        match reply.data.get("data_type").and_then(Value::as_str) {
            Some("file_info") => {
                self.file_name = reply
                    .data
                    .get("file_name")
                    .and_then(Value::as_str)
                    .map(safe_name);
                self.total_bytes = reply.data.get("file_size").and_then(Value::as_u64);
                self.emit(sink)?;
            }
            Some("file_chunk") => {
                let b64 = reply.data.get("data").and_then(Value::as_str).unwrap_or_default();
                let bytes = B64.decode(b64).map_err(|e| DebugError::Transport {
                    message: format!("下载分块不是合法 base64：{e}"),
                })?;
                self.write_chunk(&bytes).await?;
                self.emit(sink)?;
            }
            // test_download_stream 之类的纯帧：进度照常推，不落盘
            _ => {
                self.done_bytes += reply.data.get("size").and_then(Value::as_u64).unwrap_or(0);
                self.emit(sink)?;
            }
        }
        Ok(Verdict::Continue)
    }

    async fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), DebugError> {
        if self.done_bytes + bytes.len() as u64 > MAX_DOWNLOAD_BYTES {
            return Err(DebugError::Transport {
                message: "下载超过 4 GiB，中断".to_owned(),
            });
        }
        if self.out.is_none() {
            let name = self.file_name.clone().unwrap_or_else(|| self.fallback_name.clone());
            let path = self.dir.join(format!("{}__{}", safe_name(&self.request_id), name));
            let file = tokio::fs::File::create(&path)
                .await
                .map_err(|e| DebugError::Transport {
                    message: format!("建下载文件失败：{e}"),
                })?;
            self.out = Some((file, path));
        }
        let (file, _) = self.out.as_mut().expect("前面刚确认过 out 有值");
        use tokio::io::AsyncWriteExt as _;
        file.write_all(bytes)
            .await
            .map_err(|e| DebugError::Transport {
                message: format!("写下载文件失败：{e}"),
            })?;
        self.done_bytes += bytes.len() as u64;
        Ok(())
    }

    fn emit(&self, sink: &Arc<dyn DebugStreamSink>) -> Result<(), DebugError> {
        let ok = sink.send(&DebugStreamProgress {
            v: DEBUG_STREAM_VERSION,
            request_id: self.request_id.clone(),
            stage: DebugStreamStage::Downloading,
            file_name: self.file_name.clone().unwrap_or_default(),
            done_bytes: self.done_bytes,
            total_bytes: self.total_bytes,
            done_chunks: self.done_chunks,
            total_chunks: None,
        });
        if ok {
            Ok(())
        } else {
            Err(DebugError::Cancelled)
        }
    }

    /// 收成的文件留下；返回 None 就是没上过盘（test_download_stream）
    fn keep(mut self) -> Option<PathBuf> {
        self.out.take().map(|(_, path)| path)
    }

    /// 半成品删掉。删不掉不追着报（写失败那份才是要紧的），下次清理可以再扫
    async fn abandon(mut self) {
        if let Some((_, path)) = self.out.take() {
            let _ = tokio::fs::remove_file(path).await;
        }
    }
}

/// 把 params 里所有等于 token 的字符串换成 remote，返回换了几处；0 = 标记压根没填进参数
fn substitute_token(params: &mut Value, token: &str, remote: &str) -> usize {
    let mut hits = 0;
    substitute_in(params, token, remote, &mut hits);
    hits
}

fn substitute_in(value: &mut Value, token: &str, remote: &str, hits: &mut usize) {
    match value {
        Value::String(s) if s == token => {
            *s = remote.to_owned();
            *hits += 1;
        }
        Value::Array(items) => {
            for item in items {
                substitute_in(item, token, remote, hits);
            }
        }
        Value::Object(map) => {
            for v in map.values_mut() {
                substitute_in(v, token, remote, hits);
            }
        }
        _ => {}
    }
}

/// 原始字节流给 `/api/debug/upload`：一块块读，读出来一块报一拍进度
fn file_body(
    file: tokio::fs::File,
    report: impl FnMut(u64) + Send + Sync + 'static,
) -> impl futures_util::Stream<Item = Result<bytes::Bytes, std::io::Error>> + Send + Sync + 'static {
    stream::unfold((file, report), |(mut file, mut report)| async move {
        let mut buf = vec![0_u8; CHUNK_BYTES];
        match file.read(&mut buf).await {
            Ok(0) => None,
            Ok(n) => {
                buf.truncate(n);
                report(n as u64);
                Some((Ok(bytes::Bytes::from(buf)), (file, report)))
            }
            Err(e) => Some((Err(e), (file, report))),
        }
    })
}

/// 给人看的（也是上游要的）文件名：只留最后一段
fn file_name_of(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "upload.bin".to_owned())
}

/// 下载物落到本机盘时用的安全名：basename + 白名单字符，来历不明的字符全变成下划线
fn safe_name(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    let mut out: String = base
        .chars()
        .map(|c| match c {
            '.' | '-' | '_' | '0'..='9' | 'a'..='z' | 'A'..='Z' => c,
            _ => '_',
        })
        .collect();
    out.truncate(120);
    // 开头一段全是被替换出来的下划线（中文名前辍、「.」），不是名字的一部分
    let out = out.trim_start_matches(['.', '_']).to_owned();
    if out.is_empty() {
        DOWNLOAD_FALLBACK_NAME.to_owned()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_file(path: &str) -> DebugLocalFile {
        DebugLocalFile {
            path: path.to_owned(),
            token: format!("{LOCAL_FILE_TOKEN_PREFIX}{path}"),
        }
    }

    #[test]
    fn route_puts_actions_on_the_road_they_belong_to() {
        let req = |action: &str, files: Vec<DebugLocalFile>| DebugStreamCallRequest {
            request_id: "r".into(),
            bot_id: "1".into(),
            channel: DebugChannelId::Internal,
            action: action.to_owned(),
            params: json!({}),
            local_files: files,
            timeout_ms: None,
            origin: ncd_domain::onebot_debug::DebugCallOrigin::Editor,
        };
        assert!(matches!(
            stream_route(&req("upload_file_stream", vec![local_file("C:/a.bin")])).unwrap(),
            Route::StandaloneUpload(_)
        ));
        assert!(matches!(
            stream_route(&req("download_file_stream", vec![])).unwrap(),
            Route::Download
        ));
        assert!(matches!(
            stream_route(&req("test_download_stream", vec![])).unwrap(),
            Route::Download
        ));
        assert!(matches!(
            stream_route(&req("clean_stream_temp_file", vec![])).unwrap(),
            Route::Plain
        ));
        assert!(matches!(
            stream_route(&req("upload_group_file", vec![local_file("C:/a.bin")])).unwrap(),
            Route::PushThenCall(_)
        ));
        // upload_file_stream 手填参数、不带本机文件：普通的单帧调用（手开一块块地试）
        assert!(matches!(
            stream_route(&req("upload_file_stream", vec![])).unwrap(),
            Route::Plain
        ));
        for (action, files) in [
            ("upload_file_stream", vec![local_file("C:/a.bin"), local_file("C:/b.bin")]),
            ("download_file_stream", vec![local_file("C:/a.bin")]),
        ] {
            assert!(matches!(
                stream_route(&req(action, files)),
                Err(DebugError::InvalidParams { .. })
            ));
        }
    }

    #[test]
    fn substitute_replaces_every_level() {
        let mut params = json!({
            "file": "ncd-local-file://C:/a.png",
            "message": [
                {"type": "image", "data": {"file": "ncd-local-file://C:/a.png"}},
                {"type": "text", "data": {"text": "看"}},
            ],
            "untouched": "ncd-local-file://C:/b.png",
        });
        let hits = substitute_token(&mut params, "ncd-local-file://C:/a.png", "/tmp/bot/a.png");
        assert_eq!(hits, 2);
        assert_eq!(params["file"], json!("/tmp/bot/a.png"));
        assert_eq!(params["message"][0]["data"]["file"], json!("/tmp/bot/a.png"));
        assert_eq!(params["message"][1]["data"]["text"], json!("看"));
        assert_eq!(params["untouched"], json!("ncd-local-file://C:/b.png"));
    }

    #[test]
    fn token_must_match_path_byte_for_byte() {
        let good = local_file("C:/tmp/a.png");
        assert!(check_token(&good).is_ok());
        let mut bad = good.clone();
        bad.token = "ncd-local-file://D:/else.png".to_owned();
        assert!(matches!(
            check_token(&bad),
            Err(DebugError::InvalidParams { .. })
        ));
        let mut foreign = good.clone();
        foreign.token = "http://example/x".to_owned();
        assert!(check_token(&foreign).is_err());
    }

    #[test]
    fn safe_name_keeps_basename_and_strips_odd_chars() {
        assert_eq!(safe_name("C:\\tmp\\报告 v2 (final).png"), "v2__final_.png");
        assert_eq!(safe_name("/tmp/upload_1/chunk"), "chunk");
        assert_eq!(safe_name("..\\..\\etc\\passwd"), "passwd");
        assert_eq!(safe_name(""), DOWNLOAD_FALLBACK_NAME);
        assert_eq!(safe_name("..."), DOWNLOAD_FALLBACK_NAME);
        let long = safe_name(&"中".repeat(200));
        assert!(long.chars().count() <= 120);
    }

    #[test]
    fn chunk_ack_distinguishes_ack_from_early_reply() {
        let ok: RawCall = RawCall {
            text: String::new(),
            value: json!({"status": "ok", "retcode": 0, "data": {"type": "stream", "status": "chunk_received"}}),
        };
        assert!(chunk_ack_ok(&ok));
        // 确认里没有 status 字段的宽容收下
        let bare = RawCall {
            text: String::new(),
            value: json!({"status": "ok", "retcode": 0, "data": {"type": "stream"}}),
        };
        assert!(chunk_ack_ok(&bare));
        for bad in [
            json!({"status": "failed", "retcode": 1404, "data": null}),
            json!({"status": "ok", "retcode": 0, "data": {"type": "stream", "status": "file_created"}}),
        ] {
            let raw = RawCall {
                text: String::new(),
                value: bad,
            };
            assert!(!chunk_ack_ok(&raw));
        }
    }

    #[test]
    fn upload_base_overrides_protocol_fields_but_keeps_user_extras() {
        let base = json!({
            "filename": "我的.zip",
            "file_retention": 60_000,
            "reset": true,
            "stream_id": "旧的",
            "ext_flag": 1,
        });
        let meta = LocalFileMeta {
            size: 1500,
            sha256: "abc".into(),
            total_chunks: 1,
        };
        let body = upload_base(&base, "auto.bin", &meta, "sid");
        assert_eq!(body["stream_id"], json!("sid"));
        assert_eq!(body["filename"], json!("我的.zip"), "用户写的文件名优先");
        assert_eq!(body["file_retention"], json!(60_000));
        assert_eq!(body["ext_flag"], json!(1));
        assert_eq!(body["file_size"], json!(1500));
        assert!(body.get("reset").is_none());
        assert!(body.get("chunk_data").is_none());
        assert!(body.get("is_complete").is_none());
    }
}
