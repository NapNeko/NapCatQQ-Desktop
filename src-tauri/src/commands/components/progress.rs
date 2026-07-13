//! 部署任务进度推送（components 子模块共用）

use ncd_component::{ProgressEvent, ProgressKind};
use ncd_runtime::DeploymentTaskContext;

pub(super) async fn push_task_progress(task_ctx: &DeploymentTaskContext, kind: ProgressKind) {
    task_ctx.push_progress(ProgressEvent::new(kind)).await;
}
