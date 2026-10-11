use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::super::offline_notifier::NoopOfflineNotifier;
use super::*;
use ncd_domain::napcat_events::NapCatWebuiAuthIssue;
use ncd_traits::events::{EventFilter, EventSubscription};

// ── 纯函数 ──

#[test]
fn retry_delay_transient_doubles_to_60s_cap() {
    let secs: Vec<u64> = (1..=7)
        .map(|n| retry_delay(AuthFailureKind::Transient, n).as_secs())
        .collect();
    assert_eq!(secs, vec![5, 10, 20, 40, 60, 60, 60]);
}

#[test]
fn retry_delay_rate_limited_is_fixed_65s() {
    for n in [1, 2, 5, 50] {
        assert_eq!(
            retry_delay(AuthFailureKind::RateLimited, n),
            Duration::from_secs(65)
        );
    }
}

#[test]
fn retry_delay_rejected_doubles_to_300s_cap() {
    let secs: Vec<u64> = (1..=6)
        .map(|n| retry_delay(AuthFailureKind::Rejected, n).as_secs())
        .collect();
    assert_eq!(secs, vec![65, 130, 260, 300, 300, 300]);
    assert_eq!(
        retry_delay(AuthFailureKind::TwoFactor, 1),
        Duration::from_secs(65)
    );
    // 大 streak 不能溢出
    assert_eq!(
        retry_delay(AuthFailureKind::Rejected, u32::MAX),
        Duration::from_secs(300)
    );
}

#[test]
fn from_error_maps_every_variant() {
    use AuthFailureKind::*;
    let cases: Vec<(NapCatWebUiError, AuthFailureKind)> = vec![
        (NapCatWebUiError::Unauthorized(401), Rejected),
        (
            NapCatWebUiError::Rejected("token is invalid".into()),
            Rejected,
        ),
        (
            NapCatWebUiError::BusinessCode {
                code: -1,
                message: "x".into(),
            },
            Rejected,
        ),
        (NapCatWebUiError::RateLimited, RateLimited),
        (NapCatWebUiError::TwoFactorRequired, TwoFactor),
        (NapCatWebUiError::Status(500), Transient),
        (NapCatWebUiError::Throttled, Transient),
        (NapCatWebUiError::Timeout, Transient),
        (NapCatWebUiError::Http("refused".into()), Transient),
        (NapCatWebUiError::Decode("x".into()), Transient),
        (NapCatWebUiError::NotLogin, Transient),
    ];
    for (err, expected) in cases {
        assert_eq!(AuthFailureKind::from_error(&err), expected, "{err:?}");
    }
}

#[test]
fn issue_for_transient_waits_four_failures() {
    for n in 1..=3 {
        assert_eq!(issue_for(AuthFailureKind::Transient, n), None);
    }
    assert_eq!(
        issue_for(AuthFailureKind::Transient, 4),
        Some(NapCatWebuiAuthIssue::Unreachable)
    );
    assert_eq!(
        issue_for(AuthFailureKind::Rejected, 1),
        Some(NapCatWebuiAuthIssue::TokenRejected)
    );
    assert_eq!(
        issue_for(AuthFailureKind::RateLimited, 1),
        Some(NapCatWebuiAuthIssue::RateLimited)
    );
    assert_eq!(
        issue_for(AuthFailureKind::TwoFactor, 1),
        Some(NapCatWebuiAuthIssue::TwoFactorRequired)
    );
}

#[test]
fn refresh_allowed_uses_max_of_throttle_and_backoff() {
    let throttle = Duration::from_secs(5);
    assert!(refresh_allowed(None, throttle, Duration::from_secs(300)));
    // 没有退避时退回固定节流
    assert!(!refresh_allowed(
        Some(Duration::from_secs(4)),
        throttle,
        Duration::ZERO
    ));
    assert!(refresh_allowed(
        Some(Duration::from_secs(5)),
        throttle,
        Duration::ZERO
    ));
    // 退避更长时以退避为准
    assert!(!refresh_allowed(
        Some(Duration::from_secs(64)),
        throttle,
        Duration::from_secs(65)
    ));
    assert!(refresh_allowed(
        Some(Duration::from_secs(65)),
        throttle,
        Duration::from_secs(65)
    ));
}

// ── 带 mock HTTP 的 do_auth_refresh / run_poller ──

type FetchResult = Result<String, NapCatWebUiError>;

