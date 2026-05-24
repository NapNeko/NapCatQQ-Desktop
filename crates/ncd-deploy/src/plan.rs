//! `DeployPlan`:多 Component 部署计划。
//!
//! 设计要点:
//! - 顺序执行:plan 内的 step 按 push 顺序执行(调用方负责依赖排序)
//! - 每个 step 都是 Component + StepKind 二元组
//! - 失败回滚走 `rollback_on_failure` 字段触发
//! - enum dispatch 通过 `Arc<dyn Component>` 走动态分发,plan 可序列化的元数据
//!   保留在 `name` / `kind`

use std::sync::Arc;

use ncd_component::Component;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::DeployError;

/// Step 操作类型(StepKind)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum StepKind {
    /// 探测 + 必要时 install(已装则跳过)
    EnsureInstalled,
    /// 强制 install(覆盖现有)
    ForceInstall,
    /// 走 update 路径(默认调用 Component::update,Component 可 override)
    Update,
    /// uninstall
    Uninstall,
    /// verify only(不改任何文件)
    Verify,
}

impl StepKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EnsureInstalled => "ensure_installed",
            Self::ForceInstall => "force_install",
            Self::Update => "update",
            Self::Uninstall => "uninstall",
            Self::Verify => "verify",
        }
    }
}

/// 单个部署步骤。
pub struct DeployStep {
    /// 步骤名称(用于日志 / 进度上报)
    pub name: String,
    /// 操作类型
    pub kind: StepKind,
    /// 目标 Component(`Arc<dyn Component>` 让 plan 可在多 task 间共享)
    pub component: Arc<dyn Component>,
    /// 失败时是否中断 plan(默认 true,某些"可选"step 可设 false 让 plan 继续)
    pub fail_fast: bool,
    /// 是否在 plan 失败回滚时跑 uninstall(默认 false:不主动卸载已 install 的 component,
    /// 因为可能产生环境破坏)。
    pub rollback_on_failure: bool,
}

impl std::fmt::Debug for DeployStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeployStep")
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("component_id", &self.component.id())
            .field("fail_fast", &self.fail_fast)
            .field("rollback_on_failure", &self.rollback_on_failure)
            .finish()
    }
}

/// 部署计划。
pub struct DeployPlan {
    pub steps: Vec<DeployStep>,
}

impl DeployPlan {
    /// 创建空 plan。
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    /// 创建一个 builder。
    pub fn builder() -> DeployBuilder {
        DeployBuilder { steps: Vec::new() }
    }

    /// 校验 plan 合法性:
    /// - 步骤非空
    /// - 步骤名唯一
    pub fn validate(&self) -> Result<(), DeployError> {
        if self.steps.is_empty() {
            return Err(DeployError::invalid_plan("empty step list"));
        }
        let mut seen = std::collections::HashSet::new();
        for step in &self.steps {
            if !seen.insert(step.name.as_str()) {
                return Err(DeployError::invalid_plan(format!(
                    "duplicate step name: {}",
                    step.name
                )));
            }
        }
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

impl Default for DeployPlan {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for DeployPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeployPlan")
            .field("steps", &self.steps)
            .finish()
    }
}

/// Builder:链式 API 构建 plan。
pub struct DeployBuilder {
    steps: Vec<DeployStep>,
}

impl DeployBuilder {
    /// 追加一个 ensure_installed step(component 还没装才装)。
    pub fn ensure_installed(self, name: impl Into<String>, component: Arc<dyn Component>) -> Self {
        self.step(name, StepKind::EnsureInstalled, component)
    }

    /// 追加一个 force_install step。
    pub fn force_install(self, name: impl Into<String>, component: Arc<dyn Component>) -> Self {
        self.step(name, StepKind::ForceInstall, component)
    }

    /// 追加一个 update step。
    pub fn update(self, name: impl Into<String>, component: Arc<dyn Component>) -> Self {
        self.step(name, StepKind::Update, component)
    }

    /// 追加一个 verify step。
    pub fn verify(self, name: impl Into<String>, component: Arc<dyn Component>) -> Self {
        self.step(name, StepKind::Verify, component)
    }

    /// 追加一个自定义 step。
    pub fn step(
        mut self,
        name: impl Into<String>,
        kind: StepKind,
        component: Arc<dyn Component>,
    ) -> Self {
        self.steps.push(DeployStep {
            name: name.into(),
            kind,
            component,
            fail_fast: true,
            rollback_on_failure: false,
        });
        self
    }

    /// 修改最后一个 step 的 fail_fast 标志。
    pub fn last_fail_fast(mut self, fail_fast: bool) -> Self {
        if let Some(last) = self.steps.last_mut() {
            last.fail_fast = fail_fast;
        }
        self
    }

    /// 修改最后一个 step 的 rollback_on_failure 标志。
    pub fn last_rollback_on_failure(mut self, rb: bool) -> Self {
        if let Some(last) = self.steps.last_mut() {
            last.rollback_on_failure = rb;
        }
        self
    }

