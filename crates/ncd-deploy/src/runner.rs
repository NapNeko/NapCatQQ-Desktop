//! `DeployPlan::run` 编排执行 + 失败回滚 + 进度上报。
//!
//! 把 plan 跑起来,emit 进度,失败时按需回滚已 install 的 step。

use std::time::Instant;

use ncd_component::{ActionCtx, ProgressKind, ProgressLogLevel};
use ncd_host::Host;

use crate::error::DeployError;
use crate::plan::{DeployPlan, DeployStep, StepKind};
use crate::result::{DeployOutcome, StepOutcome};

impl DeployPlan {
    /// 执行 plan。
    ///
    /// `ctx`:进度上报通道(子任务用 `ctx.child()` 派生取消子节点)
    /// 返回 [`DeployOutcome`],含每步状态 + 总耗时。
    pub async fn run(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<DeployOutcome, DeployError> {
        self.validate()?;

        let total_start = Instant::now();
        let total_steps = self.steps.len() as u32;
        ctx.emit(ProgressKind::Started { total_steps }).await;

        let mut outcomes: Vec<StepOutcome> = Vec::with_capacity(self.steps.len());
        let mut completed_for_rollback: Vec<&DeployStep> = Vec::new();

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
            let mut child_ctx = ctx.child();
            let result = run_single_step(step, host, &mut child_ctx).await;
            let dur = elapsed_ms(step_start);

            match result {
                Ok(skipped) => {
                    outcomes.push(StepOutcome::ok(&step.name, step.kind, dur, skipped));
                    completed_for_rollback.push(step);
                    ctx.emit(ProgressKind::StepEnd {
                        step: step_idx,
                        ok: true,
                    })
                    .await;
                }
                Err(e) => {
                    let err_str = format!("{e}");
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

        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(DeployOutcome::new(outcomes, elapsed_ms(total_start)))
    }
}

/// 跑单个 step。返回 `Ok(true)` 表示被 skip,`Ok(false)` 表示真的跑了。
async fn run_single_step(
    step: &DeployStep,
    host: &dyn Host,
    ctx: &mut ActionCtx,
) -> Result<bool, ncd_component::ActionError> {
    match step.kind {
        StepKind::EnsureInstalled => {
            let detected = step.component.detect(host).await?;
            if detected.is_some() {
                ctx.info(format!(
                    "step '{}' skipped: already installed",
                    step.name
                ))
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
    }
}

/// 对所有 `rollback_on_failure=true` 的已完成 step 倒序跑 uninstall。
async fn rollback(
    host: &dyn Host,
    ctx: &mut ActionCtx,
    completed: &[&DeployStep],
) -> Result<(), DeployError> {
    for step in completed.iter().rev() {
        if !step.rollback_on_failure {
            continue;
        }
        ctx.info(format!("rollback step '{}'", step.name)).await;
        if let Err(e) = step.component.uninstall(host, ctx).await {
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
        fn os(&self) -> Os { Os::Linux }
        fn arch(&self) -> Arch { Arch::X86_64 }
        fn locality(&self) -> Locality { Locality::Local }
        fn id(&self) -> &str { "stub" }
        fn shell(&self) -> &dyn ncd_host::HostShell { &ncd_host::shell::BashShell }
        fn pkg_manager(&self) -> Option<&dyn ncd_host::PackageManager> { None }
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
        async fn spawn(
            &self,
            _: HostCommand,
        ) -> Result<Box<dyn ncd_host::HostProcess>, HostError> {
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
    }

    #[async_trait]
    impl Component for CountedComponent {
        fn id(&self) -> ComponentId { self.id_value }
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
        async fn install(
            &self,
            _: &dyn Host,
            _: &mut ActionCtx,
        ) -> Result<(), ActionError> {
            self.install_calls.fetch_add(1, Ordering::SeqCst);
            if self.install_should_fail {
                Err(ActionError::other("simulated install failure"))
            } else {
                Ok(())
            }
        }
        async fn uninstall(
            &self,
            _: &dyn Host,
            _: &mut ActionCtx,
        ) -> Result<(), ActionError> {
            self.uninstall_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn verify(&self, _: &dyn Host) -> Result<VerifyReport, ActionError> {
            Ok(VerifyReport::ok())
        }
        fn launch_command(
            &self,
            _: &dyn Host,
            _: &LaunchArgs,
        ) -> Result<HostCommand, ActionError> {
            Ok(HostCommand::new("echo"))
        }
    }

    fn comp(
        id: ComponentId,
        already_installed: bool,
        install_fails: bool,
    ) -> (
        Arc<dyn Component>,
        Arc<AtomicU32>,
        Arc<AtomicU32>,
    ) {
        let install_calls = Arc::new(AtomicU32::new(0));
        let uninstall_calls = Arc::new(AtomicU32::new(0));
        let c = Arc::new(CountedComponent {
            id_value: id,
            already_installed,
            install_should_fail: install_fails,
            install_calls: install_calls.clone(),
            uninstall_calls: uninstall_calls.clone(),
        });
        (c, install_calls, uninstall_calls)
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
        let plan = DeployPlan::builder()
            .ensure_installed("a", a)
            .build();
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
        let plan = DeployPlan::builder()
            .force_install("a", a)
            .build();
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
            .ensure_installed("a", a).last_rollback_on_failure(true)
            .ensure_installed("b", b).last_rollback_on_failure(true)
            .ensure_installed("c", c).last_rollback_on_failure(true)
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
