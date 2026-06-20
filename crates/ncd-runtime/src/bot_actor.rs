use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::ids::BotId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/")]
pub enum BotActorState {
    #[default]
    Stopped,
    Starting,
    Running,
    Stopping,
    Crashed,
    Repairing,
}

impl BotActorState {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Starting | Self::Running | Self::Stopping)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/")]
pub struct BotActorSnapshot {
    #[ts(type = "string")]
    pub bot_id: BotId,
    pub state: BotActorState,
    #[ts(type = "number")]
    pub revision: u64,
    #[ts(type = "number")]
    pub token_generation: u64,
    pub pending_restart: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_transition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_error: Option<String>,
}

impl BotActorSnapshot {
    pub fn new(bot_id: impl Into<BotId>) -> Self {
        Self {
            bot_id: bot_id.into(),
            state: BotActorState::Stopped,
            revision: 0,
            token_generation: 0,
            pending_restart: false,
            last_transition: None,
            last_error: None,
        }
    }

    fn advance(&mut self, state: BotActorState, transition: impl Into<String>) {
        self.state = state;
        self.revision = self.revision.saturating_add(1);
        self.last_transition = Some(transition.into());
    }

    fn note_error(&mut self, error: impl Into<String>) {
        self.last_error = Some(error.into());
        self.revision = self.revision.saturating_add(1);
    }

