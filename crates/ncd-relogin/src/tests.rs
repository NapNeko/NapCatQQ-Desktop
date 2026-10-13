use super::*;
use ncd_domain::relogin::ReloginSettings;
use std::{
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

struct Fake {
    observations: Mutex<HashMap<String, Observation>>,
    sent: Mutex<Vec<(String, u64, Outgoing)>>,
    results: Mutex<HashMap<String, Delivery>>,
    captures: AtomicUsize,
    fallbacks: AtomicUsize,
    flip_generation: bool,
    scanned: bool,
    current: bool,
    invalidate_after_send: bool,
}
impl Default for Fake {
    fn default() -> Self {
        Self {
            observations: Mutex::new(HashMap::new()),
            sent: Mutex::new(vec![]),
            results: Mutex::new(HashMap::new()),
            captures: AtomicUsize::new(0),
            fallbacks: AtomicUsize::new(0),
            flip_generation: false,
            scanned: false,
            current: true,
            invalidate_after_send: false,
        }
    }
}
impl Fake {
    fn set(&self, id: &str, state: AccountState) -> Observation {
        let observation = Observation {
            bot_id: id.into(),
            name: format!("bot-{id}"),
            generation: "process-1".into(),
            reset_generation: String::new(),
            state,
        };
        self.observations
            .lock()
            .unwrap()
            .insert(id.into(), observation.clone());
        observation
    }
    fn count(&self) -> usize {
        self.sent.lock().unwrap().len()
    }

    fn with_reset(&self, state: AccountState, reset: &str) -> Observation {
        let mut observation = self.set("1", state);
        observation.reset_generation = reset.into();
        self.observations
            .lock()
            .unwrap()
            .insert("1".into(), observation.clone());
        observation
    }
}
#[async_trait]
impl ReloginPort for Fake {
    async fn observe(&self, id: &str) -> Option<Observation> {
        self.observations.lock().unwrap().get(id).cloned()
    }
    async fn qr(&self, id: &str, _refresh: bool) -> Result<QrReply, String> {
        self.captures.fetch_add(1, Ordering::SeqCst);
        let generation = self.observe(id).await.unwrap().generation;
        if self.flip_generation {
            self.observations
                .lock()
                .unwrap()
                .get_mut(id)
                .unwrap()
                .generation = "process-2".into();
        }
        Ok(QrReply {
            generation,
            snapshot: SnowLumaQrLoginSnapshot {
                v: 1,
                state: if self.scanned {
                    SnowLumaQrLoginState::Scanned
                } else {
                    SnowLumaQrLoginState::WaitingForQr
                },
                qq_pid: Some(42),
                qrcode_payload: Some(format!(
                    "data:image/png;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(b"\x89PNG\r\n\x1a\n")
                )),
                uin: None,
                observer_removed: true,
                message: None,
            },
        })
    }
    async fn qr_is_current(&self, _id: &str, _qr: &QrReply) -> bool {
        self.current && (!self.invalidate_after_send || self.count() == 0)
    }
    async fn send(&self, messenger: &str, recipient: u64, message: Outgoing) -> Delivery {
        self.sent
            .lock()
            .unwrap()
            .push((messenger.into(), recipient, message));
        self.results
            .lock()
            .unwrap()
            .get(messenger)
            .cloned()
            .unwrap_or(Delivery::Sent {
                message_id: format!("msg-{recipient}"),
            })
    }
    async fn fallback(&self, _bot: &str) {
        self.fallbacks.fetch_add(1, Ordering::SeqCst);
    }
    async fn close_qr(&self, _bot: &str) {}
}
fn settings() -> ReloginSettings {
    ReloginSettings {
        enabled: true,
        administrator_ids: vec![99],
        ..Default::default()
    }
}
fn manager(path: &std::path::Path) -> ReloginManager {
    let mut manager = ReloginManager::load(path.join("ledger.json")).unwrap();
    manager.configure(settings(), vec!["1".into(), "2".into(), "3".into()]);
    manager
}
async fn offline(manager: &mut ReloginManager, fake: &Fake) {
    fake.set("2", AccountState::Online);
    fake.set("3", AccountState::Online);
    manager
        .tick(&[fake.set("1", AccountState::Online)], 1000, fake)
        .await
        .unwrap();
    manager
        .tick(&[fake.set("1", AccountState::Offline)], 1001, fake)
        .await
        .unwrap();
    manager
        .tick(&[fake.set("1", AccountState::Offline)], 1061, fake)
        .await
        .unwrap();
}
fn command(sender: u64, id: &str, at: i64, command: Command) -> Incoming {
    Incoming {
        messenger: "2".into(),
        sender,
        message_id: id.into(),
        reply_to: None,
        at,
        command,
    }
}

#[tokio::test]
async fn cold_start_and_short_outages_do_not_notify() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = manager(dir.path());
    let f = Fake::default();
    f.set("2", AccountState::Online);
    m.tick(&[f.set("1", AccountState::Offline)], 1000, &f)
        .await
        .unwrap();
    m.tick(&[f.set("1", AccountState::Offline)], 2000, &f)
        .await
        .unwrap();
    m.tick(&[f.set("1", AccountState::Online)], 2001, &f)
        .await
        .unwrap();
    m.tick(&[f.set("1", AccountState::Offline)], 2002, &f)
        .await
        .unwrap();
    m.tick(&[f.set("1", AccountState::Online)], 2030, &f)
        .await
        .unwrap();
    assert_eq!(f.count(), 0);
}

