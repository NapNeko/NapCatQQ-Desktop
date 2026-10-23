// 将组件内部进度聚合为部署步骤，并保留嵌套步骤的日志。

use super::{ProgressKind, ProgressLogLevel};

struct ActiveStep {
    step: u32,
    message: String,
}

struct ProgressFrame {
    total_steps: u32,
    started: bool,
    active: Option<ActiveStep>,
}

impl ProgressFrame {
    fn new(total_steps: u32, started: bool) -> Self {
        Self {
            total_steps: total_steps.max(1),
            started,
            active: None,
        }
    }

    fn percent(&self, step: u32, percent: u8) -> u8 {
        let completed = step.saturating_sub(1).min(self.total_steps - 1);
        ((u64::from(completed) * 100 + u64::from(percent.min(100))) / u64::from(self.total_steps))
            as u8
    }
}

pub(super) struct StepProgressScope {
    target_step: u32,
    frames: Vec<ProgressFrame>,
    closed: bool,
}

impl StepProgressScope {
    pub(super) fn new(target_step: u32) -> Self {
        Self {
            target_step,
            frames: vec![ProgressFrame::new(1, false)],
            closed: false,
        }
    }

    fn percent(&self, mut step: u32, mut percent: u8) -> u8 {
        for frame in self.frames.iter().rev() {
            if step == 0 {
                step = frame.active.as_ref().map_or(1, |active| active.step);
            }
            percent = frame.percent(step, percent);
            step = 0;
        }
        percent
    }

    fn progress(&self, step: u32, percent: u8, message: String) -> ProgressKind {
        ProgressKind::StepProgress {
            step: self.target_step,
            percent: self.percent(step, percent),
            message,
            speed_bps: None,
            downloaded_bytes: None,
            total_bytes: None,
            download_stage: None,
            docker_layers: None,
        }
    }

    fn end_active(&mut self, ok: bool) -> Vec<ProgressKind> {
        let Some(active) = self.frames.last_mut().and_then(|frame| frame.active.take()) else {
            return Vec::new();
        };
        vec![
            ProgressKind::Log {
                level: if ok {
                    ProgressLogLevel::Info
                } else {
                    ProgressLogLevel::Error
                },
                message: format!("{}：{}", if ok { "完成" } else { "失败" }, active.message),
            },
            self.progress(active.step, if ok { 100 } else { 0 }, active.message),
        ]
    }

    pub(super) fn finish(&mut self, ok: bool) -> Vec<ProgressKind> {
        if self.closed {
            return Vec::new();
        }
        self.closed = true;
        let mut events = Vec::new();
        while !self.frames.is_empty() {
            events.extend(self.end_active(ok));
            self.frames.pop();
        }
        events
    }

    pub(super) fn map(&mut self, kind: ProgressKind) -> Vec<ProgressKind> {
        if self.closed {
            // 错误日志可能晚于组件返回；只拦截进度，保留诊断信息。
            return match kind {
                ProgressKind::Log { .. } => vec![kind],
                _ => Vec::new(),
            };
        }
        match kind {
            ProgressKind::Started { total_steps } => {
                if let Some(frame) = self.frames.last_mut() {
                    if frame.started && frame.active.is_some() {
                        self.frames.push(ProgressFrame::new(total_steps, true));
                    } else {
                        frame.total_steps = total_steps.max(1);
                        frame.started = true;
                    }
                }
                Vec::new()
            }
            ProgressKind::StepBegin { step, message } => {
                if let Some(frame) = self.frames.last_mut() {
                    frame.active = Some(ActiveStep {
                        step,
                        message: message.clone(),
                    });
                }
                vec![
                    ProgressKind::Log {
                        level: ProgressLogLevel::Info,
                        message: format!("开始：{message}"),
                    },
                    self.progress(step, 0, message),
                ]
            }
            ProgressKind::StepProgress {
                step,
                percent,
                message,
                speed_bps,
                downloaded_bytes,
                total_bytes,
                download_stage,
                docker_layers,
            } => vec![ProgressKind::StepProgress {
                step: self.target_step,
                percent: self.percent(step, percent),
                message,
                speed_bps,
                downloaded_bytes,
                total_bytes,
                download_stage,
                docker_layers,
            }],
            ProgressKind::StepEnd { step, ok } => {
                if self
                    .frames
                    .last()
                    .and_then(|frame| frame.active.as_ref())
                    .is_some_and(|active| active.step == step)
                {
                    self.end_active(ok)
                } else {
                    Vec::new()
                }
            }
            ProgressKind::Finished { ok } if self.frames.len() > 1 => {
                let events = self.end_active(ok);
                self.frames.pop();
                events
            }
            // 最外层 Finished 由组件返回值决定，不能提前结束整个部署。
            ProgressKind::Finished { .. } => Vec::new(),
            ProgressKind::Log { .. } => vec![kind],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{ActionCtx, ProgressEvent};
    use tokio::sync::mpsc;

    fn drain_progress(rx: &mut mpsc::Receiver<ProgressEvent>) -> Vec<ProgressKind> {
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event.kind);
        }
        events
    }

    fn download_progress(step: u32, percent: u8) -> ProgressKind {
        ProgressKind::StepProgress {
            step,
            percent,
            message: "download".into(),
            speed_bps: Some(1024),
            downloaded_bytes: Some(4096),
            total_bytes: Some(8192),
            download_stage: Some("streaming".into()),
            docker_layers: Some(vec![ncd_domain::DockerPullLayerSnapshot {
                id: "layer".into(),
                phase: "下载中".into(),
                detail: Some("4KB/8KB".into()),
                done: false,
            }]),
        }
    }

