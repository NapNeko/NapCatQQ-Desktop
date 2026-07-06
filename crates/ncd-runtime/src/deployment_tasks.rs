use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::SystemTime;

use ncd_domain::{
    DeploymentTaskKind, DeploymentTaskList, DeploymentTaskResource, DeploymentTaskSnapshot,
    DeploymentTaskStatus, ProgressEvent,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::events::{BroadcastEventBus, DomainEvent, EventBus};

const DEFAULT_MAX_DOWNLOAD_TASKS: usize = 2;
const MAX_PROGRESS_EVENTS_PER_TASK: usize = 260;

pub type DeploymentTaskFuture = Pin<Box<dyn Future<Output = DeploymentTaskRunResult> + Send>>;
pub type DeploymentTaskRunner =
    Box<dyn FnOnce(DeploymentTaskContext) -> DeploymentTaskFuture + Send + 'static>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentTaskRunResult {
    pub ok: bool,
    pub message: Option<String>,
    pub error: Option<String>,
}

impl DeploymentTaskRunResult {
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            ok: true,
            message: Some(message.into()),
            error: None,
        }
    }

    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: None,
            error: Some(error.into()),
        }
    }
}

pub struct DeploymentTaskRequest {
    pub task_id: String,
    pub kind: DeploymentTaskKind,
    pub host_id: String,
    pub title: String,
    pub resources: Vec<DeploymentTaskResource>,
    pub dedupe_key: Option<String>,
    pub cancellable: bool,
    pub runner: DeploymentTaskRunner,
}

#[derive(Clone)]
pub struct DeploymentTaskContext {
    task_id: String,
    manager: DeploymentTaskManager,
    cancel: CancellationToken,
}

impl DeploymentTaskContext {
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    pub async fn push_progress(&self, event: ProgressEvent) {
        self.manager.push_progress(&self.task_id, event).await;
    }
}

#[derive(Clone)]
pub struct DeploymentTaskManager {
    inner: Arc<Mutex<DeploymentTaskState>>,
    event_bus: BroadcastEventBus,
}

struct DeploymentTaskState {
    tasks: HashMap<String, DeploymentTaskRecord>,
    order: VecDeque<String>,
    pending: VecDeque<String>,
    resource_owners: HashMap<String, String>,
    download_slots_used: usize,
    max_download_slots: usize,
}

struct DeploymentTaskRecord {
    snapshot: DeploymentTaskSnapshot,
    runner: Option<DeploymentTaskRunner>,
    cancel: CancellationToken,
}

struct ReadyTask {
    task_id: String,
    runner: DeploymentTaskRunner,
    cancel: CancellationToken,
    snapshot: DeploymentTaskSnapshot,
}

impl DeploymentTaskManager {
    pub fn new(event_bus: BroadcastEventBus) -> Self {
        Self {
            inner: Arc::new(Mutex::new(DeploymentTaskState {
                tasks: HashMap::new(),
                order: VecDeque::new(),
                pending: VecDeque::new(),
                resource_owners: HashMap::new(),
                download_slots_used: 0,
                max_download_slots: DEFAULT_MAX_DOWNLOAD_TASKS,
            })),
            event_bus,
        }
    }

    pub async fn submit(&self, request: DeploymentTaskRequest) -> String {
        let mut snapshots = Vec::new();
        let task_id = {
            let mut state = self.inner.lock().await;
            if let Some(key) = request.dedupe_key.as_deref() {
                if let Some(existing) = state.active_task_by_dedupe_key(key) {
                    return existing;
                }
            }

            let task_id = request.task_id.clone();
            let snapshot = DeploymentTaskSnapshot {
                task_id: task_id.clone(),
                kind: request.kind,
                status: DeploymentTaskStatus::Queued,
                host_id: request.host_id,
                title: request.title,
                dedupe_key: request.dedupe_key,
                resources: request.resources,
                progress_events: Vec::new(),
                submitted_at_ms: now_ms(),
                started_at_ms: None,
                ended_at_ms: None,
                message: Some("已进入队列".to_string()),
                error: None,
                cancellable: request.cancellable,
            };
            snapshots.push(snapshot.clone());
            let record = DeploymentTaskRecord {
                snapshot,
                runner: Some(request.runner),
                cancel: CancellationToken::new(),
            };
            state.order.push_back(task_id.clone());
            state.pending.push_back(task_id.clone());
            state.tasks.insert(task_id.clone(), record);
            task_id
        };

        self.publish_many(snapshots);
        self.dispatch_ready_tasks().await;
        task_id
    }

    pub async fn list(&self) -> DeploymentTaskList {
        let state = self.inner.lock().await;
        let tasks = state
            .order
            .iter()
            .filter_map(|id| state.tasks.get(id))
            .map(|r| r.snapshot.clone())
            .collect();
        DeploymentTaskList { tasks }
    }