#[tokio::test]
async fn one_notice_survives_restart_and_expiry() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = manager(dir.path());
    let f = Fake::default();
    offline(&mut m, &f).await;
    assert_eq!(f.count(), 1);
    assert_eq!(f.sent.lock().unwrap()[0].0, "2");
    drop(m);
    let mut m = manager(dir.path());
    for now in [2000, 2061, 3000] {
        m.tick(&[f.set("1", AccountState::Offline)], now, &f)
            .await
            .unwrap();
    }
    assert_eq!(f.count(), 1);
    let ledger = std::fs::read_to_string(dir.path().join("ledger.json")).unwrap();
    assert!(!ledger.contains("base64"));
}

#[tokio::test]
async fn uncertain_send_never_fails_over() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = manager(dir.path());
    let f = Fake::default();
    f.results
        .lock()
        .unwrap()
        .insert("2".into(), Delivery::Unknown);
    offline(&mut m, &f).await;
    m.tick(&[f.set("1", AccountState::Offline)], 3000, &f)
        .await
        .unwrap();
    assert_eq!(f.count(), 1);
    assert_eq!(f.fallbacks.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn definitive_rejection_uses_next_messenger() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = manager(dir.path());
    let f = Fake::default();
    f.results
        .lock()
        .unwrap()
        .insert("2".into(), Delivery::Rejected);
    offline(&mut m, &f).await;
    assert_eq!(f.count(), 2);
    assert_eq!(f.sent.lock().unwrap()[1].0, "3");
}

#[tokio::test]
async fn failover_rechecks_qr_after_first_messenger_rejects() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = manager(dir.path());
    let f = Fake {
        invalidate_after_send: true,
        ..Default::default()
    };
    f.results
        .lock()
        .unwrap()
        .insert("2".into(), Delivery::Rejected);
    offline(&mut m, &f).await;
    let sent = f.sent.lock().unwrap();
    assert_eq!(sent.len(), 2);
    assert!(sent[0].2.image.is_some());
    assert!(sent[1].2.image.is_none());
}

#[tokio::test]
async fn commands_require_admin_and_replays_do_not_refresh() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = manager(dir.path());
    let f = Fake::default();
    offline(&mut m, &f).await;
    m.command(
        command(7, "unauthorized", 1100, Command::Refresh(Some("1".into()))),
        1100,
        &f,
    )
    .await
    .unwrap();
    let cmd = command(99, "refresh", 1100, Command::Refresh(Some("1".into())));
    m.command(cmd.clone(), 1100, &f).await.unwrap();
    m.command(cmd, 1105, &f).await.unwrap();
    m.command(
        command(99, "again", 1106, Command::Refresh(Some("1".into()))),
        1106,
        &f,
    )
    .await
    .unwrap();
    assert_eq!(f.captures.load(Ordering::SeqCst), 2);
    assert_eq!(f.count(), 2);
}

#[tokio::test]
async fn administrators_share_qr_without_refreshing_twice() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = manager(dir.path());
    let f = Fake::default();
    let mut options = settings();
    options.administrator_ids.push(100);
    m.configure(options, vec!["2".into()]);
    offline(&mut m, &f).await;
    m.command(
        command(99, "a", 1100, Command::Refresh(Some("1".into()))),
        1100,
        &f,
    )
    .await
    .unwrap();
    m.command(
        command(100, "b", 1102, Command::Refresh(Some("1".into()))),
        1102,
        &f,
    )
    .await
    .unwrap();
    assert_eq!(f.captures.load(Ordering::SeqCst), 2);
    assert_eq!(f.count(), 4);
}