    pub fn build(self) -> DeployPlan {
        DeployPlan { steps: self.steps }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use ncd_component::{
        ActionCtx, ActionError, Component, ComponentId, DetectedVersion, LaunchArgs, VerifyReport,
    };
    use ncd_host::{Host, HostCommand, Locality, Os};

    struct DummyComponent {
        id_value: ComponentId,
    }

    #[async_trait]
    impl Component for DummyComponent {
        fn id(&self) -> ComponentId {
            self.id_value
        }
        fn supported_targets(&self) -> &'static [(Os, Locality)] {
            &[(Os::Linux, Locality::Local)]
        }
        async fn detect(&self, _host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
            Ok(None)
        }
        async fn install(
            &self,
            _host: &dyn Host,
            _ctx: &mut ActionCtx,
        ) -> Result<(), ActionError> {
            Ok(())
        }
        async fn verify(&self, _host: &dyn Host) -> Result<VerifyReport, ActionError> {
            Ok(VerifyReport::ok())
        }
        fn launch_command(
            &self,
            _host: &dyn Host,
            _args: &LaunchArgs,
        ) -> Result<HostCommand, ActionError> {
            Ok(HostCommand::new("echo"))
        }
    }

    fn dummy(id: ComponentId) -> Arc<dyn Component> {
        Arc::new(DummyComponent { id_value: id })
    }

    #[test]
    fn builder_collects_steps_in_order() {
        let plan = DeployPlan::builder()
            .ensure_installed("nodejs", dummy(ComponentId::NodeJs))
            .ensure_installed("linuxqq", dummy(ComponentId::LinuxQq))
            .force_install("napcat", dummy(ComponentId::NapCat))
            .build();
        assert_eq!(plan.len(), 3);
        assert_eq!(plan.steps[0].name, "nodejs");
        assert_eq!(plan.steps[0].kind, StepKind::EnsureInstalled);
        assert_eq!(plan.steps[2].kind, StepKind::ForceInstall);
    }

    #[test]
    fn validate_rejects_empty_plan() {
        let plan = DeployPlan::new();
        assert!(matches!(
            plan.validate(),
            Err(DeployError::InvalidPlan { .. })
        ));
    }

    #[test]
    fn validate_rejects_duplicate_step_names() {
        let plan = DeployPlan::builder()
            .ensure_installed("step1", dummy(ComponentId::NodeJs))
            .ensure_installed("step1", dummy(ComponentId::LinuxQq))
            .build();
        let err = plan.validate().unwrap_err();
        assert!(matches!(err, DeployError::InvalidPlan { .. }));
        assert!(err.to_string().contains("duplicate"));
    }

    #[test]
    fn validate_passes_for_unique_names() {
        let plan = DeployPlan::builder()
            .ensure_installed("a", dummy(ComponentId::NodeJs))
            .ensure_installed("b", dummy(ComponentId::LinuxQq))
            .build();
        assert!(plan.validate().is_ok());
    }

    #[test]
    fn last_fail_fast_modifier_applies() {
        let plan = DeployPlan::builder()
            .ensure_installed("required", dummy(ComponentId::NodeJs))
            .ensure_installed("optional", dummy(ComponentId::NoVnc))
            .last_fail_fast(false)
            .build();
        assert!(plan.steps[0].fail_fast);
        assert!(!plan.steps[1].fail_fast);
    }

    #[test]
    fn last_rollback_on_failure_modifier_applies() {
        let plan = DeployPlan::builder()
            .force_install("napcat", dummy(ComponentId::NapCat))
            .last_rollback_on_failure(true)
            .build();
        assert!(plan.steps[0].rollback_on_failure);
    }

    #[test]
    fn step_kind_str_values_align_with_serde_snake_case() {
        assert_eq!(StepKind::EnsureInstalled.as_str(), "ensure_installed");
        assert_eq!(StepKind::ForceInstall.as_str(), "force_install");
        assert_eq!(StepKind::Update.as_str(), "update");
        assert_eq!(StepKind::Uninstall.as_str(), "uninstall");
        assert_eq!(StepKind::Verify.as_str(), "verify");
    }

    /// serde 序列化必须与 `as_str()` 字面量保持一致：前端拿到的是
    /// 同一份 snake_case 字符串。任何漂移会破坏 Tauri command 入参解析。
    #[test]
    fn step_kind_serde_aligns_with_as_str() {
        for kind in [
            StepKind::EnsureInstalled,
            StepKind::ForceInstall,
            StepKind::Update,
            StepKind::Uninstall,
            StepKind::Verify,
        ] {
            let serialized = serde_json::to_string(&kind).unwrap();
            let expected = format!("\"{}\"", kind.as_str());
            assert_eq!(serialized, expected);
            let decoded: StepKind = serde_json::from_str(&serialized).unwrap();
            assert_eq!(decoded, kind);
        }
    }

    #[test]
    fn debug_includes_component_id() {
        let plan = DeployPlan::builder()
            .ensure_installed("nodejs", dummy(ComponentId::NodeJs))
            .build();
        let dbg = format!("{plan:?}");
        assert!(dbg.contains("NodeJs"));
        // Debug 用 Rust 的 PascalCase variant name(StepKind 没派生 serde,所以不是 snake_case)
        assert!(dbg.contains("EnsureInstalled"));
    }
}