    pub async fn cancel(&self, task_id: &str) -> Result<(), String> {
        let mut to_publish = None;
        let mut needs_dispatch = false;
        {
            let mut state = self.inner.lock().await;
            let Some(record) = state.tasks.get_mut(task_id) else {
                return Err(format!("task not found: {task_id}"));
            };
            match record.snapshot.status {
                DeploymentTaskStatus::Queued | DeploymentTaskStatus::WaitingInput => {
                    record.cancel.cancel();
                    record.runner = None;
                    record.snapshot.status = DeploymentTaskStatus::Cancelled;
                    record.snapshot.message = Some("已取消".to_string());
                    record.snapshot.ended_at_ms = Some(now_ms());
                    state.pending.retain(|id| id != task_id);
                    to_publish = state.tasks.get(task_id).map(|r| r.snapshot.clone());
                    needs_dispatch = true;
                }
                DeploymentTaskStatus::Running => {
                    record.cancel.cancel();
                    record.snapshot.message = Some("正在取消...".to_string());
                    to_publish = Some(record.snapshot.clone());
                }
                status if status.is_terminal() => {}
                _ => {}
            }
        }

        if let Some(snapshot) = to_publish {
            self.publish(snapshot);
        }
        if needs_dispatch {
            self.dispatch_ready_tasks().await;
        }
        Ok(())
    }

    pub async fn push_progress(&self, task_id: &str, event: ProgressEvent) {
        let snapshot = {
            let mut state = self.inner.lock().await;
            let Some(record) = state.tasks.get_mut(task_id) else {
                return;
            };
            record.snapshot.progress_events.push(event);
            let excess = record
                .snapshot
                .progress_events
                .len()
                .saturating_sub(MAX_PROGRESS_EVENTS_PER_TASK);
            if excess > 0 {
                record.snapshot.progress_events.drain(0..excess);
            }
            record.snapshot.clone()
        };
        self.publish(snapshot);
    }

    async fn finish(&self, task_id: &str, result: DeploymentTaskRunResult) {
        let snapshot = {
            let mut state = self.inner.lock().await;
            let resources = match state.tasks.get(task_id) {
                Some(record) => record.snapshot.resources.clone(),
                None => return,
            };
            state.release_resources(&resources, task_id);

            let Some(record) = state.tasks.get_mut(task_id) else {
                return;
            };
            let cancelled = record.cancel.is_cancelled();
            record.snapshot.status = if cancelled {
                DeploymentTaskStatus::Cancelled
            } else if result.ok {
                DeploymentTaskStatus::Success
            } else {
                DeploymentTaskStatus::Failed
            };
            record.snapshot.ended_at_ms = Some(now_ms());
            record.snapshot.message = result.message;
            record.snapshot.error = result.error;
            record.snapshot.cancellable = false;
            record.snapshot.clone()
        };
        self.publish(snapshot);
    }

    async fn finish_and_dispatch(&self, task_id: &str, result: DeploymentTaskRunResult) {
        self.finish(task_id, result).await;
        let ready = self.take_ready_tasks().await;
        self.spawn_ready_tasks(ready);
    }

    async fn dispatch_ready_tasks(&self) {
        let ready = self.take_ready_tasks().await;
        self.spawn_ready_tasks(ready);
    }

    async fn take_ready_tasks(&self) -> Vec<ReadyTask> {
        let mut ready_tasks = Vec::new();
        loop {
            let ready = {
                let mut state = self.inner.lock().await;
                state.take_next_ready_task()
            };
            let Some(ready) = ready else {
                break;
            };
            ready_tasks.push(ready);
        }
        ready_tasks
    }

    fn spawn_ready_tasks(&self, ready_tasks: Vec<ReadyTask>) {
        for ready in ready_tasks {
            self.publish(ready.snapshot.clone());
            let manager = self.clone();
            tokio::spawn(async move {
                let ctx = DeploymentTaskContext {
                    task_id: ready.task_id.clone(),
                    manager: manager.clone(),
                    cancel: ready.cancel,
                };
                let result = (ready.runner)(ctx).await;
                manager.finish_and_dispatch(&ready.task_id, result).await;
            });
        }
    }

    fn publish(&self, snapshot: DeploymentTaskSnapshot) {
        self.event_bus
            .publish(DomainEvent::deployment_task_changed(snapshot));
    }

    fn publish_many(&self, snapshots: Vec<DeploymentTaskSnapshot>) {
        for snapshot in snapshots {
            self.publish(snapshot);
        }
    }
}

impl DeploymentTaskState {
    fn active_task_by_dedupe_key(&self, key: &str) -> Option<String> {
        self.tasks
            .values()
            .find(|record| {
                record.snapshot.dedupe_key.as_deref() == Some(key)
                    && record.snapshot.status.is_active()
            })
            .map(|record| record.snapshot.task_id.clone())
    }

    fn take_next_ready_task(&mut self) -> Option<ReadyTask> {
        let idx = self.pending.iter().position(|task_id| {
            self.tasks
                .get(task_id)
                .map(|r| self.resources_available(&r.snapshot.resources))
                .unwrap_or(false)
        })?;
        let task_id = self.pending.remove(idx)?;

        let resources = self.tasks.get(&task_id)?.snapshot.resources.clone();
        self.allocate_resources(&resources, &task_id);

        let record = self.tasks.get_mut(&task_id)?;
        let runner = record.runner.take()?;
        record.snapshot.status = DeploymentTaskStatus::Running;
        record.snapshot.started_at_ms = Some(now_ms());
        record.snapshot.message = Some("正在执行".to_string());
        Some(ReadyTask {
            task_id,
            runner,
            cancel: record.cancel.clone(),
            snapshot: record.snapshot.clone(),
        })
    }

