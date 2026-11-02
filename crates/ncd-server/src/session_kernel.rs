//! 远端连接的唯一登记表；快照保留最新代次，不依赖逐条广播。

use crate::ServerState;
use ncd_host::Host;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;
use tokio::sync::{Mutex as AsyncMutex, watch};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionSnapshot {
    pub generation: u64,
    pub revision: u64,
    pub recovery_requested: u64,
    pub state: ServerState,
}

pub type ConnectionSnapshots = HashMap<String, ConnectionSnapshot>;
pub type ConnectionUpdates = watch::Receiver<ConnectionSnapshots>;

pub struct ConnectionObserver {
    updates: ConnectionUpdates,
    seen: ConnectionSnapshots,
    pending: std::collections::VecDeque<(String, ConnectionSnapshot)>,
    include_requests: bool,
}

impl ConnectionObserver {
    pub fn new(updates: ConnectionUpdates) -> Self {
        Self {
            updates,
            seen: HashMap::new(),
            pending: Default::default(),
            include_requests: false,
        }
    }

    pub fn with_recovery_requests(mut self) -> Self {
        self.include_requests = true;
        self
    }

    pub async fn next(&mut self) -> Option<(String, ConnectionSnapshot)> {
        loop {
            if let Some(change) = self.pending.pop_front() {
                return Some(change);
            }
            let current = self.updates.borrow_and_update().clone();
            for (id, snapshot) in &current {
                if self.seen.get(id).is_none_or(|previous| {
                    previous.generation != snapshot.generation
                        || previous.revision != snapshot.revision
                        || previous.state != snapshot.state
                        || (self.include_requests
                            && previous.recovery_requested != snapshot.recovery_requested)
                }) {
                    self.pending.push_back((id.clone(), snapshot.clone()));
                }
            }
            for id in self.seen.keys().filter(|id| !current.contains_key(*id)) {
                self.pending.push_back((
                    id.clone(),
                    ConnectionSnapshot {
                        generation: 0,
                        revision: 0,
                        recovery_requested: 0,
                        state: ServerState::Disconnected,
                    },
                ));
            }
            self.seen = current;
            if self.pending.is_empty() && self.updates.changed().await.is_err() {
                return None;
            }
        }
    }
}

#[derive(Default)]
struct Session {
    generation: u64,
    host: Option<Arc<dyn Host>>,
    connect: Arc<AsyncMutex<()>>,
    retry_at: Option<Instant>,
}

#[derive(Default)]
struct Sessions {
    next_generation: u64,
    by_id: HashMap<String, Session>,
}

pub(crate) struct SessionKernel {
    sessions: Mutex<Sessions>,
    updates: watch::Sender<ConnectionSnapshots>,
}

impl Default for SessionKernel {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(Sessions::default()),
            updates: watch::channel(HashMap::new()).0,
        }
    }
}

