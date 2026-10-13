//! 按主机串行处理断线清理与恢复，不让慢主机挡住其他主机。

use std::collections::{HashMap, VecDeque};

use futures_util::future::BoxFuture;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::events::EventSubscription;

type DebugRecoveryGate = Arc<Mutex<Option<tokio::time::Instant>>>;
pub(super) type DebugRecoveryGates = Arc<Mutex<HashMap<String, DebugRecoveryGate>>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HostChange {
    Lost,
    Recovered,
}

struct HostWork {
    change: HostChange,
    cancel: CancellationToken,
    pending: VecDeque<HostChange>,
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
    pub async fn run_host_connection_listener(self) {
        let subscription = self.event_bus.subscribe(EventFilter::all());
        run_host_changes(subscription, move |server_id, change| {
            let manager = self.clone();
            Box::pin(async move {
                match change {
                    HostChange::Lost => {
                        manager
                            .remote_snowluma_tunnels
                            .drop_server(&server_id)
                            .await;
                        let daemon = manager
                            .remote_snowluma_daemons
                            .lock()
                            .await
                            .get(&server_id)
                            .cloned();
                        if let Some(daemon) = daemon {
                            daemon.detach_local_sessions().await;
                        }
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

async fn run_host_changes(
    mut subscription: EventSubscription,
    mut handle: impl FnMut(String, HostChange) -> BoxFuture<'static, ()>,
) {
    let mut pending: HashMap<String, HostWork> = HashMap::new();
    let mut running: FuturesUnordered<BoxFuture<'static, String>> = FuturesUnordered::new();
    loop {
        tokio::select! {
            event = subscription.next() => {
                let (server_id, change) = match event {
                    Some(DomainEvent::HostConnectionLost { server_id, .. }) => (server_id, HostChange::Lost),
                    Some(DomainEvent::HostConnectionRecovered { server_id, .. }) => (server_id, HostChange::Recovered),
                    Some(_) => continue,
                    None => return,
                };
                if let Some(work) = pending.get_mut(&server_id) {
                    // 已换连接时，不能继续等旧 SSH 命令的超时；清理任务本身仍要按序完成。
                    if change == HostChange::Lost && work.change == HostChange::Recovered {
                        work.cancel.cancel();
                    }
                    let queue = &mut work.pending;
                    // 断线覆盖尚未执行的恢复；之后的恢复仍必须在清理完成后执行。
                    if change == HostChange::Lost {
                        queue.clear();
                    }
                    if queue.back() != Some(&change) {
                        queue.push_back(change);
                    }
                } else {
                    let cancel = CancellationToken::new();
                    pending.insert(server_id.clone(), HostWork {
                        change, cancel: cancel.clone(), pending: VecDeque::new(),
                    });
                    let task = handle(server_id.clone(), change);
                    running.push(host_task(server_id, task, cancel));
                }
            }
            Some(server_id) = running.next(), if !running.is_empty() => {
                let work = pending.get_mut(&server_id).expect("running host registered");
                if let Some(change) = work.pending.pop_front() {
                    work.change = change;
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::{Semaphore, mpsc};

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
        let task = tokio::spawn(run_host_changes(subscription, move |id, change| {
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
        let task = tokio::spawn(run_host_changes(subscription, move |id, change| {
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
