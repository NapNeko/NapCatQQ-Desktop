//! `ncd-deploy`:NapCatQQ-Desktop 的部署编排层。
//!
//! 蓝图 §5.5 / M5:把 `Component` 列表 + `Host` 编排成 `DeployPlan`,
//! 处理顺序执行 / 失败回滚 / fallback 链 / 流式进度上报。
//!
//! ## 三维抽象的最上层
//!
//! ```text
//! Component(WHAT)  ×  Host(WHERE)  ×  Action(VERB)
//!     ↓                 ↓                ↓
//! ncd-component     ncd-host         ncd-component
//!                      ↓
//!                  ncd-deploy(本 crate)
//! ```
//!
//! ## 当前(M5.1)
//!
//! - ✅ `DeployPlan` / `DeployStep` / `StepKind` 数据结构
//! - ✅ `DeployBuilder` 链式 API
//! - ⏳ `DeployPlan::run`(M5.2)
//! - ⏳ Fallback 链 / 失败回滚(M5.2)

pub mod error;
pub mod plan;
pub mod result;
pub mod runner;

pub use error::DeployError;
pub use plan::{DeployBuilder, DeployPlan, DeployStep, StepKind};
pub use result::{DeployOutcome, StepOutcome, StepStatus};
