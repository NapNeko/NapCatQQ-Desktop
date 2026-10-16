//! 会话事件先建立基线，共享 daemon 事件按远端序号去重。
use std::collections::{HashMap, HashSet, VecDeque};

use ncd_domain::DomainEvent;
use ncd_domain::windows_bot::{WindowsBotEventBatch, WindowsBotSpec};
use ncd_domain::windows_remote::{WindowsAgentAction, WindowsAgentResult};
use ncd_traits::events::EventBus;
use ncd_traits::runtime_backend::BotBackendError;

use super::RemoteWindowsBotBackend;

#[derive(Default)]
pub(super) struct EventCursor {
    generation: Option<String>,
    after: u64,
}

#[cfg(test)]
mod tests {
    use super::DaemonEvents;

    #[test]
    fn daemon_sequences_accept_unseen_older_values_and_scope_worker_restarts() {
        let mut seen = DaemonEvents::default();
        assert!(seen.accept("one", "worker", 20));
        assert!(seen.accept("one", "worker", 10));
        assert!(!seen.accept("one", "worker", 20));
        assert!(seen.accept("two", "worker", 20));
        assert!(seen.accept("one", "replacement", 20));
        assert!(!seen.accept("one", "worker", 10));
    }
}

#[derive(Default)]
struct SeenSequences {
    values: HashSet<u64>,
    order: VecDeque<u64>,
}

#[derive(Default)]
pub(super) struct DaemonEvents {
    workers: HashMap<(String, String), SeenSequences>,
    order: VecDeque<(String, String)>,
}

impl DaemonEvents {
    fn accept(&mut self, server: &str, generation: &str, sequence: u64) -> bool {
        let key = (server.to_string(), generation.to_string());
        if !self.workers.contains_key(&key) {
            self.order.push_back(key.clone());
            self.workers.insert(key.clone(), SeenSequences::default());
            while self.order.len() > 32 {
                if let Some(previous) = self.order.pop_front() {
                    self.workers.remove(&previous);
                }
            }
        }
        let Some(seen) = self.workers.get_mut(&key) else {
            return false;
        };
        if !seen.values.insert(sequence) {
            return false;
        }
        seen.order.push_back(sequence);
        while seen.order.len() > 4096 {
            if let Some(previous) = seen.order.pop_front() {
                seen.values.remove(&previous);
            }
        }
        true
    }
}

impl RemoteWindowsBotBackend {
    async fn event_batch(
        &self,
        after: u64,
        generation: Option<String>,
    ) -> Result<WindowsBotEventBatch, BotBackendError> {
        match self
            .request(WindowsAgentAction::BotEvents {
                bot_id: self.id.to_string(),
                after,
                generation,
            })
            .await?
        {
            WindowsAgentResult::BotEvents(batch) => Ok(batch),
            _ => Err(BotBackendError::Io(
                "Windows 托管端返回了意外的事件响应".into(),
            )),
        }
    }

    async fn baseline_cursor(
        &self,
        cursor: &mut EventCursor,
        check_worker: bool,
    ) -> Result<(), BotBackendError> {
        if cursor.generation.is_some() && !check_worker {
            return Ok(());
        }
        let baseline = self.event_batch(u64::MAX, None).await?;
        if !baseline.events.is_empty() {
            return Err(BotBackendError::InvalidConfig(
                "请更新目标主机的 Windows 托管端后重试".into(),
            ));
        }
        // 同一 worker 的未读事件可能属于本次 start，不能再次跳到队尾。
        if cursor.generation.as_ref() != Some(&baseline.generation) {
            cursor.after = baseline.next;
            cursor.generation = Some(baseline.generation);
        }
        Ok(())
    }

    pub(super) async fn ensure_event_baseline(
        &self,
        check_worker: bool,
    ) -> Result<(), BotBackendError> {
        self.baseline_cursor(&mut *self.cursor.lock().await, check_worker)
            .await
    }

    pub(super) async fn start_with_event_baseline(
        &self,
        spec: WindowsBotSpec,
    ) -> Result<WindowsAgentResult, BotBackendError> {
        let mut cursor = self.cursor.lock().await;
        self.baseline_cursor(&mut cursor, true).await?;
        self.request(WindowsAgentAction::BotStart(spec)).await
    }

    pub(super) async fn poll_events(&self) -> Result<(), BotBackendError> {
        let mut cursor = self.cursor.lock().await;
        self.baseline_cursor(&mut cursor, false).await?;
        let batch = self
            .event_batch(cursor.after, cursor.generation.clone())
            .await?;
        if cursor.generation.as_ref() != Some(&batch.generation) {
            self.baseline_cursor(&mut cursor, true).await?;
            return Ok(());
        }
        if batch.event_sequences.len() != batch.events.len() {
            return Err(BotBackendError::InvalidConfig(
                "Windows 托管端缺少事件序号，请更新后重试".into(),
            ));
        }
        for (event, sequence) in batch.events.into_iter().zip(batch.event_sequences) {
            self.check_open()?;
            if matches!(event, DomainEvent::NapCatWebuiAvailable { .. }) {
                continue;
            }
            if matches!(event, DomainEvent::BotProcessExited { .. }) {
                let current = self
                    .request(WindowsAgentAction::BotStatus {
                        bot_id: self.id.to_string(),
                    })
                    .await;
                if !matches!(
                    current,
                    Ok(WindowsAgentResult::BotStatus(status))
                        if matches!(status.state, ncd_domain::BotActorState::Stopped | ncd_domain::BotActorState::Crashed)
                ) {
                    continue;
                }
            }
            if matches!(event, DomainEvent::SnowLumaDaemonStateChanged { .. })
                && !self.daemon_events.lock().await.accept(
                    &self.server,
                    &batch.generation,
                    sequence,
                )
            {
                continue;
            }
            self.check_open()?;
            match event {
                DomainEvent::SnowLumaDaemonStateChanged {
                    state,
                    ref_count,
                    reason,
                    ..
                } => self.bus.publish(DomainEvent::snowluma_daemon_state_changed(
                    state,
                    ref_count,
                    reason,
                    Some(self.server.clone()),
                )),
                DomainEvent::SnowLumaDaemonLog { line } => self
                    .bus
                    .publish(DomainEvent::bot_log(self.id.clone(), line)),
                event => self.bus.publish(event),
            }
        }
        cursor.after = batch.next;
        Ok(())
    }
}
