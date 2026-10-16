//! Windows 远端应用由持久代理监督，Desktop 仅跟随状态和日志。
use super::*;
use ncd_domain::windows_remote::*;
use ncd_host::Os;
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) fn applies(host: &dyn Host) -> bool {
    host.os() == Os::Windows && host.locality() == Locality::Remote
}
fn workload_id(instance: &AppInstance) -> String {
    format!("app-{}", instance.id.as_str())
}

async fn request(
    host: &dyn Host,
    action: WindowsAgentAction,
) -> Result<WindowsAgentResult, AppFrameworkError> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let id = format!(
        "app-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let response = host
        .windows_agent_request(WindowsAgentRequest {
            v: WINDOWS_AGENT_PROTOCOL,
            request_id: id.clone(),
            action,
        })
        .await
        .map_err(host_err)?;
    if response.v != WINDOWS_AGENT_PROTOCOL || response.request_id != id {
        return Err(AppFrameworkError::Runtime(
            "Windows 代理响应身份不匹配".into(),
        ));
    }
    Ok(response.result)
}

pub(super) async fn status(
    host: &dyn Host,
    instance: &AppInstance,
) -> Result<Option<WindowsWorkloadStatus>, AppFrameworkError> {
    match request(
        host,
        WindowsAgentAction::Status {
            instance_id: workload_id(instance),
        },
    )
    .await?
    {
        WindowsAgentResult::Workload(status) => Ok(Some(status)),
        WindowsAgentResult::Error(error)
            if matches!(
                error.code.as_str(),
                "NotFound" | "WorkloadNotFound" | "not_found"
            ) =>
        {
            Ok(None)
        }
        WindowsAgentResult::Error(error) => Err(AppFrameworkError::Runtime(error.message)),
        _ => Err(AppFrameworkError::Runtime(
            "Windows 代理返回了错误的工作负载状态".into(),
        )),
    }
}