impl SessionKernel {
    fn lock(&self) -> MutexGuard<'_, Sessions> {
        self.sessions
            .lock()
            .expect("remote session kernel poisoned")
    }

    pub fn subscribe(&self) -> ConnectionUpdates {
        self.updates.subscribe()
    }

    pub fn host(&self, id: &str) -> Option<Arc<dyn Host>> {
        self.lock().by_id.get(id)?.host.clone()
    }

    pub fn connect_lock(&self, id: &str) -> Arc<AsyncMutex<()>> {
        self.lock()
            .by_id
            .entry(id.into())
            .or_default()
            .connect
            .clone()
    }

    pub fn begin(&self, id: &str) -> u64 {
        let mut sessions = self.lock();
        sessions.next_generation += 1;
        let generation = sessions.next_generation;
        sessions.by_id.entry(id.into()).or_default().generation = generation;
        self.publish(id, generation, ServerState::Connecting);
        generation
    }

    pub fn install(self: &Arc<Self>, id: &str, generation: u64, host: Arc<dyn Host>) -> bool {
        let mut sessions = self.lock();
        let Some(session) = sessions
            .by_id
            .get_mut(id)
            .filter(|s| s.generation == generation)
        else {
            return false;
        };
        let old = session.host.replace(Arc::clone(&host));
        session.retry_at = None;
        self.publish(id, generation, ServerState::Connected);
        drop(sessions);
        if let Some(host) = old {
            crate::server_manager::retire_host(host);
        }
        if let Some(mut changes) = host.transport_changes() {
            let kernel = Arc::downgrade(self);
            let host = Arc::downgrade(&host);
            let id = id.to_owned();
            tokio::spawn(async move {
                while changes.changed().await.is_ok() {
                    let (Some(kernel), Some(host)) = (kernel.upgrade(), host.upgrade()) else {
                        break;
                    };
                    if !kernel.notify_recovery(&id, &host, false) {
                        break;
                    }
                }
            });
        }
        true
    }

    pub fn request_recovery(&self, id: &str, expected: &Arc<dyn Host>) -> bool {
        self.notify_recovery(id, expected, true)
    }

    fn notify_recovery(&self, id: &str, expected: &Arc<dyn Host>, request_only: bool) -> bool {
        let sessions = self.lock();
        if !sessions
            .by_id
            .get(id)
            .and_then(|s| s.host.as_ref())
            .is_some_and(|host| Arc::ptr_eq(host, expected))
        {
            return false;
        }
        self.updates.send_if_modified(|states| {
            let Some(snapshot) = states
                .get_mut(id)
                .filter(|s| s.state == ServerState::Connected)
            else {
                return false;
            };
            if request_only {
                snapshot.recovery_requested += 1;
            } else {
                snapshot.revision += 1;
            }
            true
        })
    }

    pub fn fail(&self, id: &str, generation: u64, retry_at: Instant) -> bool {
        let mut sessions = self.lock();
        let Some(session) = sessions
            .by_id
            .get_mut(id)
            .filter(|s| s.generation == generation)
        else {
            return false;
        };
        session.retry_at = Some(retry_at);
        let old = session.host.take();
        self.publish(id, generation, ServerState::Failed);
        drop(sessions);
        if let Some(host) = old {
            crate::server_manager::retire_host(host);
        }
        true
    }

    pub fn is_current(&self, id: &str, generation: u64) -> bool {
        self.lock()
            .by_id
            .get(id)
            .is_some_and(|s| s.generation == generation)
    }

    // 具体探测只能驱逐捕获的连接；手动断开和档案变更传 None。
    pub fn invalidate(&self, id: &str, expected: Option<&Arc<dyn Host>>) -> Option<Arc<dyn Host>> {
        let mut sessions = self.lock();
        if let Some(expected) = expected {
            let current = sessions.by_id.get(id)?.host.as_ref()?;
            if !Arc::ptr_eq(current, expected) {
                return None;
            }
        }
        sessions.next_generation += 1;
        let generation = sessions.next_generation;
        let session = sessions.by_id.entry(id.into()).or_default();
        session.generation = generation;
        let host = session.host.take();
        self.publish(id, generation, ServerState::Disconnected);
        host
    }

    pub fn remove(&self, id: &str) -> Option<Arc<dyn Host>> {
        let mut sessions = self.lock();
        let host = sessions.by_id.remove(id).and_then(|s| s.host);
        self.updates.send_modify(|states| {
            states.remove(id);
        });
        host
    }

    pub fn retry_at(&self, id: &str) -> Option<Instant> {
        self.lock().by_id.get(id).and_then(|s| s.retry_at)
    }

    pub fn set_retry(&self, id: &str, retry_at: Option<Instant>) {
        self.lock().by_id.entry(id.into()).or_default().retry_at = retry_at;
    }

    pub fn prune_retries(&self) {
        for session in self.lock().by_id.values_mut() {
            if session.retry_at.is_some_and(|at| at <= Instant::now()) {
                session.retry_at = None;
            }
        }
    }

    fn publish(&self, id: &str, generation: u64, state: ServerState) {
        self.updates.send_modify(|states| {
            states.insert(
                id.into(),
                ConnectionSnapshot {
                    generation,
                    revision: 0,
                    recovery_requested: 0,
                    state,
                },
            );
        });
    }

    #[cfg(test)]
    pub fn contains(&self, id: &str) -> bool {
        self.lock().by_id.contains_key(id)
    }

    #[cfg(test)]
    pub fn retry_count(&self) -> usize {
        self.lock()
            .by_id
            .values()
            .filter(|s| s.retry_at.is_some())
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn late_connection_cannot_replace_a_new_generation_or_deleted_profile() {
        let kernel = Arc::new(SessionKernel::default());
        let first = kernel.begin("server");
        let next = kernel.begin("server");
        let host = ncd_host::local::local_host();
        assert!(!kernel.install("server", first, host.clone()));
        assert!(kernel.install("server", next, host.clone()));
        kernel.remove("server");
        assert!(!kernel.install("server", next, host));
        assert!(kernel.subscribe().borrow().is_empty());
    }

    #[tokio::test]
    async fn slow_subscriber_observes_latest_state_without_replaying_events() {
        let kernel = Arc::new(SessionKernel::default());
        let mut updates = kernel.subscribe();
        for _ in 0..2048 {
            kernel.begin("server");
        }
        let generation = kernel.begin("server");
        assert!(kernel.install("server", generation, ncd_host::local::local_host()));
        updates.changed().await.unwrap();
        assert_eq!(
            updates.borrow()["server"],
            ConnectionSnapshot {
                generation,
                revision: 0,
                recovery_requested: 0,
                state: ServerState::Connected
            }
        );
    }

    #[tokio::test]
    async fn failure_from_an_old_host_cannot_disconnect_the_replacement() {
        let kernel = Arc::new(SessionKernel::default());
        let old = ncd_host::local::local_host();
        let current = ncd_host::local::local_host();
        let first = kernel.begin("server");
        assert!(kernel.install("server", first, old.clone()));
        let next = kernel.begin("server");
        assert!(kernel.install("server", next, current.clone()));
        assert!(kernel.invalidate("server", Some(&old)).is_none());
        assert!(Arc::ptr_eq(&kernel.host("server").unwrap(), &current));
        assert_eq!(
            kernel.subscribe().borrow()["server"].state,
            ServerState::Connected
        );
    }

    #[tokio::test(start_paused = true)]
    async fn recovery_requests_wake_the_owner_without_invalidating_consumers() {
        let kernel = Arc::new(SessionKernel::default());
        let host = ncd_host::local::local_host();
        let generation = kernel.begin("server");
        kernel.install("server", generation, host.clone());
        let mut consumer = ConnectionObserver::new(kernel.subscribe());
        let mut owner = ConnectionObserver::new(kernel.subscribe()).with_recovery_requests();
        consumer.next().await.unwrap();
        owner.next().await.unwrap();
        for _ in 0..20 {
            assert!(kernel.request_recovery("server", &host));
        }
        let (_, request) = owner.next().await.unwrap();
        assert_eq!(request.recovery_requested, 20);
        assert_eq!(request.revision, 0);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(1), consumer.next())
                .await
                .is_err()
        );
        assert!(kernel.notify_recovery("server", &host, false));
        assert_eq!(consumer.next().await.unwrap().1.revision, 1);
    }
}