/// fetch_credential 先按脚本顺序返回，脚本用完后每次调用 fallback
struct ScriptedClient {
    script: Mutex<VecDeque<FetchResult>>,
    fallback: fn() -> FetchResult,
    calls: AtomicUsize,
}

impl ScriptedClient {
    fn new(script: Vec<FetchResult>, fallback: fn() -> FetchResult) -> Self {
        Self {
            script: Mutex::new(script.into()),
            fallback,
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

fn always_rejected() -> FetchResult {
    Err(NapCatWebUiError::Rejected("token is invalid".into()))
}

#[async_trait]
impl NapCatWebUiClient for ScriptedClient {
    async fn fetch_credential(&self, _port: u16, _token: &str) -> FetchResult {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let next = self.script.lock().expect("mock mutex").pop_front();
        next.unwrap_or_else(self.fallback)
    }

    async fn check_login_status(
        &self,
        _port: u16,
        _auth: &str,
    ) -> Result<CheckLoginStatusData, NapCatWebUiError> {
        Ok(CheckLoginStatusData::default())
    }

    async fn check_online_status(
        &self,
        _port: u16,
        _auth: &str,
    ) -> Result<GetQQLoginInfoData, NapCatWebUiError> {
        Ok(GetQQLoginInfoData::default())
    }

    async fn set_ob11_config(
        &self,
        _port: u16,
        _auth: &str,
        _config_json: &str,
    ) -> Result<(), NapCatWebUiError> {
        Ok(())
    }
}

struct NoRestart;
#[async_trait]
impl RestartHandle for NoRestart {
    async fn restart_bot(&self, _bot_id: &BotId) {}
}

fn deps_with(client: Arc<ScriptedClient>) -> (PollerDeps, Arc<BroadcastEventBus>) {
    let bus = Arc::new(BroadcastEventBus::default());
    let deps = PollerDeps {
        event_bus: bus.clone(),
        http: client,
        notifier: Arc::new(NoopOfflineNotifier),
        restart_handle: Arc::new(NoRestart),
    };
    (deps, bus)
}

async fn drain(sub: &mut EventSubscription) -> Vec<DomainEvent> {
    let mut out = Vec::new();
    while let Ok(Some(ev)) = tokio::time::timeout(Duration::from_millis(5), sub.next()).await {
        out.push(ev);
    }
    out
}

fn auth_issues(events: &[DomainEvent]) -> Vec<Option<NapCatWebuiAuthIssue>> {
    events
        .iter()
        .filter_map(|ev| match ev {
            DomainEvent::NapCatWebuiAuthChanged { issue, .. } => Some(*issue),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn do_auth_refresh_rejected_publishes_issue_once() {
    let bot_id = BotId::new("auth-rejected");
    let cfg = PollerConfig::default();
    let client = Arc::new(ScriptedClient::new(vec![], always_rejected));
    let (deps, bus) = deps_with(client.clone());
    let mut sub = bus.subscribe(EventFilter::bot("auth-rejected"));
    let mut state = LoginState::new();

    do_auth_refresh(&bot_id, 6099, "tok", &cfg, &deps, &mut state).await;
    do_auth_refresh(&bot_id, 6099, "tok", &cfg, &deps, &mut state).await;

    assert_eq!(client.calls(), 2);
    assert_eq!(state.auth_failure_kind, Some(AuthFailureKind::Rejected));
    assert_eq!(state.auth_failure_streak, 2);
    assert_eq!(state.auth_retry_delay, Duration::from_secs(130));
    assert!(state.auth.is_none());
    assert_eq!(
        auth_issues(&drain(&mut sub).await),
        vec![Some(NapCatWebuiAuthIssue::TokenRejected)]
    );
}

#[tokio::test]
async fn do_auth_refresh_kind_change_republishes() {
    let bot_id = BotId::new("auth-kind");
    let cfg = PollerConfig::default();
    let client = Arc::new(ScriptedClient::new(
        vec![
            Err(NapCatWebUiError::Timeout),
            Err(NapCatWebUiError::Decode("html".into())),
            Err(NapCatWebUiError::Timeout),
            Err(NapCatWebUiError::Http("reset".into())),
        ],
        always_rejected,
    ));
    let (deps, bus) = deps_with(client.clone());
    let mut sub = bus.subscribe(EventFilter::bot("auth-kind"));
    let mut state = LoginState::new();

    for _ in 0..3 {
        do_auth_refresh(&bot_id, 6099, "tok", &cfg, &deps, &mut state).await;
    }
    assert!(
        auth_issues(&drain(&mut sub).await).is_empty(),
        "暂时性失败前三次不报"
    );

    do_auth_refresh(&bot_id, 6099, "tok", &cfg, &deps, &mut state).await;
    assert_eq!(state.auth_failure_streak, 4);
    assert_eq!(
        auth_issues(&drain(&mut sub).await),
        vec![Some(NapCatWebuiAuthIssue::Unreachable)]
    );

    // 换成明确拒绝:类别变了,streak 从 1 计,重新广播
    do_auth_refresh(&bot_id, 6099, "tok", &cfg, &deps, &mut state).await;
    assert_eq!(state.auth_failure_kind, Some(AuthFailureKind::Rejected));
    assert_eq!(state.auth_failure_streak, 1);
    assert_eq!(state.auth_retry_delay, Duration::from_secs(65));
    assert_eq!(
        auth_issues(&drain(&mut sub).await),
        vec![Some(NapCatWebuiAuthIssue::TokenRejected)]
    );
}

#[tokio::test]
async fn do_auth_refresh_success_after_failure_clears_issue() {
    let bot_id = BotId::new("auth-recover");
    let cfg = PollerConfig::default();
    let client = Arc::new(ScriptedClient::new(
        vec![Err(NapCatWebUiError::RateLimited), Ok("cred".into())],
        always_rejected,
    ));
    let (deps, bus) = deps_with(client.clone());
    let mut sub = bus.subscribe(EventFilter::bot("auth-recover"));
    let mut state = LoginState::new();

    do_auth_refresh(&bot_id, 6099, "tok", &cfg, &deps, &mut state).await;
    assert_eq!(state.auth_retry_delay, Duration::from_secs(65));
    do_auth_refresh(&bot_id, 6099, "tok", &cfg, &deps, &mut state).await;

    assert_eq!(state.auth.as_deref(), Some("cred"));
    assert_eq!(state.auth_failure_kind, None);
    assert_eq!(state.auth_failure_streak, 0);
    assert_eq!(state.auth_retry_delay, Duration::ZERO);
    assert_eq!(state.auth_issue_published, None);
    assert_eq!(
        auth_issues(&drain(&mut sub).await),
        vec![Some(NapCatWebuiAuthIssue::RateLimited), None]
    );
}

/// 旧逻辑:未登录时 1s 一拍发 RequestAuthRefresh,只有 5s 节流,600s 里约 120 次登录
#[tokio::test(start_paused = true)]
async fn run_poller_rejected_token_backs_off() {
    let client = Arc::new(ScriptedClient::new(vec![], always_rejected));
    let (deps, bus) = deps_with(client.clone());
    let mut sub = bus.subscribe(EventFilter::bot("backoff-bot"));

    let poller = NapCatLoginPoller::spawn(
        BotId::new("backoff-bot"),
        6099,
        "wrong-token".to_string(),
        PollerConfig::default(),
        deps,
    );
    tokio::time::sleep(Duration::from_secs(600)).await;

    let calls = client.calls();
    assert!(
        (3..=5).contains(&calls),
        "600s 内登录次数应被退避压到个位数, got {calls}"
    );
    assert_eq!(
        auth_issues(&drain(&mut sub).await),
        vec![Some(NapCatWebuiAuthIssue::TokenRejected)]
    );
    poller.dispose();
}

#[tokio::test(start_paused = true)]
async fn run_poller_timeouts_then_success_recovers() {
    let client = Arc::new(ScriptedClient::new(
        vec![
            Err(NapCatWebUiError::Timeout),
            Err(NapCatWebUiError::Timeout),
            Ok("cred".into()),
        ],
        always_rejected,
    ));
    let (deps, bus) = deps_with(client.clone());
    let mut sub = bus.subscribe(EventFilter::bot("recover-bot"));

    let poller = NapCatLoginPoller::spawn(
        BotId::new("recover-bot"),
        6099,
        "tok".to_string(),
        PollerConfig::default(),
        deps,
    );
    tokio::time::sleep(Duration::from_secs(60)).await;

    // 第 3 次成功后有凭据,状态查询正常,不再换凭据
    assert_eq!(client.calls(), 3);
    assert!(
        auth_issues(&drain(&mut sub).await).is_empty(),
        "两次超时不该报 Unreachable"
    );
    poller.dispose();
}