#[tokio::test]
async fn stale_or_scanned_qr_is_never_sent() {
    for f in [
        Fake {
            flip_generation: true,
            ..Default::default()
        },
        Fake {
            scanned: true,
            ..Default::default()
        },
        Fake {
            current: false,
            ..Default::default()
        },
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut m = manager(dir.path());
        offline(&mut m, &f).await;
        assert_eq!(f.count(), 1);
        assert!(f.sent.lock().unwrap()[0].2.image.is_none());
    }
}

#[tokio::test]
async fn unknown_resets_confirmation_and_stop_discards_incident() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = manager(dir.path());
    let f = Fake::default();
    f.set("2", AccountState::Online);
    for (now, state) in [
        (1000, AccountState::Online),
        (1001, AccountState::Offline),
        (1050, AccountState::Unknown),
        (1061, AccountState::Offline),
        (1100, AccountState::Stopped),
        (1200, AccountState::Offline),
        (1400, AccountState::Offline),
    ] {
        m.tick(&[f.set("1", state)], now, &f).await.unwrap();
    }
    assert_eq!(f.count(), 0);
}

#[tokio::test]
async fn rapid_manual_restart_cancels_old_incident_before_next_poll() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = manager(dir.path());
    let f = Fake::default();
    offline(&mut m, &f).await;
    f.with_reset(AccountState::Offline, "manual-stop-1");
    m.command(
        command(99, "late", 1100, Command::Refresh(Some("1".into()))),
        1100,
        &f,
    )
    .await
    .unwrap();
    assert_eq!(f.captures.load(Ordering::SeqCst), 1);
    let count = f.count();
    for now in [1200, 1300] {
        m.tick(
            &[f.with_reset(AccountState::Offline, "manual-stop-1")],
            now,
            &f,
        )
        .await
        .unwrap();
    }
    assert_eq!(f.count(), count);
}

#[tokio::test]
async fn unknown_probe_does_not_forget_manual_restart_generation() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = manager(dir.path());
    let f = Fake::default();
    f.set("2", AccountState::Online);
    for (now, state, reset) in [
        (1000, AccountState::Online, "manual-stop-1"),
        (1001, AccountState::Unknown, ""),
        (1100, AccountState::Offline, "manual-stop-1"),
        (1160, AccountState::Offline, "manual-stop-1"),
    ] {
        m.tick(&[f.with_reset(state, reset)], now, &f)
            .await
            .unwrap();
    }
    assert_eq!(f.count(), 1);
}

#[tokio::test]
async fn participant_receives_one_recovery_after_stable_online() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = manager(dir.path());
    let f = Fake::default();
    offline(&mut m, &f).await;
    m.command(
        command(99, "refresh", 1100, Command::Refresh(None)),
        1100,
        &f,
    )
    .await
    .unwrap();
    m.tick(&[f.set("1", AccountState::Online)], 1120, &f)
        .await
        .unwrap();
    m.tick(&[f.set("1", AccountState::Online)], 1240, &f)
        .await
        .unwrap();
    m.tick(&[f.set("1", AccountState::Online)], 1300, &f)
        .await
        .unwrap();
    assert_eq!(f.count(), 3);
    assert!(f.sent.lock().unwrap()[2].2.text.contains("已恢复"));
}

#[test]
fn ledger_is_exclusive_and_corruption_is_not_silently_reset() {
    let dir = tempfile::tempdir().unwrap();
    let m = manager(dir.path());
    assert!(ReloginManager::load(dir.path().join("ledger.json")).is_err());
    drop(m);
    std::fs::write(dir.path().join("ledger.json"), b"broken").unwrap();
    assert!(ReloginManager::load(dir.path().join("ledger.json")).is_err());
}

#[tokio::test]
async fn no_messenger_falls_back_once_and_does_not_touch_qr() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = manager(dir.path());
    let f = Fake::default();
    m.tick(&[f.set("1", AccountState::Online)], 1000, &f)
        .await
        .unwrap();
    for now in [1001, 1061, 1300] {
        m.tick(&[f.set("1", AccountState::Offline)], now, &f)
            .await
            .unwrap();
    }
    assert_eq!(f.captures.load(Ordering::SeqCst), 0);
    assert_eq!(f.fallbacks.load(Ordering::SeqCst), 1);
    m.tick(&[f.set("1", AccountState::Online)], 1400, &f)
        .await
        .unwrap();
    f.set("2", AccountState::Online);
    m.tick(&[f.set("1", AccountState::Online)], 1520, &f)
        .await
        .unwrap();
    assert_eq!(f.count(), 0);
}
