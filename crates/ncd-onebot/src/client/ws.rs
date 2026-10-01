//! OneBot WebSocket 客户端：一条连接上既发动作（按 `echo` 配对回包），又收事件（转发给调用方）。
//!
//! 连接由两个后台任务撑着：读任务分拣每一帧，写任务串行发出请求。任何一头出错或对端关闭，
//! 整条连接就标记为已关闭，所有在等回包的调用立刻以 [`ClientError::Closed`] 失败，
//! 而不是各自干等到超时。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::error::UrlError;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};
use uuid::Uuid;

use super::{ClientError, RawCall};

/// 建立连接（含 WebSocket 升级）的时限
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// 关闭时给对端发 Close 帧的时限；对端已经不在了就别拖着
const CLOSE_FRAME_TIMEOUT: Duration = Duration::from_millis(500);

const WS_SCHEME_HINT: &str = "地址必须以 ws:// 或 wss:// 开头";

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// 等回包的两种形态：普通调用拿第一帧就走，流式调用把同一 echo 的每一帧都收下
enum Pending {
    Once(oneshot::Sender<RawCall>),
    Stream(mpsc::Sender<RawCall>),
}

type PendingMap = HashMap<String, Pending>;

/// 流式调用的帧缓冲：消费方（往盘上写分块）最慢也慢不过几十帧，再大就是真卡住了，
/// 这时背压直接顶回读任务 —— 丢一帧就是坏文件，宁堵不丢
const STREAM_QUEUE_CAP: usize = 64;

/// 读任务、写任务与句柄共享的状态
struct Shared {
    /// 等回包的调用，键是 echo。只在很短的临界区里持有，从不跨 await
    pending: Mutex<PendingMap>,
    /// 置 true 表示连接结束（对端关了、出错了或本端要关）。两个任务都盯着它
    closed_tx: watch::Sender<bool>,
    /// 事件通道满了而丢掉的条数
    dropped: AtomicU64,
}

impl Shared {
    fn pending(&self) -> MutexGuard<'_, PendingMap> {
        // 临界区里只做 HashMap 增删，不会 panic；就算真被毒化，数据也仍然可用
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 结束连接：先置关闭标记，再清空等待表。
    /// 顺序有讲究：`call` 是「先登记、再检查关闭标记」，这里反过来「先标记、再清表」，
    /// 于是登记发生在清表之前的会被清掉（等待方收到 Closed），发生在之后的能看到标记
    fn shut(&self) {
        self.closed_tx.send_replace(true);
        self.pending().clear();
    }
}

/// 排队等写任务发出的一次调用
struct Outgoing {
    /// 用来在发出前确认调用方还在等
    echo: String,
    frame: String,
    /// 写任务开始把它交给套接字时置位。连接结束时没置位的，调用方据此知道上游没收到
    written: Arc<AtomicBool>,
}

struct Inner {
    shared: Arc<Shared>,
    /// 交给写任务发出的调用
    cmd_tx: mpsc::UnboundedSender<Outgoing>,
    closed_rx: watch::Receiver<bool>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        // 最后一个句柄没了：通知两个任务收工，Close 帧由写任务尽力发出
        self.shared.closed_tx.send_replace(true);
    }
}

/// 一条已连上的 WebSocket。可以 `Clone` 到多处并发调用；所有克隆共用同一条连接，
/// 最后一个句柄被丢弃时连接关闭
#[derive(Clone)]
pub struct WsClient {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for WsClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WsClient")
            .field("closed", &self.is_closed())
            .field("dropped", &self.dropped())
            .finish_non_exhaustive()
    }
}

