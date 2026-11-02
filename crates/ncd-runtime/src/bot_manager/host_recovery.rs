//! 按主机串行处理断线清理与恢复，不让慢主机挡住其他主机。

use std::collections::{HashMap, VecDeque};

use futures_util::future::BoxFuture;
use futures_util::stream::{FuturesUnordered, StreamExt};
use ncd_traits::{BotConfigRepo, ConfigStore};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use super::BotManager;
use crate::events::EventSubscription;
use crate::events::{DomainEvent, EventBus, EventFilter};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostChange {
    Lost,
    Recovered,
}

struct HostWork {
    change: HostChange,
    version: (u64, u64),
    cancel: CancellationToken,
    pending: VecDeque<(HostChange, (u64, u64))>,
}

fn host_task(
    server_id: String,
    task: BoxFuture<'static, ()>,
    cancel: CancellationToken,
) -> BoxFuture<'static, String> {
    Box::pin(async move {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {}
            _ = task => {}
        }
        server_id
    })
}

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> BotManager<R, S> {
    pub async fn run_host_connection_listener(
        self,
        ready: Option<tokio::sync::oneshot::Sender<()>>,
    ) {
        let subscription = self.event_bus.subscribe(EventFilter::all());
        if let Some(tx) = ready {
            let _ = tx.send(());
        }
        let updates = self
            .server_manager
            .as_ref()
            .map(|servers| servers.connection_updates())
            .or_else(|| {
                self.host_resolver
                    .as_ref()
                    .and_then(|resolver| resolver.connection_updates())
            });
        let connections = updates
            .map(|updates| ncd_server::ConnectionObserver::new(updates).with_recovery_requests());
        run_host_changes(subscription, connections, move |server_id, change| {
            let manager = self.clone();
            Box::pin(async move {
                match change {
                    HostChange::Lost => {
                        manager.detach_remote_snowluma_sessions(&server_id).await;
                    }
                    HostChange::Recovered => {
                        let started = tokio::time::Instant::now();
                        match manager
                            .reconcile_remote_runtimes_for_server(&server_id)
                            .await
                        {
                            Ok(done) => info!(
                                server_id,
                                reconciled = done.len(),
                                elapsed_ms = started.elapsed().as_millis(),
                                "主机连接恢复：远端运行态接管完成"
                            ),
                            Err(err) => warn!(server_id, %err, "主机恢复后的运行态接管失败"),
                        }
                    }
                }
            })
        })
        .await;
    }
}

pub(crate) async fn run_host_changes(
    mut subscription: EventSubscription,
    mut connections: Option<ncd_server::ConnectionObserver>,
    mut handle: impl FnMut(String, HostChange) -> BoxFuture<'static, ()>,
) {
    let mut pending: HashMap<String, HostWork> = HashMap::new();
    let mut running: FuturesUnordered<BoxFuture<'static, String>> = FuturesUnordered::new();
    loop {
        tokio::select! {
            change = next_host_change(&mut subscription, &mut connections) => {
                let Some((server_id, change, version)) = change else { return; };
                if let Some(work) = pending.get_mut(&server_id) {
                    // 已换连接时，不能继续等旧 SSH 命令的超时；清理任务本身仍要按序完成。
                    if (change == HostChange::Lost || version != work.version) && work.change == HostChange::Recovered {
                        work.cancel.cancel();
                    }
                    let queue = &mut work.pending;
                    // 断线覆盖尚未执行的恢复；之后的恢复仍必须在清理完成后执行。
                    if change == HostChange::Lost || version != work.version {
                        queue.clear();
                    }
                    if queue.back() != Some(&(change, version)) {
                        queue.push_back((change, version));
                    }
                } else {
                    let cancel = CancellationToken::new();
                    pending.insert(server_id.clone(), HostWork {
                        change, version, cancel: cancel.clone(), pending: VecDeque::new(),
                    });
                    let task = handle(server_id.clone(), change);
                    running.push(host_task(server_id, task, cancel));
                }
            }
            Some(server_id) = running.next(), if !running.is_empty() => {
                let work = pending.get_mut(&server_id).expect("running host registered");
                if let Some((change, version)) = work.pending.pop_front() {
                    work.change = change;
                    work.version = version;
                    work.cancel = CancellationToken::new();
                    let task = handle(server_id.clone(), change);
                    running.push(host_task(server_id, task, work.cancel.clone()));
                } else {
                    pending.remove(&server_id);
                }
            }
        }
    }
}