    fn clear_error(&mut self) {
        self.last_error = None;
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum BotActorError {
    #[error("invalid transition from {from:?} via {command}")]
    InvalidTransition {
        from: BotActorState,
        command: &'static str,
    },
    #[error("actor mailbox closed")]
    MailboxClosed,
}

pub struct BotActorHandle {
    inner: Arc<BotActorControl>,
}

impl Clone for BotActorHandle {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

struct BotActorControl {
    bot_id: BotId,
    command_tx: mpsc::Sender<BotActorCommand>,
    snapshot_rx: watch::Receiver<BotActorSnapshot>,
    cancellation_token: Arc<Mutex<CancellationToken>>,
}

enum BotActorCommand {
    RequestStart {
        reply: oneshot::Sender<Result<BotActorSnapshot, BotActorError>>,
    },
    RequestStartTransition {
        reply: oneshot::Sender<Result<(BotActorSnapshot, bool), BotActorError>>,
    },
    ConfirmRunning {
        reply: oneshot::Sender<Result<BotActorSnapshot, BotActorError>>,
    },
    RequestStop {
        reply: oneshot::Sender<Result<BotActorSnapshot, BotActorError>>,
    },
    ConfirmStopped {
        reply: oneshot::Sender<Result<BotActorSnapshot, BotActorError>>,
    },
    RequestRestart {
        reply: oneshot::Sender<Result<BotActorSnapshot, BotActorError>>,
    },
    MarkCrashed {
        reason: String,
        reply: oneshot::Sender<Result<BotActorSnapshot, BotActorError>>,
    },
    EnterRepair {
        reason: String,
        reply: oneshot::Sender<Result<BotActorSnapshot, BotActorError>>,
    },
    ResolveRepair {
        reply: oneshot::Sender<Result<BotActorSnapshot, BotActorError>>,
    },
    Shutdown {
        reply: oneshot::Sender<Result<BotActorSnapshot, BotActorError>>,
    },
}

impl BotActorHandle {
    pub fn spawn(bot_id: impl Into<BotId>) -> Self {
        let bot_id = bot_id.into();
        let initial_snapshot = BotActorSnapshot::new(bot_id.clone());
        let (command_tx, command_rx) = mpsc::channel(32);
        let (snapshot_tx, snapshot_rx) = watch::channel(initial_snapshot.clone());
        let cancellation_token = Arc::new(Mutex::new(CancellationToken::new()));
        let worker_token = Arc::clone(&cancellation_token);
        let worker_bot_id = bot_id.clone();

        tokio::spawn(async move {
            run_actor(worker_bot_id, command_rx, snapshot_tx, worker_token).await;
        });

        Self {
            inner: Arc::new(BotActorControl {
                bot_id,
                command_tx,
                snapshot_rx,
                cancellation_token,
            }),
        }
    }

    pub fn bot_id(&self) -> &BotId {
        &self.inner.bot_id
    }

    pub fn snapshot(&self) -> BotActorSnapshot {
        self.inner.snapshot_rx.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<BotActorSnapshot> {
        self.inner.snapshot_rx.clone()
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        self.inner
            .cancellation_token
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub async fn request_start(&self) -> Result<BotActorSnapshot, BotActorError> {
        self.send_snapshot_command(|reply| BotActorCommand::RequestStart { reply })
            .await
    }

    pub async fn request_start_transition(
        &self,
    ) -> Result<(BotActorSnapshot, bool), BotActorError> {
        self.send_command(|reply| BotActorCommand::RequestStartTransition { reply })
            .await
    }

    pub async fn confirm_running(&self) -> Result<BotActorSnapshot, BotActorError> {
        self.send_snapshot_command(|reply| BotActorCommand::ConfirmRunning { reply })
            .await
    }

    pub async fn request_stop(&self) -> Result<BotActorSnapshot, BotActorError> {
        self.send_snapshot_command(|reply| BotActorCommand::RequestStop { reply })
            .await
    }

    pub async fn confirm_stopped(&self) -> Result<BotActorSnapshot, BotActorError> {
        self.send_snapshot_command(|reply| BotActorCommand::ConfirmStopped { reply })
            .await
    }

    pub async fn request_restart(&self) -> Result<BotActorSnapshot, BotActorError> {
        self.send_snapshot_command(|reply| BotActorCommand::RequestRestart { reply })
            .await
    }

    pub async fn mark_crashed(
        &self,
        reason: impl Into<String>,
    ) -> Result<BotActorSnapshot, BotActorError> {
        let reason = reason.into();
        self.send_snapshot_command(move |reply| BotActorCommand::MarkCrashed { reason, reply })
            .await
    }

    pub async fn enter_repair(
        &self,
        reason: impl Into<String>,
    ) -> Result<BotActorSnapshot, BotActorError> {
        let reason = reason.into();
        self.send_snapshot_command(move |reply| BotActorCommand::EnterRepair { reason, reply })
            .await
    }

    pub async fn resolve_repair(&self) -> Result<BotActorSnapshot, BotActorError> {
        self.send_snapshot_command(|reply| BotActorCommand::ResolveRepair { reply })
            .await
    }

    pub async fn shutdown(&self) -> Result<BotActorSnapshot, BotActorError> {
        self.send_snapshot_command(|reply| BotActorCommand::Shutdown { reply })
            .await
    }

    async fn send_snapshot_command<F>(&self, make: F) -> Result<BotActorSnapshot, BotActorError>
    where
        F: FnOnce(oneshot::Sender<Result<BotActorSnapshot, BotActorError>>) -> BotActorCommand,
    {
        self.send_command(make).await
    }

    async fn send_command<T, F>(&self, make: F) -> Result<T, BotActorError>
    where
        F: FnOnce(oneshot::Sender<Result<T, BotActorError>>) -> BotActorCommand,
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.inner
            .command_tx
            .send(make(reply_tx))
            .await
            .map_err(|_| BotActorError::MailboxClosed)?;
        reply_rx.await.map_err(|_| BotActorError::MailboxClosed)?
    }
}

fn log_actor_result(
    bot_id: &BotId,
    result: &Result<bool, BotActorError>,
    command: &'static str,
) {
    match result {
        Ok(true) => {
            // 状态已推进，last_transition 在 snapshot 里；此处只记命令名避免与 manager 重复刷屏。
            info!(
                target: "ncd_runtime::bot_actor",
                bot_id = %bot_id,
                command,
                "Bot 状态已推进"
            );
        }
        Ok(false) => {}
        Err(BotActorError::InvalidTransition { from, command: cmd }) => {
            warn!(
                target: "ncd_runtime::bot_actor",
                bot_id = %bot_id,
                from = ?from,
                command = cmd,
                "Bot 状态转换被拒绝"
            );
        }
        Err(BotActorError::MailboxClosed) => {}
    }
}

async fn run_actor(
    bot_id: BotId,
    mut command_rx: mpsc::Receiver<BotActorCommand>,
    snapshot_tx: watch::Sender<BotActorSnapshot>,
    cancellation_token: Arc<Mutex<CancellationToken>>,
) {
    let mut snapshot = BotActorSnapshot::new(bot_id.clone());

    while let Some(command) = command_rx.recv().await {
        match command {
            BotActorCommand::RequestStart { reply } => {
                let result = request_start(&mut snapshot, &cancellation_token);
                log_actor_result(&bot_id, &result, "start");
                if result.is_ok() {
                    let _ = snapshot_tx.send(snapshot.clone());
                }
                let _ = reply.send(result.map(|_| snapshot.clone()));
            }
            BotActorCommand::RequestStartTransition { reply } => {
                let result = request_start(&mut snapshot, &cancellation_token);
                log_actor_result(&bot_id, &result, "start");
                if result.is_ok() {
                    let _ = snapshot_tx.send(snapshot.clone());
                }
                let _ = reply.send(result.map(|advanced| (snapshot.clone(), advanced)));
            }
            BotActorCommand::ConfirmRunning { reply } => {
                let result = confirm_running(&mut snapshot);
                log_actor_result(&bot_id, &result, "confirm_running");
                if result.is_ok() {
                    let _ = snapshot_tx.send(snapshot.clone());
                }
                let _ = reply.send(result.map(|_| snapshot.clone()));
            }
            BotActorCommand::RequestStop { reply } => {
                let result = request_stop(&mut snapshot, &cancellation_token);
                log_actor_result(&bot_id, &result, "stop");
                if result.is_ok() {
                    let _ = snapshot_tx.send(snapshot.clone());
                }
                let _ = reply.send(result.map(|_| snapshot.clone()));
            }
            BotActorCommand::ConfirmStopped { reply } => {
                let result = confirm_stopped(&mut snapshot, &cancellation_token);
                log_actor_result(&bot_id, &result, "confirm_stopped");
                if result.is_ok() {
                    let _ = snapshot_tx.send(snapshot.clone());
                }
                let _ = reply.send(result.map(|_| snapshot.clone()));
            }
            BotActorCommand::RequestRestart { reply } => {
                let result = request_restart(&mut snapshot, &cancellation_token);
                log_actor_result(&bot_id, &result, "restart");
                if result.is_ok() {
                    let _ = snapshot_tx.send(snapshot.clone());
                }
                let _ = reply.send(result.map(|_| snapshot.clone()));
            }
            BotActorCommand::MarkCrashed { reason, reply } => {
                let reason_for_log = reason.clone();
                let result = mark_crashed(&mut snapshot, &cancellation_token, reason);
                log_actor_result(&bot_id, &result, "mark_crashed");
                if let Ok(true) = &result {
                    warn!(
                        target: "ncd_runtime::bot_actor",
                        bot_id = %bot_id,
                        reason = %reason_for_log,
                        "Bot 运行异常已标记为崩溃"
                    );
                }
                if result.is_ok() {
                    let _ = snapshot_tx.send(snapshot.clone());
                }
                let _ = reply.send(result.map(|_| snapshot.clone()));
            }
            BotActorCommand::EnterRepair { reason, reply } => {
                let reason_for_log = reason.clone();
                let result = enter_repair(&mut snapshot, &cancellation_token, reason);
                log_actor_result(&bot_id, &result, "enter_repair");
                if let Ok(true) = &result {
                    warn!(
                        target: "ncd_runtime::bot_actor",
                        bot_id = %bot_id,
                        reason = %reason_for_log,
                        "Bot 进入修复状态"
                    );
                }
                if result.is_ok() {
                    let _ = snapshot_tx.send(snapshot.clone());
                }
                let _ = reply.send(result.map(|_| snapshot.clone()));
            }
            BotActorCommand::ResolveRepair { reply } => {
                let result = resolve_repair(&mut snapshot);
                log_actor_result(&bot_id, &result, "resolve_repair");
                if result.is_ok() {
                    let _ = snapshot_tx.send(snapshot.clone());
                }
                let _ = reply.send(result.map(|_| snapshot.clone()));
            }
            BotActorCommand::Shutdown { reply } => {
                cancel_current_token(&cancellation_token);
                snapshot.pending_restart = false;
                snapshot.clear_error();
                snapshot.advance(BotActorState::Stopped, "shutdown");
                info!(
                    target: "ncd_runtime::bot_actor",
                    bot_id = %bot_id,
                    "Bot Actor 已关闭"
                );
                let _ = snapshot_tx.send(snapshot.clone());
                let _ = reply.send(Ok(snapshot.clone()));
                break;
            }
        }
    }
}

fn request_start(
    snapshot: &mut BotActorSnapshot,
    cancellation_token: &Arc<Mutex<CancellationToken>>,
) -> Result<bool, BotActorError> {
    match snapshot.state {
        BotActorState::Stopped | BotActorState::Crashed => {
            reset_token(snapshot, cancellation_token);
            snapshot.pending_restart = false;
            snapshot.clear_error();
            snapshot.advance(BotActorState::Starting, "start_requested");
            Ok(true)
        }
        BotActorState::Starting | BotActorState::Running => Ok(false),
        BotActorState::Stopping => Err(BotActorError::InvalidTransition {
            from: snapshot.state,
            command: "start",
        }),
        BotActorState::Repairing => Err(BotActorError::InvalidTransition {
            from: snapshot.state,
            command: "start",
        }),
    }
}

fn confirm_running(snapshot: &mut BotActorSnapshot) -> Result<bool, BotActorError> {
    match snapshot.state {
        BotActorState::Starting => {
            snapshot.clear_error();
            snapshot.advance(BotActorState::Running, "start_completed");
            Ok(true)
        }
        BotActorState::Running => Ok(false),
        _ => Err(BotActorError::InvalidTransition {
            from: snapshot.state,
            command: "confirm_running",
        }),
    }
}

fn request_stop(
    snapshot: &mut BotActorSnapshot,
    cancellation_token: &Arc<Mutex<CancellationToken>>,
) -> Result<bool, BotActorError> {
    match snapshot.state {
        BotActorState::Running | BotActorState::Starting => {
            cancel_current_token(cancellation_token);
            snapshot.pending_restart = false;
            snapshot.advance(BotActorState::Stopping, "stop_requested");
            Ok(true)
        }
        BotActorState::Stopped | BotActorState::Stopping => Ok(false),
        BotActorState::Crashed => Ok(false),
        BotActorState::Repairing => Ok(false),
    }
}

fn confirm_stopped(
    snapshot: &mut BotActorSnapshot,
    cancellation_token: &Arc<Mutex<CancellationToken>>,
) -> Result<bool, BotActorError> {
    match snapshot.state {
        BotActorState::Stopping => {
            if snapshot.pending_restart {
                reset_token(snapshot, cancellation_token);
                snapshot.pending_restart = false;
                snapshot.clear_error();
                snapshot.advance(BotActorState::Starting, "restart_queued");
            } else {
                snapshot.clear_error();
                snapshot.advance(BotActorState::Stopped, "stop_completed");
            }
            Ok(true)
        }
        BotActorState::Stopped => Ok(false),
        _ => Err(BotActorError::InvalidTransition {
            from: snapshot.state,
            command: "confirm_stopped",
        }),
    }
}

fn request_restart(
    snapshot: &mut BotActorSnapshot,
    cancellation_token: &Arc<Mutex<CancellationToken>>,
) -> Result<bool, BotActorError> {
    match snapshot.state {
        BotActorState::Running | BotActorState::Starting => {
            cancel_current_token(cancellation_token);
            snapshot.pending_restart = true;
            snapshot.advance(BotActorState::Stopping, "restart_requested");
            Ok(true)
        }
        BotActorState::Stopped | BotActorState::Crashed => {
            reset_token(snapshot, cancellation_token);
            snapshot.pending_restart = false;
            snapshot.clear_error();
            snapshot.advance(BotActorState::Starting, "restart_requested");
            Ok(true)
        }
        BotActorState::Stopping => {
            snapshot.pending_restart = true;
            Ok(false)
        }
        BotActorState::Repairing => Err(BotActorError::InvalidTransition {
            from: snapshot.state,
            command: "restart",
        }),
    }
}

fn mark_crashed(
    snapshot: &mut BotActorSnapshot,
    cancellation_token: &Arc<Mutex<CancellationToken>>,
    reason: String,
) -> Result<bool, BotActorError> {
    if matches!(snapshot.state, BotActorState::Stopped) {
        return Err(BotActorError::InvalidTransition {
            from: snapshot.state,
            command: "mark_crashed",
        });
    }

    cancel_current_token(cancellation_token);
    snapshot.pending_restart = false;
    snapshot.note_error(reason);
    snapshot.advance(BotActorState::Crashed, "runtime_crashed");
    Ok(true)
}

fn enter_repair(
    snapshot: &mut BotActorSnapshot,
    cancellation_token: &Arc<Mutex<CancellationToken>>,
    reason: String,
) -> Result<bool, BotActorError> {
    match snapshot.state {
        BotActorState::Running => Err(BotActorError::InvalidTransition {
            from: snapshot.state,
            command: "enter_repair",
        }),
        BotActorState::Repairing => Ok(false),
        _ => {
            cancel_current_token(cancellation_token);
            snapshot.pending_restart = false;
            snapshot.note_error(reason);
            snapshot.advance(BotActorState::Repairing, "repair_required");
            Ok(true)
        }
    }
}

fn resolve_repair(snapshot: &mut BotActorSnapshot) -> Result<bool, BotActorError> {
    match snapshot.state {
        BotActorState::Repairing => {
            snapshot.clear_error();
            snapshot.advance(BotActorState::Stopped, "repair_resolved");
            Ok(true)
        }
        BotActorState::Stopped => Ok(false),
        _ => Err(BotActorError::InvalidTransition {
            from: snapshot.state,
            command: "resolve_repair",
        }),
    }
}

fn reset_token(
    snapshot: &mut BotActorSnapshot,
    cancellation_token: &Arc<Mutex<CancellationToken>>,
) {
    let mut guard = cancellation_token.lock().unwrap_or_else(|e| e.into_inner());
    *guard = CancellationToken::new();
    snapshot.token_generation = snapshot.token_generation.saturating_add(1);
}

fn cancel_current_token(cancellation_token: &Arc<Mutex<CancellationToken>>) {
    cancellation_token
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .cancel();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bot_actor_handles_start_stop_cycle() {
        let actor = BotActorHandle::spawn("10001");

        let starting = actor.request_start().await.unwrap();
        assert_eq!(starting.state, BotActorState::Starting);
        assert_eq!(starting.token_generation, 1);

        let running = actor.confirm_running().await.unwrap();
        assert_eq!(running.state, BotActorState::Running);

        let stopping = actor.request_stop().await.unwrap();
        assert_eq!(stopping.state, BotActorState::Stopping);

        let stopped = actor.confirm_stopped().await.unwrap();
        assert_eq!(stopped.state, BotActorState::Stopped);

        let snapshot = actor.snapshot();
        assert_eq!(snapshot.state, BotActorState::Stopped);
        let _ = actor.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn bot_actor_queues_restart_after_stop() {
        let actor = BotActorHandle::spawn("10002");

        actor.request_start().await.unwrap();
        actor.confirm_running().await.unwrap();
        let restarting = actor.request_restart().await.unwrap();
        assert_eq!(restarting.state, BotActorState::Stopping);
        assert!(restarting.pending_restart);

        let queued = actor.confirm_stopped().await.unwrap();
        assert_eq!(queued.state, BotActorState::Starting);
        assert_eq!(queued.token_generation, 2);
        assert!(!queued.pending_restart);
        let _ = actor.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn bot_actor_enters_repair_mode_after_crash() {
        let actor = BotActorHandle::spawn("10003");

        actor.request_start().await.unwrap();
        actor.confirm_running().await.unwrap();
        let crashed = actor
            .mark_crashed("process exited unexpectedly")
            .await
            .unwrap();
        assert_eq!(crashed.state, BotActorState::Crashed);
        assert_eq!(
            crashed.last_error.as_deref(),
            Some("process exited unexpectedly")
        );

        let repairing = actor.enter_repair("manual repair required").await.unwrap();
        assert_eq!(repairing.state, BotActorState::Repairing);

        let stopped = actor.resolve_repair().await.unwrap();
        assert_eq!(stopped.state, BotActorState::Stopped);
        let _ = actor.shutdown().await.unwrap();
    }
}