/// 连上 `url`（`ws://` 或 `wss://` 以外的会报错）。有令牌时带 `Authorization: Bearer`。
///
/// 收到的帧按下面的规则分拣：
/// - 带字符串 `echo` 且对得上等待中的调用 → 交给那次调用；
/// - 否则带 `post_type` 的当事件，`try_send` 进 `events`，通道满了就丢并计数（[`WsClient::dropped`]），
///   绝不阻塞读任务 —— 否则一个不读事件的消费者会把回包也一起堵死；
/// - 其余（晚到的回包、无关帧）忽略。
///
/// 升级时被 401 / 403 拒绝是 [`ClientError::Unauthorized`]，其它失败是 [`ClientError::Connect`]。
pub async fn connect_ws(
    url: &str,
    token: Option<&str>,
    events: mpsc::Sender<Value>,
) -> Result<WsClient, ClientError> {
    let mut request = url
        .into_client_request()
        .map_err(|e| ClientError::Connect(format!("地址无效：{}", describe_ws_error(&e))))?;
    // tungstenite 要先把 TCP 连上才检查协议头，写成 http:// 会白等一次连接；这里先拦下
    if !matches!(request.uri().scheme_str(), Some("ws" | "wss")) {
        return Err(ClientError::Connect(WS_SCHEME_HINT.to_owned()));
    }
    if let Some(token) = token.filter(|t| !t.is_empty()) {
        let mut value = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| ClientError::Connect("令牌含有不能放进请求头的字符".to_owned()))?;
        // 标记敏感：万一哪里把请求头打进日志，也不会露出令牌
        value.set_sensitive(true);
        request.headers_mut().insert(AUTHORIZATION, value);
    }

    let (stream, _response) = tokio::time::timeout(CONNECT_TIMEOUT, connect_async(request))
        .await
        .map_err(|_| ClientError::Connect("连接超时".to_owned()))?
        .map_err(|e| match &e {
            WsError::Http(response) => {
                let status = response.status().as_u16();
                if matches!(status, 401 | 403) {
                    ClientError::Unauthorized(status)
                } else {
                    ClientError::Connect(format!("WebSocket 升级被拒绝（HTTP {status}）"))
                }
            }
            other => ClientError::Connect(describe_ws_error(other)),
        })?;

    let (closed_tx, closed_rx) = watch::channel(false);
    let shared = Arc::new(Shared {
        pending: Mutex::new(HashMap::new()),
        closed_tx,
        dropped: AtomicU64::new(0),
    });
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (sink, source) = stream.split();

    tokio::spawn(read_loop(source, events, Arc::clone(&shared)));
    tokio::spawn(write_loop(sink, cmd_rx, Arc::clone(&shared)));

    Ok(WsClient {
        inner: Arc::new(Inner {
            shared,
            cmd_tx,
            closed_rx,
        }),
    })
}

/// 错误文案。不直接用 tungstenite 的 Display 是因为个别变体是英文且对用户没意义
fn describe_ws_error(err: &WsError) -> String {
    match err {
        WsError::Url(UrlError::TlsFeatureNotEnabled) => "当前构建不支持 wss:// 连接".to_owned(),
        WsError::Url(UrlError::UnsupportedUrlScheme) => WS_SCHEME_HINT.to_owned(),
        other => other.to_string(),
    }
}

/// 等到关闭标记变成 true。只有发送端没了才会 Err，那同样意味着连接已经不在了。
/// 单独成函数是为了不把 `watch::Ref`（持着读锁、不是 `Send`）带进 `select!` 的输出里
async fn wait_closed(rx: &mut watch::Receiver<bool>) {
    let _ = rx.wait_for(|closed| *closed).await;
}

async fn read_loop(
    mut source: SplitStream<WsStream>,
    events: mpsc::Sender<Value>,
    shared: Arc<Shared>,
) {
    let mut closed_rx = shared.closed_tx.subscribe();
    loop {
        let message = tokio::select! {
            biased;
            // 本端要关：不必等对端回 Close 帧
            _ = wait_closed(&mut closed_rx) => break,
            next = source.next() => next,
        };
        match message {
            Some(Ok(Message::Text(text))) => dispatch(&text, &events, &shared),
            Some(Ok(Message::Binary(bytes))) => match std::str::from_utf8(&bytes) {
                Ok(text) => dispatch(text, &events, &shared),
                Err(_) => tracing::debug!("忽略无法解码的二进制帧（{} 字节）", bytes.len()),
            },
            // Ping 的 Pong 由 tungstenite 在下一次读写时自动带出，这里不用管
            Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => {}
            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
        }
    }
    shared.shut();
}