async fn next_host_change(
    events: &mut EventSubscription,
    connections: &mut Option<ncd_server::ConnectionObserver>,
) -> Option<(String, HostChange, (u64, u64))> {
    if let Some(connections) = connections {
        loop {
            let (id, snapshot) = connections.next().await?;
            let version = (snapshot.generation, snapshot.revision);
            match snapshot.state {
                ncd_server::ServerState::Connected => {
                    return Some((id, HostChange::Recovered, version));
                }
                ncd_server::ServerState::Disconnected | ncd_server::ServerState::Failed => {
                    return Some((id, HostChange::Lost, version));
                }
                ncd_server::ServerState::Connecting => {}
            }
        }
    }
    loop {
        match events.next().await? {
            DomainEvent::HostConnectionLost { server_id, .. } => {
                return Some((server_id, HostChange::Lost, (0, 0)));
            }
            DomainEvent::HostConnectionRecovered { server_id, .. } => {
                return Some((server_id, HostChange::Recovered, (0, 0)));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::BroadcastEventBus;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::{Semaphore, mpsc};

    #[tokio::test]
    async fn newer_connected_snapshot_cancels_old_recovery_even_when_lost_was_coalesced() {
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Held(Arc<AtomicBool>);
        impl Drop for Held {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let bus = BroadcastEventBus::default();
        let (states, updates) = tokio::sync::watch::channel(ncd_server::ConnectionSnapshots::new());
        let released = Arc::new(AtomicBool::new(false));
        let release = released.clone();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut attempt = 0;
        let task = tokio::spawn(run_host_changes(
            bus.subscribe(EventFilter::all()),
            Some(ncd_server::ConnectionObserver::new(updates)),
            move |id, change| {
                let current = attempt;
                attempt += 1;
                let tx = tx.clone();
                let released = release.clone();
                Box::pin(async move {
                    if current == 0 {
                        let _held = Held(released);
                        tx.send((id, change)).unwrap();
                        std::future::pending::<()>().await;
                    } else {
                        assert!(released.load(Ordering::SeqCst));
                        tx.send((id, change)).unwrap();
                    }
                })
            },
        ));
        for generation in [1, 2] {
            states.send_modify(|states| {
                states.insert(
                    "server".into(),
                    ncd_server::ConnectionSnapshot {
                        generation,
                        revision: 0,
                        recovery_requested: 0,
                        state: ncd_server::ServerState::Connected,
                    },
                );
            });
            assert_eq!(
                next(&mut rx).await,
                ("server".into(), HostChange::Recovered)
            );
        }
        assert!(released.load(Ordering::SeqCst));
        task.abort();
    }

    fn publish(bus: &BroadcastEventBus, id: &str, change: HostChange) {
        bus.publish(match change {
            HostChange::Lost => DomainEvent::HostConnectionLost {
                server_id: id.into(),
                reason: None,
                consecutive_failures: 1,
            },
            HostChange::Recovered => DomainEvent::HostConnectionRecovered {
                server_id: id.into(),
                latency_ms: 1,
            },
        });
    }

    async fn next(rx: &mut mpsc::UnboundedReceiver<(String, HostChange)>) -> (String, HostChange) {
        tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap()
    }

    #[tokio::test]
    async fn slow_cleanup_does_not_block_other_hosts_and_recovery_waits_for_cleanup() {
        let bus = BroadcastEventBus::default();
        let subscription = bus.subscribe(EventFilter::all());
        let gate = Arc::new(Semaphore::new(0));
        let wait = Arc::clone(&gate);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(run_host_changes(subscription, None, move |id, change| {
            let gate = Arc::clone(&wait);
            let tx = tx.clone();
            Box::pin(async move {
                tx.send((id.clone(), change)).unwrap();
                if id == "slow" && change == HostChange::Lost {
                    gate.acquire().await.unwrap().forget();
                }
            })
        }));
        publish(&bus, "slow", HostChange::Lost);
        assert_eq!(next(&mut rx).await, ("slow".into(), HostChange::Lost));
        publish(&bus, "slow", HostChange::Recovered);
        publish(&bus, "fast", HostChange::Recovered);
        assert_eq!(next(&mut rx).await, ("fast".into(), HostChange::Recovered));
        assert!(rx.try_recv().is_err());
        gate.add_permits(1);
        assert_eq!(next(&mut rx).await, ("slow".into(), HostChange::Recovered));

        // 30 秒内再次断线、恢复，也必须完整执行，不能当作去抖噪声丢掉。
        publish(&bus, "fast", HostChange::Lost);
        publish(&bus, "fast", HostChange::Recovered);
        assert_eq!(next(&mut rx).await, ("fast".into(), HostChange::Lost));
        assert_eq!(next(&mut rx).await, ("fast".into(), HostChange::Recovered));
        task.abort();
    }

    #[tokio::test]
    async fn recovery_bursts_coalesce_without_losing_a_later_disconnect() {
        let bus = BroadcastEventBus::default();
        let subscription = bus.subscribe(EventFilter::all());
        let gate = Arc::new(Semaphore::new(0));
        let wait = Arc::clone(&gate);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(run_host_changes(subscription, None, move |id, change| {
            let gate = Arc::clone(&wait);
            let tx = tx.clone();
            Box::pin(async move {
                tx.send((id, change)).unwrap();
                gate.acquire().await.unwrap().forget();
            })
        }));
        publish(&bus, "host", HostChange::Recovered);
        assert_eq!(next(&mut rx).await.1, HostChange::Recovered);
        for _ in 0..30 {
            publish(&bus, "host", HostChange::Recovered);
        }
        publish(&bus, "host", HostChange::Lost);
        publish(&bus, "host", HostChange::Recovered);
        tokio::task::yield_now().await;
        assert_eq!(next(&mut rx).await.1, HostChange::Lost);
        gate.add_permits(1);
        assert_eq!(next(&mut rx).await.1, HostChange::Recovered);
        gate.add_permits(1);
        tokio::task::yield_now().await;
        assert!(rx.try_recv().is_err());
        task.abort();
    }
}
