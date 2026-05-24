//! `ncd-deploy`:NapCatQQ-Desktop 的部署编排层。
//!
//! 把 `Component` 列表 + `Host` 编排成 `DeployPlan`,处理顺序执行 / 失败回滚 /
//! fallback 链 / 流式进度上报。
//!
//! 在 Component × Host × Action 三维抽象中,本 crate 处于最上层:
//! `ncd-component` 提供 Component(WHAT)和 Action(VERB),`ncd-host` 提供
//! Host(WHERE),本 crate 把它们拼起来。

pub mod error;
pub mod plan;
pub mod result;
pub mod runner;

pub use error::DeployError;
pub use plan::{DeployBuilder, DeployPlan, DeployStep, StepKind};
pub use result::{DeployOutcome, StepOutcome, StepStatus};
