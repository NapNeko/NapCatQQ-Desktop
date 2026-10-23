//! DeployPlan::run 编排执行 + 失败回滚 + 进度上报
//!
//! 把 plan 跑起来,emit 进度,失败时按需回滚已 install 的 step

use std::time::Instant;

use tracing::{error, info, warn};

use ncd_component::{ActionCtx, ProgressKind, ProgressLogLevel};
use ncd_host::Host;

use crate::error::DeployError;
use crate::plan::{DeployPlan, DeployStep, StepKind};
use crate::result::{DeployOutcome, StepOutcome};

impl DeployPlan {
    /// 执行 plan
    ///
    /// ctx:进度上报通道(子任务用 ctx.child() 派生取消子节点)
    /// 返回 [DeployOutcome],含每步状态 + 总耗时
    pub async fn run(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<DeployOutcome, DeployError> {
        self.validate()?;

        info!(
            target: "ncd_deploy::runner",
            steps = self.steps.len(),
            "deploy plan run started"
        );

        let total_start = Instant::now();
        let total_steps = self.steps.len() as u32;
        ctx.emit(ProgressKind::Started { total_steps }).await;

        let mut outcomes: Vec<StepOutcome> = Vec::with_capacity(self.steps.len());
        let mut completed_for_rollback: Vec<(u32, &DeployStep)> = Vec::new();

        for (idx, step) in self.steps.iter().enumerate() {
            let step_idx = (idx + 1) as u32;

            if ctx.is_cancelled() {
                outcomes.push(StepOutcome::cancelled(
                    &step.name,
                    step.kind,
                    elapsed_ms(total_start),
                ));
                ctx.emit(ProgressKind::Finished { ok: false }).await;
                return Ok(DeployOutcome::new(outcomes, elapsed_ms(total_start)));
            }

            ctx.emit(ProgressKind::StepBegin {
                step: step_idx,
                message: format!("{} ({})", step.name, step.kind.as_str()),
            })
            .await;

            let step_start = Instant::now();
            let mut child_ctx = ctx.scoped_step(step_idx);
            let result = run_single_step(step, host, &mut child_ctx).await;
            let result = if child_ctx.is_cancelled() {
                Err(ncd_component::ActionError::Cancelled)
            } else {
                result
            };
            child_ctx.finish_step_progress(result.is_ok()).await;
            let dur = elapsed_ms(step_start);

            match result {
                Ok(skipped) => {
                    outcomes.push(StepOutcome::ok(&step.name, step.kind, dur, skipped));
                    completed_for_rollback.push((step_idx, step));
                    ctx.emit(ProgressKind::StepEnd {
                        step: step_idx,
                        ok: true,
                    })
                    .await;
                }
                Err(ncd_component::ActionError::Cancelled) => {
                    outcomes.push(StepOutcome::cancelled(&step.name, step.kind, dur));
                    ctx.emit(ProgressKind::StepEnd {
                        step: step_idx,
                        ok: false,
                    })
                    .await;
                    ctx.emit(ProgressKind::Finished { ok: false }).await;
                    return Ok(DeployOutcome::new(outcomes, elapsed_ms(total_start)));
                }
                Err(e) => {
                    let err_str = format!("{e}");
                    error!(
                        target: "ncd_deploy::runner",
                        step = %step.name,
                        kind = step.kind.as_str(),
                        err = %err_str,
                        "部署步骤失败"
                    );
                    outcomes.push(StepOutcome::failed(&step.name, step.kind, dur, &err_str));
                    ctx.emit(ProgressKind::StepEnd {
                        step: step_idx,
                        ok: false,
                    })
                    .await;
                    ctx.log(
                        ProgressLogLevel::Error,
                        format!("step '{}' failed: {err_str}", step.name),
                    )
                    .await;

                    if step.fail_fast {
                        // 触发回滚
                        if let Err(rb_err) = rollback(host, ctx, &completed_for_rollback).await {
                            error!(
                                target: "ncd_deploy::runner",
                                step = %step.name,
                                err = %rb_err,
                                "部署失败后回滚未完全成功"
                            );
                            ctx.emit(ProgressKind::Finished { ok: false }).await;
                            return Err(DeployError::RollbackFailed {
                                step: step.name.clone(),
                                orig: err_str,
                                rollback: format!("{rb_err}"),
                            });
                        }
                        ctx.emit(ProgressKind::Finished { ok: false }).await;
                        return Ok(DeployOutcome::new(outcomes, elapsed_ms(total_start)));
                    } else {
                        // 不中断,继续下一 step
                        continue;
                    }
                }
            }
        }

        let outcome = DeployOutcome::new(outcomes, elapsed_ms(total_start));
        ctx.emit(ProgressKind::Finished { ok: outcome.ok }).await;
        info!(
            target: "ncd_deploy::runner",
            elapsed_ms = outcome.total_duration_ms,
            ok = outcome.ok,
            "deploy plan run finished"
        );
        Ok(outcome)
    }
}

/// 跑单个 step,返回 Ok(true) 表示被 skip,Ok(false) 表示真的跑了
async fn run_single_step(
    step: &DeployStep,
    host: &dyn Host,
    ctx: &mut ActionCtx,
) -> Result<bool, ncd_component::ActionError> {
    match step.kind {
        StepKind::EnsureInstalled => {
            let detected = step.component.detect(host).await?;
            if detected.is_some() {
                ctx.info(format!("step '{}' skipped: already installed", step.name))
                    .await;
                return Ok(true);
            }
            step.component.install(host, ctx).await?;
            Ok(false)
        }
        StepKind::ForceInstall => {
            step.component.install(host, ctx).await?;
            Ok(false)
        }
        StepKind::Update => {
            step.component.update(host, ctx).await?;
            Ok(false)
        }
        StepKind::Uninstall => {
            step.component.uninstall(host, ctx).await?;
            Ok(false)
        }
        StepKind::Verify => {
            let report = step.component.verify(host).await?;
            if !report.ok {
                return Err(ncd_component::ActionError::other(format!(
                    "verify failed for {:?}: {report:?}",
                    step.component.id()
                )));
            }
            Ok(false)
        }
        StepKind::EnsureDependencies => {
            step.component.ensure_dependencies(host, ctx).await?;
            Ok(false)
        }
    }
}

/// 对所有 rollback_on_failure=true 的已完成 step 倒序跑 uninstall
async fn rollback(
    host: &dyn Host,
    ctx: &mut ActionCtx,
    completed: &[(u32, &DeployStep)],
) -> Result<(), DeployError> {
    warn!(
        target: "ncd_deploy::runner",
        steps = completed.len(),
        "部署失败，开始回滚已完成的步骤"
    );
    for (step_idx, step) in completed.iter().rev() {
        if !step.rollback_on_failure {
            continue;
        }
        ctx.info(format!("rollback step '{}'", step.name)).await;
        let mut rollback_ctx = ctx.scoped_step(*step_idx);
        let result = step.component.uninstall(host, &mut rollback_ctx).await;
        rollback_ctx.finish_step_progress(result.is_ok()).await;
        if let Err(e) = result {
            ctx.warn(format!("rollback uninstall '{}' failed: {e}", step.name))
                .await;
            return Err(DeployError::Action(e));
        }
    }
    Ok(())
}

fn elapsed_ms(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use ncd_component::{
        ActionCtx, ActionError, Component, ComponentId, DetectedVersion, LaunchArgs, VerifyReport,
    };
    use ncd_host::{Arch, Host, HostCommand, HostError, HostPath, Locality, Os};
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// 极简 Host stub:所有方法返回 Unsupported,只是给 Component 走流程
    struct StubHost;

    #[async_trait]
    impl Host for StubHost {
        fn os(&self) -> Os {
            Os::Linux
        }
        fn arch(&self) -> Arch {
            Arch::X86_64
        }
        fn locality(&self) -> Locality {
            Locality::Local
        }
        fn id(&self) -> &str {
            "stub"
        }
        fn shell(&self) -> &dyn ncd_host::HostShell {
            &ncd_host::shell::BashShell
        }
        async fn read_file(&self, _: &HostPath) -> Result<bytes::Bytes, HostError> {
            Err(HostError::Unsupported { operation: "stub" })
        }
        async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "stub" })
        }
        async fn list_dir(&self, _: &HostPath) -> Result<Vec<ncd_host::DirEntry>, HostError> {
            Err(HostError::Unsupported { operation: "stub" })
        }
        async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "stub" })
        }
        async fn remove_file(&self, _: &HostPath) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "stub" })
        }
        async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "stub" })
        }
        async fn exists(&self, _: &HostPath) -> Result<bool, HostError> {
            Err(HostError::Unsupported { operation: "stub" })
        }
        async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "stub" })
        }
        async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "stub" })
        }
        async fn extract_archive(
            &self,
            _: &HostPath,
            _: &HostPath,
            _: ncd_host::ArchiveKind,
        ) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "stub" })
        }
        async fn spawn(&self, _: HostCommand) -> Result<Box<dyn ncd_host::HostProcess>, HostError> {
            Err(HostError::Unsupported { operation: "stub" })
        }
        async fn run_to_string(
            &self,
            _: HostCommand,
        ) -> Result<ncd_host::CommandOutput, HostError> {
            Err(HostError::Unsupported { operation: "stub" })
        }
    }

    /// 可控 Component:detect 永远 None / Some,install 可设失败,记录调用次数
    struct CountedComponent {
        id_value: ComponentId,
        already_installed: bool,
        install_should_fail: bool,
        install_calls: Arc<AtomicU32>,
        uninstall_calls: Arc<AtomicU32>,
        progress_events: Vec<ProgressKind>,
        cancel_during_install: bool,
    }

    #[async_trait]
    impl Component for CountedComponent {
        fn id(&self) -> ComponentId {
            self.id_value
        }
        fn supported_targets(&self) -> &'static [(Os, Locality)] {
            &[(Os::Linux, Locality::Local)]
        }
        async fn detect(&self, _: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
            if self.already_installed {
                Ok(Some(DetectedVersion {
                    version: "1.0".into(),
                    source: "stub".into(),
                }))
            } else {
                Ok(None)
            }
        }
        async fn install(&self, _: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
            self.install_calls.fetch_add(1, Ordering::SeqCst);
            for event in &self.progress_events {
                ctx.emit(event.clone()).await;
            }
            if self.cancel_during_install {
                ctx.cancel();
                return Err(ActionError::Cancelled);
            }
            if self.install_should_fail {
                Err(ActionError::other("simulated install failure"))
            } else {
                Ok(())
            }
        }
        async fn uninstall(&self, _: &dyn Host, _: &mut ActionCtx) -> Result<(), ActionError> {
            self.uninstall_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn verify(&self, _: &dyn Host) -> Result<VerifyReport, ActionError> {
            Ok(VerifyReport::ok())
        }
        fn launch_command(&self, _: &dyn Host, _: &LaunchArgs) -> Result<HostCommand, ActionError> {
            Ok(HostCommand::new("echo"))
        }
    }

    fn comp(
        id: ComponentId,
        already_installed: bool,
        install_fails: bool,
    ) -> (Arc<dyn Component>, Arc<AtomicU32>, Arc<AtomicU32>) {
        let install_calls = Arc::new(AtomicU32::new(0));
        let uninstall_calls = Arc::new(AtomicU32::new(0));
        let c = Arc::new(CountedComponent {
            id_value: id,
            already_installed,
            install_should_fail: install_fails,
            install_calls: install_calls.clone(),
            uninstall_calls: uninstall_calls.clone(),
            progress_events: Vec::new(),
            cancel_during_install: false,
        });
        (c, install_calls, uninstall_calls)
    }

    fn qq_progress(fails: bool) -> Vec<ProgressKind> {
        let mut events = vec![ProgressKind::Started { total_steps: 4 }];
        for (idx, message) in [
            "check system dependencies",
            "detect package format",
            "download QQ package",
            "extract QQ",
        ]
        .into_iter()
        .enumerate()
        {
            let step = idx as u32 + 1;
            events.push(ProgressKind::StepBegin {
                step,
                message: message.into(),
            });
            if step == 4 && fails {
                break;
            }
            events.push(ProgressKind::StepEnd { step, ok: true });
        }
        if !fails {
            events.push(ProgressKind::Finished { ok: true });
        }
        events
    }

    fn progress_component(
        id: ComponentId,
        fails: bool,
        cancel_during_install: bool,
    ) -> Arc<dyn Component> {
        Arc::new(CountedComponent {
            id_value: id,
            already_installed: false,
            install_should_fail: fails,
            install_calls: Arc::new(AtomicU32::new(0)),
            uninstall_calls: Arc::new(AtomicU32::new(0)),
            progress_events: qq_progress(fails),
            cancel_during_install,
        })
    }

    async fn run_with_progress(plan: &DeployPlan) -> (DeployOutcome, Vec<ProgressKind>) {
        let (mut ctx, mut rx) = ActionCtx::with_capacity(256);
        let outcome = plan.run(&StubHost, &mut ctx).await.unwrap();
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event.kind);
        }
        (outcome, events)
    }

    fn lifecycle(events: &[ProgressKind]) -> Vec<ProgressKind> {
        events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    ProgressKind::Started { .. }
                        | ProgressKind::StepBegin { .. }
                        | ProgressKind::StepEnd { .. }
                        | ProgressKind::Finished { .. }
                )
            })
            .cloned()
            .collect()
    }

    #[tokio::test]
    async fn qq_extract_failure_keeps_parent_step_identity_and_closes_extract_once() {
        let plan = DeployPlan::builder()
            .step(
                "single",
                StepKind::Update,
                progress_component(ComponentId::Qq, true, false),
            )
            .build();
        let (outcome, events) = run_with_progress(&plan).await;
        assert!(!outcome.ok);
        assert_eq!(
            lifecycle(&events),
            [
                ProgressKind::Started { total_steps: 1 },
                ProgressKind::StepBegin {
                    step: 1,
                    message: "single (update)".into()
                },
                ProgressKind::StepEnd { step: 1, ok: false },
                ProgressKind::Finished { ok: false },
            ]
        );
        let failures: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                ProgressKind::Log {
                    level: ProgressLogLevel::Error,
                    message,
                } if message.starts_with("失败：") => Some(message.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(failures, ["失败：extract QQ"]);
        assert!(events.iter().any(|event| matches!(event,
            ProgressKind::StepProgress { step: 1, percent: 75, message, .. }
                if message == "extract QQ"
        )));
    }

    #[tokio::test]
    async fn qq_success_has_only_one_plan_started_and_finished() {
        let plan = DeployPlan::builder()
            .step(
                "single",
                StepKind::Update,
                progress_component(ComponentId::Qq, false, false),
            )
            .build();
        let (outcome, events) = run_with_progress(&plan).await;
        assert!(outcome.ok);
        assert_eq!(
            lifecycle(&events),
            [
                ProgressKind::Started { total_steps: 1 },
                ProgressKind::StepBegin {
                    step: 1,
                    message: "single (update)".into()
                },
                ProgressKind::StepEnd { step: 1, ok: true },
                ProgressKind::Finished { ok: true },
            ]
        );
    }

    #[tokio::test]
    async fn child_finished_does_not_end_plan_before_the_next_outer_step() {
        let plan = DeployPlan::builder()
            .force_install(
                "first",
                progress_component(ComponentId::NodeJs, false, false),
            )
            .force_install("second", progress_component(ComponentId::Qq, false, false))
            .build();
        let (outcome, events) = run_with_progress(&plan).await;
        assert!(outcome.ok);
        assert_eq!(
            lifecycle(&events),
            [
                ProgressKind::Started { total_steps: 2 },
                ProgressKind::StepBegin {
                    step: 1,
                    message: "first (force_install)".into()
                },
                ProgressKind::StepEnd { step: 1, ok: true },
                ProgressKind::StepBegin {
                    step: 2,
                    message: "second (force_install)".into()
                },
                ProgressKind::StepEnd { step: 2, ok: true },
                ProgressKind::Finished { ok: true },
            ]
        );
        assert!(events.iter().any(|event| matches!(event,
            ProgressKind::StepProgress { step: 2, percent: 75, message, .. }
                if message == "extract QQ"
        )));
    }

    #[tokio::test]
    async fn optional_failure_cannot_emit_a_successful_plan_finished() {
        let plan = DeployPlan::builder()
            .force_install(
                "optional",
                progress_component(ComponentId::NodeJs, true, false),
            )
            .last_fail_fast(false)
            .force_install(
                "required",
                progress_component(ComponentId::Qq, false, false),
            )
            .build();
        let (outcome, events) = run_with_progress(&plan).await;
        assert!(!outcome.ok);
        assert_eq!(events.last(), Some(&ProgressKind::Finished { ok: false }));
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, ProgressKind::Finished { .. }))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn cancellation_after_child_finished_cannot_report_plan_success() {
        let plan = DeployPlan::builder()
            .force_install(
                "cancelled",
                progress_component(ComponentId::NodeJs, false, true),
            )
            .force_install(
                "unreached",
                progress_component(ComponentId::Qq, false, false),
            )
            .build();
        let (outcome, events) = run_with_progress(&plan).await;
        assert_eq!(outcome.steps.len(), 1);
        assert_eq!(
            outcome.steps[0].status,
            crate::result::StepStatus::Cancelled
        );
        assert_eq!(events.last(), Some(&ProgressKind::Finished { ok: false }));
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, ProgressKind::Finished { .. }))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn run_executes_all_steps_in_order() {
        let (a, a_install, _) = comp(ComponentId::NodeJs, false, false);
        let (b, b_install, _) = comp(ComponentId::Qq, false, false);
        let plan = DeployPlan::builder()
            .ensure_installed("a", a)
            .ensure_installed("b", b)
            .build();
        let host = StubHost;
        let (mut ctx, _rx) = ActionCtx::new();
        let outcome = plan.run(&host, &mut ctx).await.unwrap();
        assert!(outcome.ok);
        assert_eq!(outcome.steps.len(), 2);
        assert_eq!(a_install.load(Ordering::SeqCst), 1);
        assert_eq!(b_install.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn ensure_installed_skips_when_already_installed() {
        let (a, a_install, _) = comp(ComponentId::NodeJs, true, false);
        let plan = DeployPlan::builder().ensure_installed("a", a).build();
        let host = StubHost;
        let (mut ctx, _rx) = ActionCtx::new();
        let outcome = plan.run(&host, &mut ctx).await.unwrap();
        assert!(outcome.ok);
        assert!(outcome.steps[0].skipped);
        assert_eq!(a_install.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn force_install_runs_even_when_detected() {
        let (a, a_install, _) = comp(ComponentId::NodeJs, true, false);
        let plan = DeployPlan::builder().force_install("a", a).build();
        let host = StubHost;
        let (mut ctx, _rx) = ActionCtx::new();
        let outcome = plan.run(&host, &mut ctx).await.unwrap();
        assert!(outcome.ok);
        assert_eq!(a_install.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn fail_fast_stops_after_failed_step() {
        let (a, a_install, _) = comp(ComponentId::NodeJs, false, false);
        let (b, b_install, _) = comp(ComponentId::Qq, false, true); // install fail
        let (c, c_install, _) = comp(ComponentId::NapCat, false, false);
        let plan = DeployPlan::builder()
            .ensure_installed("a", a)
            .ensure_installed("b", b)
            .ensure_installed("c", c)
            .build();
        let host = StubHost;
        let (mut ctx, _rx) = ActionCtx::new();
        let outcome = plan.run(&host, &mut ctx).await.unwrap();
        assert!(!outcome.ok);
        // a 跑了,b 跑了(失败),c 不会被执行
        assert_eq!(a_install.load(Ordering::SeqCst), 1);
        assert_eq!(b_install.load(Ordering::SeqCst), 1);
        assert_eq!(c_install.load(Ordering::SeqCst), 0);
        assert_eq!(outcome.steps.len(), 2);
    }

    #[tokio::test]
    async fn fail_continue_when_step_marked_optional() {
        let (a, a_install, _) = comp(ComponentId::NodeJs, false, true); // optional step fails
        let (b, b_install, _) = comp(ComponentId::Qq, false, false);
        let plan = DeployPlan::builder()
            .ensure_installed("a_optional", a)
            .last_fail_fast(false)
            .ensure_installed("b_required", b)
            .build();
        let host = StubHost;
        let (mut ctx, _rx) = ActionCtx::new();
        let outcome = plan.run(&host, &mut ctx).await.unwrap();
        // 整体非 ok(有失败 step)但 b 仍然跑了
        assert!(!outcome.ok);
        assert_eq!(a_install.load(Ordering::SeqCst), 1);
        assert_eq!(b_install.load(Ordering::SeqCst), 1);
        assert_eq!(outcome.steps.len(), 2);
    }

    #[tokio::test]
    async fn rollback_runs_uninstall_for_marked_steps_in_reverse() {
        let (a, _, a_un) = comp(ComponentId::NodeJs, false, false);
        let (b, _, b_un) = comp(ComponentId::Qq, false, false);
        let (c, _, c_un) = comp(ComponentId::NapCat, false, true); // 第三步 fails
        let plan = DeployPlan::builder()
            .ensure_installed("a", a)
            .last_rollback_on_failure(true)
            .ensure_installed("b", b)
            .last_rollback_on_failure(true)
            .ensure_installed("c", c)
            .last_rollback_on_failure(true)
            .build();
        let host = StubHost;
        let (mut ctx, _rx) = ActionCtx::new();
        let outcome = plan.run(&host, &mut ctx).await.unwrap();
        assert!(!outcome.ok);
        // a / b 应被 rollback,c 失败本身不会再 rollback(它没成功 install)
        assert_eq!(a_un.load(Ordering::SeqCst), 1);
        assert_eq!(b_un.load(Ordering::SeqCst), 1);
        assert_eq!(c_un.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn cancelled_ctx_aborts_remaining_steps() {
        let (a, a_install, _) = comp(ComponentId::NodeJs, false, false);
        let (b, b_install, _) = comp(ComponentId::Qq, false, false);
        let plan = DeployPlan::builder()
            .ensure_installed("a", a)
            .ensure_installed("b", b)
            .build();
        let host = StubHost;
        let (mut ctx, _rx) = ActionCtx::new();
        ctx.cancel();
        let outcome = plan.run(&host, &mut ctx).await.unwrap();
        assert!(!outcome.ok);
        assert_eq!(a_install.load(Ordering::SeqCst), 0);
        assert_eq!(b_install.load(Ordering::SeqCst), 0);
        assert_eq!(outcome.steps.len(), 1);
        assert_eq!(
            outcome.steps[0].status,
            crate::result::StepStatus::Cancelled
        );
    }

    #[tokio::test]
    async fn empty_plan_validation_error() {
        let plan = DeployPlan::new();
        let host = StubHost;
        let (mut ctx, _rx) = ActionCtx::new();
        let err = plan.run(&host, &mut ctx).await.unwrap_err();
        assert!(matches!(err, DeployError::InvalidPlan { .. }));
    }
}