/// 分拣一帧文本：回包交给等待者（普通调用第一帧就完，流式调用一帧一帧接着给），
/// 事件转发，其余忽略
async fn dispatch(text: &str, events: &mpsc::Sender<Value>, shared: &Shared) {
    // 只为调用建的连接（事件接收端早已丢掉）在没有等回包的调用时，这一帧不管是什么都用不上：
    // 连解析都省了。上游照推所有事件，一个带大块 `raw` 的消息也要解析一遍太浪费
    if events.is_closed() && shared.pending().is_empty() {
        return;
    }
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        tracing::debug!("忽略非 JSON 的文本帧");
        return;
    };

    if let Some(echo) = value.get("echo").and_then(Value::as_str) {
        // 先取出投递方式再发送：不在持锁期间做别的事
        let stream_tx = {
            let mut pending = shared.pending();
            match pending.get_mut(echo) {
                Some(Pending::Once(_)) => {
                    if let Some(Pending::Once(waiter)) = pending.remove(echo) {
                        // 等待方已经超时走人时 send 会失败，属正常，丢掉即可
                        let _ = waiter.send(RawCall {
                            text: text.to_owned(),
                            value,
                        });
                    }
                    return;
                }
                Some(Pending::Stream(tx)) => Some(tx.clone()),
                None => None,
            }
        };
        if let Some(tx) = stream_tx {
            // 消费方慢了就在这里等它跟上；连接关闭时沟道断开，send 失败即收手。
            // 流式帧一旦晚到、登记被摘，和普通回包一样掉到下面的忽略分支
            let _ = tx
                .send(RawCall {
                    text: text.to_owned(),
                    value,
                })
                .await;
            return;
        }
    }

    if value.get("post_type").is_some() && events.try_send(value).is_err() {
        shared.dropped.fetch_add(1, Ordering::Relaxed);
    }
}

async fn write_loop(
    mut sink: SplitSink<WsStream, Message>,
    mut commands: mpsc::UnboundedReceiver<Outgoing>,
    shared: Arc<Shared>,
) {
    let mut closed_rx = shared.closed_tx.subscribe();
    // 关闭信号把一次写到一半的 send 打断了：连接上留着半个帧，之后再发 Close 帧只会是乱码，
    // 直接丢连接即可
    let mut interrupted = false;
    loop {
        let outgoing = tokio::select! {
            biased;
            _ = wait_closed(&mut closed_rx) => break,
            command = commands.recv() => match command {
                Some(outgoing) => outgoing,
                None => break,
            },
        };
        // 排队期间调用方已经超时或取消（登记被摘掉了）：没人等这个回包，就别发了。
        // 否则上游会白白执行一个用户以为已经放弃的动作，`send_msg` 之类还有副作用
        if !shared.pending().contains_key(&outgoing.echo) {
            continue;
        }
        // 从这一刻起就算「可能发出去了」：写到一半被打断的，上游也可能收到了半个帧以外的东西
        outgoing.written.store(true, Ordering::SeqCst);
        // 写也要能被关闭信号打断：对端不读、内核缓冲写满时 `send` 会一直挂起，
        // 不这样的话 `close()` / 丢弃句柄之后，写任务和它握着的套接字会一直留着
        let sent = tokio::select! {
            biased;
            _ = wait_closed(&mut closed_rx) => {
                interrupted = true;
                break;
            }
            result = sink.send(Message::Text(outgoing.frame)) => result,
        };
        if sent.is_err() {
            shared.shut();
            break;
        }
    }
    if !interrupted {
        // 尽力告诉对端我们走了；对端已断时这一步失败也无所谓
        let _ = tokio::time::timeout(CLOSE_FRAME_TIMEOUT, sink.close()).await;
    }
}

/// 从 `call` 里离开（正常返回、超时、被取消）时把自己的登记摘掉，免得等待表里留下死项
struct PendingGuard<'a> {
    shared: &'a Shared,
    echo: &'a str,
}

impl Drop for PendingGuard<'_> {
    fn drop(&mut self) {
        self.shared.pending().remove(self.echo);
    }
}

