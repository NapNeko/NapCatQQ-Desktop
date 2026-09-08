//! 把 Karin 插件装更卸接到 DeploymentTask 进度通道。

use std::sync::Arc;

use ncd_appframework::PluginLogSink;
use ncd_domain::{
    AppInstanceId, AppPluginAction, AppStoreResource, ProgressEvent, ProgressKind, ProgressLogLevel,
};

use super::manager::AppManager;
use crate::deploy::tasks::{DeploymentTaskContext, DeploymentTaskRunResult};

pub async fn run_app_plugin_task(
    app_manager: Arc<AppManager>,
    instance_id: AppInstanceId,
    plugin_name: String,
    action: AppPluginAction,
    resource: AppStoreResource,
    ctx: DeploymentTaskContext,
) -> DeploymentTaskRunResult {
    if ctx.is_cancelled() {
        return DeploymentTaskRunResult::failed("已取消");
    }
    let verb = match action {
        AppPluginAction::Install => "安装",
        AppPluginAction::Update => "更新",
        AppPluginAction::Uninstall => "卸载",
    };
    ctx.push_progress(ProgressEvent::new(ProgressKind::Started { total_steps: 1 }))
        .await;
    ctx.push_progress(ProgressEvent::new(ProgressKind::StepBegin {
        step: 1,
        message: format!("{verb} {plugin_name}"),
    }))
    .await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let sink: PluginLogSink = Arc::new(move |line| {
        let _ = tx.send(line);
    });
    let ctx_log = ctx.clone();
    let drain = tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            ctx_log
                .push_progress(ProgressEvent::new(ProgressKind::Log {
                    level: ProgressLogLevel::Info,
                    message: line,
                }))
                .await;
        }
    });

    let result = app_manager
        .run_store_op(&instance_id, &plugin_name, action, resource, Some(&sink))
        .await;
    drop(sink);
    let _ = drain.await;

    match result {
        Ok(()) => {
            ctx.push_progress(ProgressEvent::new(ProgressKind::StepEnd { step: 1, ok: true }))
                .await;
            ctx.push_progress(ProgressEvent::new(ProgressKind::Finished { ok: true }))
                .await;
            DeploymentTaskRunResult::ok("完成")
        }
        Err(e) => {
            let msg = e.to_string();
            ctx.push_progress(ProgressEvent::new(ProgressKind::Log {
                level: ProgressLogLevel::Error,
                message: msg.clone(),
            }))
            .await;
            ctx.push_progress(ProgressEvent::new(ProgressKind::StepEnd { step: 1, ok: false }))
                .await;
            ctx.push_progress(ProgressEvent::new(ProgressKind::Finished { ok: false }))
                .await;
            DeploymentTaskRunResult::failed(msg)
        }
    }
}