    async fn begin(ctx: &ActionCtx, step: u32, message: &str) {
        ctx.emit(ProgressKind::StepBegin {
            step,
            message: message.into(),
        })
        .await;
    }

    fn failure_labels(events: &[ProgressKind]) -> Vec<&str> {
        events
            .iter()
            .filter_map(|event| match event {
                ProgressKind::Log {
                    level: ProgressLogLevel::Error,
                    message,
                } => Some(message.as_str()),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn scoped_step_preserves_download_and_layer_metadata() {
        let (ctx, mut rx) = ActionCtx::new();
        let scoped = ctx.scoped_step(7);
        scoped.emit(ProgressKind::Started { total_steps: 4 }).await;
        scoped.emit(download_progress(3, 50)).await;
        assert_eq!(drain_progress(&mut rx), vec![download_progress(7, 62)]);
    }

    #[tokio::test]
    async fn explicit_failure_is_not_duplicated_and_late_errors_are_retained() {
        let (ctx, mut rx) = ActionCtx::new();
        let scoped = ctx.scoped_step(1);
        scoped.emit(ProgressKind::Started { total_steps: 4 }).await;
        begin(&scoped, 4, "extract QQ").await;
        scoped
            .emit(ProgressKind::StepEnd { step: 4, ok: false })
            .await;
        scoped.finish_step_progress(false).await;
        scoped.finish_step_progress(false).await;
        scoped.emit(download_progress(4, 100)).await;
        scoped.emit(ProgressKind::Finished { ok: true }).await;
        scoped.log(ProgressLogLevel::Error, "late diagnostic").await;

        let events = drain_progress(&mut rx);
        assert_eq!(
            failure_labels(&events),
            ["失败：extract QQ", "late diagnostic"]
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, ProgressKind::StepProgress { .. }))
                .count(),
            2
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, ProgressKind::Finished { .. }))
        );
    }

    #[tokio::test]
    async fn nested_child_lifecycle_keeps_parent_coordinates_and_active_step() {
        let (ctx, mut rx) = ActionCtx::new();
        let scoped = ctx.scoped_step(2);
        scoped.emit(ProgressKind::Started { total_steps: 4 }).await;
        begin(&scoped, 3, "download QQ package").await;
        let nested = scoped.child();
        nested.emit(ProgressKind::Started { total_steps: 2 }).await;
        begin(&nested, 2, "transfer package").await;
        nested.emit(download_progress(2, 50)).await;
        nested.emit(ProgressKind::Finished { ok: false }).await;
        scoped.finish_step_progress(false).await;
        nested.emit(download_progress(2, 100)).await;

        let events = drain_progress(&mut rx);
        let progress: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                ProgressKind::StepProgress { step, percent, .. } => Some((*step, *percent)),
                _ => None,
            })
            .collect();
        assert_eq!(progress, [(2, 50), (2, 62), (2, 68), (2, 62), (2, 50)]);
        assert_eq!(
            failure_labels(&events),
            ["失败：transfer package", "失败：download QQ package"]
        );
        assert!(!events.iter().any(|event| matches!(
            event,
            ProgressKind::Started { .. }
                | ProgressKind::Finished { .. }
                | ProgressKind::StepBegin { .. }
                | ProgressKind::StepEnd { .. }
        )));
    }

    #[tokio::test]
    async fn unfinished_nested_action_is_closed_from_inside_out_on_error() {
        let (ctx, mut rx) = ActionCtx::new();
        let scoped = ctx.scoped_step(1);
        scoped.emit(ProgressKind::Started { total_steps: 4 }).await;
        begin(&scoped, 4, "extract QQ").await;
        let nested = scoped.child();
        nested.emit(ProgressKind::Started { total_steps: 2 }).await;
        begin(&nested, 2, "unpack payload").await;
        scoped.finish_step_progress(false).await;
        let events = drain_progress(&mut rx);
        assert_eq!(
            failure_labels(&events),
            ["失败：unpack payload", "失败：extract QQ"]
        );
    }

    #[tokio::test]
    async fn child_without_its_own_started_keeps_parent_progress_coordinates() {
        let (ctx, mut rx) = ActionCtx::new();
        let scoped = ctx.scoped_step(2);
        scoped.emit(ProgressKind::Started { total_steps: 4 }).await;
        scoped.child().emit(download_progress(3, 20)).await;
        assert_eq!(drain_progress(&mut rx), vec![download_progress(2, 55)]);
    }

    #[tokio::test]
    async fn closing_scope_under_backpressure_preserves_terminal_order() {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let (ctx, mut rx) = ActionCtx::with_capacity(1);
            let scoped = ctx.scoped_step(1);
            let emitter_ctx = scoped.clone();
            let emitter = tokio::spawn(async move {
                begin(&emitter_ctx, 1, "extract QQ").await;
            });
            let first = rx.recv().await.unwrap();
            let closer = tokio::spawn(async move {
                scoped.finish_step_progress(false).await;
                ctx.emit(ProgressKind::Finished { ok: false }).await;
            });
            let mut events = vec![first.kind];
            for _ in 0..4 {
                events.push(rx.recv().await.unwrap().kind);
            }
            emitter.await.unwrap();
            closer.await.unwrap();
            assert_eq!(failure_labels(&events), ["失败：extract QQ"]);
            assert_eq!(events.last(), Some(&ProgressKind::Finished { ok: false }));
        })
        .await
        .expect("progress receiver must be able to drain while the scope is closing");
    }
}