pub(super) async fn stop(host: &dyn Host, instance: &AppInstance) -> Result<(), AppFrameworkError> {
    let Some(current) = status(host, instance).await? else {
        if external_pid(host, instance).await?.is_some() {
            return Err(AppFrameworkError::Validation(
                "该应用由目标机上的其他程序启动；请先停止原进程，再由 Windows 远程代理启动".into(),
            ));
        }
        return Ok(());
    };
    if matches!(
        current.state,
        WindowsWorkloadState::Stopped | WindowsWorkloadState::Failed
    ) {
        return Ok(());
    }
    let generation = current.generation;
    match request(
        host,
        WindowsAgentAction::Stop {
            instance_id: workload_id(instance),
            generation: generation.clone(),
        },
    )
    .await?
    {
        WindowsAgentResult::Workload(_) => {}
        WindowsAgentResult::Error(error) => return Err(AppFrameworkError::Runtime(error.message)),
        _ => {
            return Err(AppFrameworkError::Runtime(
                "Windows 代理停止响应无效".into(),
            ));
        }
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let Some(current) = status(host, instance).await? else {
            return Ok(());
        };
        if current.generation != generation {
            return Err(AppFrameworkError::Runtime(
                "Windows 工作负载已更换代数，停止结果需重新确认".into(),
            ));
        }
        if matches!(
            current.state,
            WindowsWorkloadState::Stopped | WindowsWorkloadState::Failed
        ) {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(AppFrameworkError::Runtime(
                "Windows 应用仍在停止，请稍后刷新状态".into(),
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

pub(super) async fn external_pid(
    host: &dyn Host,
    instance: &AppInstance,
) -> Result<Option<u32>, AppFrameworkError> {
    let listing =
        crate::app_framework::supervisor::list_cwd_processes(host, &instance.install_dir).await?;
    let kind = crate::app_framework::supervisor::AppProcessKind::from_framework(
        instance.framework_id.as_str(),
    );
    Ok(crate::app_framework::supervisor::pick_app_pid(&listing, kind).map(|(pid, _)| pid))
}

async fn tail(
    host: &dyn Host,
    instance: &AppInstance,
    offset: u64,
) -> Result<WindowsLogChunk, AppFrameworkError> {
    match request(
        host,
        WindowsAgentAction::Tail {
            instance_id: workload_id(instance),
            offset,
            max_bytes: 256 * 1024,
        },
    )
    .await?
    {
        WindowsAgentResult::Log(log) => Ok(log),
        WindowsAgentResult::Error(error) => Err(AppFrameworkError::Runtime(error.message)),
        _ => Err(AppFrameworkError::Runtime(
            "Windows 代理日志响应无效".into(),
        )),
    }
}

pub(crate) async fn agent_log_tail(
    host: &dyn Host,
    instance: &AppInstance,
    lines: usize,
) -> Result<Vec<String>, AppFrameworkError> {
    if lines == 0 {
        return Ok(Vec::new());
    }
    if status(host, instance).await?.is_none() {
        return Ok(Vec::new());
    }
    // u64::MAX 请求末尾一个有界块；代理负责按实际文件大小夹取偏移。
    let chunk = tail(host, instance, u64::MAX).await?;
    let mut result: Vec<String> = chunk
        .data
        .lines()
        .rev()
        .take(lines)
        .map(str::to_string)
        .collect();
    result.reverse();
    Ok(result)
}

impl NativeAppRuntime {
    pub(super) async fn start_windows(
        &self,
        host: Arc<dyn Host>,
        instance: &AppInstance,
        spec: AppLaunchSpec,
    ) -> Result<u32, AppFrameworkError> {
        if status(host.as_ref(), instance).await?.is_none()
            && external_pid(host.as_ref(), instance).await?.is_some()
        {
            return Err(AppFrameworkError::Validation(
                "目标目录已有其他程序启动的应用，请先停止它，避免重复启动".into(),
            ));
        }
        let command = spec.command;
        let stdin = command
            .stdin
            .map(String::from_utf8)
            .transpose()
            .map_err(|_| AppFrameworkError::Runtime("Windows 代理仅接受 UTF-8 标准输入".into()))?;
        let launch = WindowsWorkloadSpec {
            instance_id: workload_id(instance),
            interactive: false,
            service_owned: false,
            session_id: None,
            restart_on_failure: instance.auto_start,
            command: WindowsCommandSpec {
                program: command.program,
                args: command.args,
                working_dir: Some(
                    command
                        .working_dir
                        .unwrap_or_else(|| HostPath::from_windows(&instance.install_dir))
                        .render_for(Os::Windows),
                ),
                environment: command.environment,
                stdin,
                elevated: command.elevated,
            },
        };
        let mut status = match request(host.as_ref(), WindowsAgentAction::Launch(launch)).await? {
            WindowsAgentResult::Workload(status) => status,
            WindowsAgentResult::Error(error) => {
                return Err(AppFrameworkError::Runtime(error.message));
            }
            _ => {
                return Err(AppFrameworkError::Runtime(
                    "Windows 代理启动响应无效".into(),
                ));
            }
        };
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while status.state == WindowsWorkloadState::Starting
            && status.pid.is_none()
            && tokio::time::Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(100)).await;
            status = self::status(host.as_ref(), instance)
                .await?
                .ok_or_else(|| AppFrameworkError::Runtime("Windows 代理丢失了启动记录".into()))?;
        }
        if !status.is_running() {
            if status.state == WindowsWorkloadState::WaitingForUserSession {
                match request(
                    host.as_ref(),
                    WindowsAgentAction::Stop {
                        instance_id: workload_id(instance),
                        generation: status.generation,
                    },
                )
                .await?
                {
                    WindowsAgentResult::Error(error) => {
                        return Err(AppFrameworkError::Runtime(format!(
                            "取消等待启动失败：{}",
                            error.message
                        )));
                    }
                    WindowsAgentResult::Workload(_) => {}
                    _ => return Err(AppFrameworkError::Runtime("取消等待启动的响应无效".into())),
                }
            }
            return Err(AppFrameworkError::Runtime(status.error.unwrap_or_else(
                || "请先登录目标 Windows 用户桌面，再启动应用".into(),
            )));
        }
        let pid = match status.pid {
            Some(pid) => pid,
            None => {
                stop(host.as_ref(), instance).await?;
                return Err(AppFrameworkError::Runtime("Windows 应用启动超时".into()));
            }
        };
        self.event_bus
            .publish(DomainEvent::app_instance_log_reset(instance.id.clone()));
        let task = self.spawn_windows_follow(host, instance.clone(), true);
        self.followers
            .lock()
            .await
            .insert(instance.id.clone(), task);
        Ok(pid)
    }

    pub(super) fn spawn_windows_follow(
        &self,
        host: Arc<dyn Host>,
        instance: AppInstance,
        backfill: bool,
    ) -> JoinHandle<()> {
        let store = Arc::clone(&self.store);
        let bus = Arc::clone(&self.event_bus);
        tokio::spawn(async move {
            let mut offset = if backfill { 0 } else { u64::MAX };
            let mut generation = String::new();
            let mut pending = String::new();
            loop {
                // 通讯失败保持未知：不可把网络抖动写成应用已退出。
                if let Ok(chunk) = tail(host.as_ref(), &instance, offset).await {
                    if !generation.is_empty() && generation != chunk.generation {
                        offset = 0;
                        pending.clear();
                        generation = chunk.generation;
                        bus.publish(DomainEvent::app_instance_log_reset(instance.id.clone()));
                        continue;
                    }
                    generation = chunk.generation;
                    offset = chunk.next_offset;
                    pending.push_str(&chunk.data);
                    while let Some(end) = pending.find('\n') {
                        let line: String = pending.drain(..=end).collect();
                        publish_app_log(&bus, &instance.id, line.trim_end_matches(['\r', '\n']));
                    }
                    if pending.len() > 256 * 1024 {
                        publish_app_log(&bus, &instance.id, &pending);
                        pending.clear();
                    }
                }
                match status(host.as_ref(), &instance).await {
                    Ok(Some(status))
                        if !status.is_running()
                            && !matches!(
                                status.state,
                                WindowsWorkloadState::WaitingForUserSession
                                    | WindowsWorkloadState::Unknown
                            ) =>
                    {
                        if !pending.is_empty() {
                            publish_app_log(&bus, &instance.id, &pending);
                        }
                        let failed = status.exit_code.is_none_or(|code| code != 0);
                        mark_stopped(
                            &store,
                            &bus,
                            &instance.id,
                            status.error.unwrap_or_else(|| "进程已退出".into()),
                            failed,
                        )
                        .await;
                        return;
                    }
                    _ => {}
                }
                tokio::time::sleep(REMOTE_POLL).await;
            }
        })
    }
}