    fn resources_available(&self, resources: &[DeploymentTaskResource]) -> bool {
        for resource in resources {
            match resource {
                DeploymentTaskResource::GlobalDownloadSlot => {
                    if self.download_slots_used >= self.max_download_slots {
                        return false;
                    }
                }
                other => {
                    if self.resource_owners.contains_key(&resource_key(other)) {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn allocate_resources(&mut self, resources: &[DeploymentTaskResource], task_id: &str) {
        for resource in resources {
            match resource {
                DeploymentTaskResource::GlobalDownloadSlot => {
                    self.download_slots_used += 1;
                }
                other => {
                    self.resource_owners
                        .insert(resource_key(other), task_id.to_string());
                }
            }
        }
    }

    fn release_resources(&mut self, resources: &[DeploymentTaskResource], task_id: &str) {
        for resource in resources {
            match resource {
                DeploymentTaskResource::GlobalDownloadSlot => {
                    self.download_slots_used = self.download_slots_used.saturating_sub(1);
                }
                other => {
                    let key = resource_key(other);
                    if self.resource_owners.get(&key).map(String::as_str) == Some(task_id) {
                        self.resource_owners.remove(&key);
                    }
                }
            }
        }
    }
}

fn resource_key(resource: &DeploymentTaskResource) -> String {
    match resource {
        DeploymentTaskResource::PackageManager { host_id } => {
            format!("pkg:{host_id}")
        }
        DeploymentTaskResource::InstallTarget { host_id, target } => {
            format!("install:{host_id}:{target}")
        }
        DeploymentTaskResource::DockerCapability { host_id } => {
            format!("docker-cap:{host_id}")
        }
        DeploymentTaskResource::DockerDaemon { host_id } => {
            format!("docker-daemon:{host_id}")
        }
        DeploymentTaskResource::DockerImage { host_id, flavor } => {
            format!("docker-image:{host_id}:{}", flavor.as_str())
        }
        DeploymentTaskResource::GlobalDownloadSlot => "download-slot".to_string(),
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_domain::DockerFlavor;

    fn request(task_id: &str, resources: Vec<DeploymentTaskResource>) -> DeploymentTaskRequest {
        DeploymentTaskRequest {
            task_id: task_id.to_string(),
            kind: DeploymentTaskKind::DockerInstall,
            host_id: "remote:a".to_string(),
            title: task_id.to_string(),
            resources,
            dedupe_key: Some(task_id.to_string()),
            cancellable: true,
            runner: Box::new(|_| Box::pin(async { DeploymentTaskRunResult::ok("ok") })),
        }
    }

    #[test]
    fn scheduler_does_not_head_block_on_unrelated_resources() {
        let mut state = DeploymentTaskState {
            tasks: HashMap::new(),
            order: VecDeque::new(),
            pending: VecDeque::new(),
            resource_owners: HashMap::new(),
            download_slots_used: 0,
            max_download_slots: 2,
        };
        state
            .resource_owners
            .insert("pkg:remote:a".into(), "other".into());

        for req in [
            request(
                "apt",
                vec![DeploymentTaskResource::PackageManager {
                    host_id: "remote:a".into(),
                }],
            ),
            request(
                "napcat",
                vec![DeploymentTaskResource::InstallTarget {
                    host_id: "remote:a".into(),
                    target: "napcat".into(),
                }],
            ),
        ] {
            let id = req.task_id.clone();
            state.order.push_back(id.clone());
            state.pending.push_back(id.clone());
            state.tasks.insert(
                id,
                DeploymentTaskRecord {
                    snapshot: DeploymentTaskSnapshot {
                        task_id: req.task_id,
                        kind: req.kind,
                        status: DeploymentTaskStatus::Queued,
                        host_id: req.host_id,
                        title: req.title,
                        dedupe_key: req.dedupe_key,
                        resources: req.resources,
                        progress_events: vec![],
                        submitted_at_ms: 0,
                        started_at_ms: None,
                        ended_at_ms: None,
                        message: None,
                        error: None,
                        cancellable: true,
                    },
                    runner: Some(req.runner),
                    cancel: CancellationToken::new(),
                },
            );
        }

        let ready = state.take_next_ready_task().expect("napcat can run");
        assert_eq!(ready.task_id, "napcat");
    }

    #[test]
    fn docker_image_resource_keys_include_flavor() {
        let napcat = resource_key(&DeploymentTaskResource::DockerImage {
            host_id: "h".into(),
            flavor: DockerFlavor::NapCat,
        });
        let snowluma = resource_key(&DeploymentTaskResource::DockerImage {
            host_id: "h".into(),
            flavor: DockerFlavor::SnowLuma,
        });
        assert_ne!(napcat, snowluma);
    }
}