impl WsClient {
    /// 发一个动作并等它的回包（用新生成的 uuid 当 echo）。
    ///
    /// 超时是 [`ClientError::Timeout`]；请求写出去之后连接断了是 [`ClientError::Closed`]；
    /// 还没写出去连接就关了（包括调用时已经关了）是 [`ClientError::NotSent`]，这时上游
    /// 没收到请求，调用方可以放心换一条连接重发。回包里的 retcode 非 0 不算错误，原样返回。
    pub async fn call(
        &self,
        action: &str,
        params: &Value,
        timeout: Duration,
    ) -> Result<RawCall, ClientError> {
        if self.is_closed() {
            return Err(ClientError::NotSent);
        }
        let echo = Uuid::new_v4().to_string();
        let frame = json!({"action": action, "params": params, "echo": echo}).to_string();

        let (reply_tx, reply_rx) = oneshot::channel();
        self.inner
            .shared
            .pending()
            .insert(echo.clone(), Pending::Once(reply_tx));
        let _guard = PendingGuard {
            shared: &self.inner.shared,
            echo: &echo,
        };
        // 登记之后再查一次：连接恰好在两步之间结束时，`shut` 可能已经清过表了，
        // 这条新登记没人会去唤醒，只能自己发现
        if self.is_closed() {
            return Err(ClientError::NotSent);
        }
        let written = Arc::new(AtomicBool::new(false));
        self.inner
            .cmd_tx
            .send(Outgoing {
                echo: echo.clone(),
                frame,
                written: Arc::clone(&written),
            })
            .map_err(|_| ClientError::NotSent)?;

        match tokio::time::timeout(timeout, reply_rx).await {
            Ok(Ok(reply)) => Ok(reply),
            // 发送端被丢弃 = 连接结束时等待表被清空。还排在写队列里没轮到的，上游没收到
            Ok(Err(_)) if !written.load(Ordering::SeqCst) => Err(ClientError::NotSent),
            Ok(Err(_)) => Err(ClientError::Closed),
            Err(_) => Err(ClientError::Timeout),
        }
    }

    /// 连接是否已经结束
    pub fn is_closed(&self) -> bool {
        *self.inner.closed_rx.borrow()
    }

    /// 发一个流式动作（分块下载这种「一次请求、多帧回答」）：同一 echo 的每一帧依次由
    /// 返回的 [`WsStreamCall`] 取；哪一帧是终点由动作的协议决定，这里不判断。
    ///
    /// 消费方不及时取帧时读任务会被背压顶住（一帧都不能丢），所以别在收事件的共用
    /// 连接上用它 —— 单独连一条
    pub async fn call_stream(&self, action: &str, params: &Value) -> Result<WsStreamCall, ClientError> {
        if self.is_closed() {
            return Err(ClientError::NotSent);
        }
        let echo = Uuid::new_v4().to_string();
        let frame = json!({"action": action, "params": params, "echo": echo}).to_string();

        let (tx, rx) = mpsc::channel(STREAM_QUEUE_CAP);
        self.inner
            .shared
            .pending()
            .insert(echo.clone(), Pending::Stream(tx));
        // 登记之后再查一次：连接恰好在两步之间结束时，`shut` 可能已经清过表了
        if self.is_closed() {
            self.inner.shared.pending().remove(&echo);
            return Err(ClientError::NotSent);
        }
        let written = Arc::new(AtomicBool::new(false));
        if self
            .inner
            .cmd_tx
            .send(Outgoing {
                echo: echo.clone(),
                frame,
                written: Arc::clone(&written),
            })
            .is_err()
        {
            self.inner.shared.pending().remove(&echo);
            return Err(ClientError::NotSent);
        }
        Ok(WsStreamCall {
            shared: Arc::clone(&self.inner.shared),
            echo,
            rx,
            written,
        })
    }

    /// 等到连接结束才返回；已经结束则立即返回
    pub async fn closed(&self) {
        wait_closed(&mut self.inner.closed_rx.clone()).await;
    }

    /// 主动关闭。等回包的调用会收到 [`ClientError::Closed`]
    pub fn close(&self) {
        self.inner.shared.shut();
    }

    /// 正在等回包的调用数。想关掉一条共用连接的一方据此判断会不会打断别人的调用
    pub fn in_flight(&self) -> usize {
        self.inner.shared.pending().len()
    }

    /// 因为事件通道满了而丢掉的事件累计条数
    pub fn dropped(&self) -> u64 {
        self.inner.shared.dropped.load(Ordering::Relaxed)
    }
}

/// [`WsClient::call_stream`] 拿到的流式帧序列。丢掉时摘掉等待登记，晚到的同 echo 帧照常忽略
pub struct WsStreamCall {
    shared: Arc<Shared>,
    echo: String,
    rx: mpsc::Receiver<RawCall>,
    written: Arc<AtomicBool>,
}

impl WsStreamCall {
    /// 取下一帧。`timeout` 是等这一帧的上限：上游先把文件搬到本地再开始吐帧，
    /// 中途可能一阵没帧，别拿整体时限去卡大文件
    pub async fn next(&mut self, timeout: Duration) -> Result<RawCall, ClientError> {
        match tokio::time::timeout(timeout, self.rx.recv()).await {
            Ok(Some(frame)) => Ok(frame),
            // 发送端被丢弃 = 连接结束时等待表被清空。还没写出去的，上游根本没收到
            Ok(None) if !self.written.load(Ordering::SeqCst) => Err(ClientError::NotSent),
            Ok(None) => Err(ClientError::Closed),
            Err(_) => Err(ClientError::Timeout),
        }
    }
}

impl Drop for WsStreamCall {
    fn drop(&mut self) {
        self.shared.pending().remove(&self.echo);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    /// 服务端握手完成后一个字节都不读，模拟对端卡死、内核缓冲写满：
    /// 此时写任务卡在 `send` 上，`close()` 必须能把它放走
    #[tokio::test]
    async fn close_unblocks_a_writer_stuck_on_a_full_socket() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (release_tx, release_rx) = oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let _ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            // 握着连接但不读，直到测试结束
            let _ = release_rx.await;
        });

        let (events_tx, _events_rx) = mpsc::channel(8);
        let client = connect_ws(&format!("ws://{addr}/"), None, events_tx)
            .await
            .unwrap();
        // 读任务 + 写任务 + 句柄自己，各拿一份共享状态
        assert_eq!(Arc::strong_count(&client.inner.shared), 3);

        // 一共 48 MiB，远超回环连接两端的内核缓冲；每个调用都排着队等回包，
        // 所以不会被「调用方已放弃」的逻辑跳过
        let params = json!({"pad": "x".repeat(8 * 1024 * 1024)});
        let callers: Vec<_> = (0..6)
            .map(|_| {
                let caller = client.clone();
                let params = params.clone();
                tokio::spawn(
                    async move { caller.call("big", &params, Duration::from_secs(30)).await },
                )
            })
            .collect();
        // 让写任务把缓冲写满、卡在 send 上
        tokio::time::sleep(Duration::from_millis(200)).await;

        client.close();
        let deadline = Instant::now() + Duration::from_secs(3);
        while Arc::strong_count(&client.inner.shared) > 1 {
            assert!(
                Instant::now() < deadline,
                "close() 之后读写任务仍未退出（strong_count = {}）",
                Arc::strong_count(&client.inner.shared)
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // 正在写的那一个是 Closed（可能已经发出去一部分），还排在队里的是 NotSent
        let mut not_sent = 0;
        for caller in callers {
            match caller.await.unwrap() {
                Err(ClientError::Closed) => {}
                Err(ClientError::NotSent) => not_sent += 1,
                other => panic!("应是 Closed 或 NotSent：{other:?}"),
            }
        }
        assert!(not_sent >= 1, "排队没写出去的调用应报 NotSent");
        let _ = release_tx.send(());
        server.await.unwrap();
    }

    /// 只为调用建的连接：没人等回包时推来的事件不解析也不计数；有调用在等时才解析，
    /// 这时的事件照旧计为丢弃。在途调用数随调用进出变化
    #[tokio::test]
    async fn a_call_only_connection_skips_events_and_counts_calls_in_flight() {
        use futures_util::{SinkExt, StreamExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (go_tx, go_rx) = oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            let event = json!({"post_type": "message", "raw": "x".repeat(1024)}).to_string();
            ws.send(Message::Text(event.clone())).await.unwrap();
            let Some(Ok(Message::Text(request))) = ws.next().await else {
                panic!("应收到一次调用");
            };
            // 调用在等回包时再推一条事件
            ws.send(Message::Text(event)).await.unwrap();
            let _ = go_rx.await;
            let request: Value = serde_json::from_str(&request).unwrap();
            let reply = json!({"status": "ok", "retcode": 0, "echo": request["echo"]});
            ws.send(Message::Text(reply.to_string())).await.unwrap();
            // 等客户端先走
            while ws.next().await.is_some() {}
        });

        let (events_tx, events_rx) = mpsc::channel(1);
        drop(events_rx);
        let client = connect_ws(&format!("ws://{addr}/"), None, events_tx)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(client.dropped(), 0, "没人等回包时事件直接跳过");
        assert_eq!(client.in_flight(), 0);

        let caller = client.clone();
        let call = tokio::spawn(async move {
            caller
                .call("get_status", &json!({}), Duration::from_secs(5))
                .await
        });
        let deadline = Instant::now() + Duration::from_secs(3);
        while client.in_flight() != 1 || client.dropped() != 1 {
            assert!(
                Instant::now() < deadline,
                "调用应在途、第二条事件应被计为丢弃"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let _ = go_tx.send(());
        assert!(call.await.unwrap().is_ok());
        assert_eq!(client.in_flight(), 0);
        client.close();
        server.await.unwrap();
    }

    /// 流式调用把同一 echo 的每一帧都按序收到（中间的帧不会提前结束等待）；
    /// 同一条连接上并发的普通调用只拿自己那帧
    #[tokio::test]
    async fn stream_call_collects_every_frame_with_its_echo() {
        use futures_util::{SinkExt, StreamExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            let mut stream_echo = None;
            let mut once_echo = None;
            while stream_echo.is_none() || once_echo.is_none() {
                let Some(Ok(Message::Text(req))) = ws.next().await else {
                    panic!("应收到两次调用");
                };
                let request: Value = serde_json::from_str(&req).unwrap();
                if request["action"] == "download_file_stream" {
                    stream_echo = Some(request["echo"].clone());
                } else {
                    once_echo = Some(request["echo"].clone());
                }
            }
            let frame = |echo: &Value, data: Value| {
                Message::Text(
                    json!({"status": "ok", "retcode": 0, "data": data, "stream": "stream-action", "echo": echo})
                        .to_string(),
                )
            };
            let stream_echo = stream_echo.unwrap();
            ws.send(frame(&stream_echo, json!({"type": "stream", "index": 0}))).await.unwrap();
            // 普通调用的回包夹在流式帧中间
            let once_echo = once_echo.unwrap();
            ws.send(Message::Text(
                json!({"status": "ok", "retcode": 0, "data": {"online": true}, "echo": once_echo}).to_string(),
            ))
            .await
            .unwrap();
            ws.send(frame(&stream_echo, json!({"type": "stream", "index": 1}))).await.unwrap();
            ws.send(frame(&stream_echo, json!({"type": "response", "total_chunks": 2}))).await.unwrap();
            while ws.next().await.is_some() {}
        });

        let (events_tx, _events_rx) = mpsc::channel(8);
        let client = connect_ws(&format!("ws://{addr}/"), None, events_tx)
            .await
            .unwrap();
        let mut stream = client
            .call_stream("download_file_stream", &serde_json::json!({}))
            .await
            .unwrap();
        let once = client
            .call("get_status", &serde_json::json!({}), Duration::from_secs(5))
            .await
            .expect("普通调用应拿到自己的回包");
        assert_eq!(once.value["data"]["online"], true);

        let mut kinds = Vec::new();
        for _ in 0..3 {
            let frame = stream.next(Duration::from_secs(5)).await.unwrap();
            kinds.push(frame.value["data"]["type"].as_str().unwrap().to_owned());
            assert_eq!(frame.value["stream"], "stream-action");
        }
        assert_eq!(kinds, ["stream", "stream", "response"]);
        client.close();
        server.await.unwrap();
    }

    /// 连接已经结束时发起流式调用：请求不可能发出，报 NotSent 而不是干等
    #[tokio::test]
    async fn stream_call_on_a_closed_connection_is_not_sent() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            drop(ws);
        });
        let (events_tx, _events_rx) = mpsc::channel(1);
        let client = connect_ws(&format!("ws://{addr}/"), None, events_tx)
            .await
            .unwrap();
        server.await.unwrap();
        client.closed().await;
        let err = client
            .call_stream("download_file_stream", &serde_json::json!({}))
            .await
            .expect_err("已关闭的连接上发起应失败");
        assert!(matches!(err, ClientError::NotSent), "got {err:?}");
    }
}
