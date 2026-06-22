// DomainEvent + DomainEventKind: 全系统事件数据类型
//
// 纯 serde 数据结构，零运行时依赖。行为逻辑（EventBus/BroadcastEventBus/订阅）
// 留在 ncd-runtime/events.rs。

use serde::{Deserialize, Serialize};

use crate::bot_actor::BotActorSnapshot;
use crate::bot_status::BotStatus;
use crate::daemon_state::{DaemonState, SnowLumaLoginState};
use crate::ids::BotId;
use crate::napcat_events::NapCatLoginInvalidationReason;
use crate::progress::ProgressEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainEventKind {
    BotStateChanged,
    BotStatusChanged,
    BotLogAppended,
    BotError,
    TaskProgress,
    #[serde(rename = "napcat_webui_available")]
    NapCatWebuiAvailable,
    BotProcessExited,
    #[serde(rename = "napcat_login_qrcode")]
    NapCatLoginQrcode,
    #[serde(rename = "napcat_login_qrcode_removed")]
    NapCatLoginQrcodeRemoved,
    #[serde(rename = "napcat_login_online")]
    NapCatLoginOnline,
    #[serde(rename = "napcat_login_invalidated")]
    NapCatLoginInvalidated,
    #[serde(rename = "snowluma_daemon_state_changed")]
    SnowLumaDaemonStateChanged,
    #[serde(rename = "snowluma_bot_injected")]
    SnowLumaBotInjected,
    #[serde(rename = "snowluma_uin_detected")]
    SnowLumaUinDetected,
    #[serde(rename = "snowluma_login_state_changed")]
    SnowLumaLoginStateChanged,
    #[serde(rename = "snowluma_pid_set_changed")]
    SnowLumaPidSetChanged,
    #[serde(rename = "snowluma_daemon_log")]
    SnowLumaDaemonLog,
    #[serde(rename = "snowluma_docker_endpoints_ready")]
    SnowLumaDockerEndpointsReady,
    #[serde(rename = "component_action_progress")]
    ComponentActionProgress,
    #[serde(rename = "docker_deploy_progress")]
    DockerDeployProgress,
    #[serde(rename = "docker_install_progress")]
    DockerInstallProgress,
    #[serde(rename = "desktop_log_appended")]
    DesktopLogAppended,
    #[serde(rename = "host_connection_lost")]
    HostConnectionLost,
    #[serde(rename = "host_connection_recovered")]
    HostConnectionRecovered,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DomainEvent {
    BotStateChanged {
        snapshot: BotActorSnapshot,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    BotStatusChanged {
        status: BotStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
    BotLogAppended {
        bot_id: BotId,
        line: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        channel: Option<String>,
    },
    BotError {
        bot_id: BotId,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hint: Option<String>,
    },
    TaskProgress {
        task_id: String,
        progress: u8,
        message: String,
    },
    #[serde(rename = "napcat_webui_available")]
    NapCatWebuiAvailable {
        bot_id: BotId,
        port: u16,
        token: String,
    },
    BotProcessExited {
        bot_id: BotId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    #[serde(rename = "napcat_login_qrcode")]
    NapCatLoginQrcode { bot_id: BotId, qrcode_url: String },
    #[serde(rename = "napcat_login_qrcode_removed")]
    NapCatLoginQrcodeRemoved { bot_id: BotId },
    #[serde(rename = "napcat_login_online")]
    NapCatLoginOnline { bot_id: BotId, online: bool },
    #[serde(rename = "napcat_login_invalidated")]
    NapCatLoginInvalidated {
        bot_id: BotId,
        reason: NapCatLoginInvalidationReason,
    },
    #[serde(rename = "snowluma_daemon_state_changed")]
    SnowLumaDaemonStateChanged {
        state: DaemonState,
        ref_count: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        server_id: Option<String>,
    },
    #[serde(rename = "snowluma_bot_injected")]
    SnowLumaBotInjected { bot_id: BotId, qq_pid: u32 },
    #[serde(rename = "snowluma_uin_detected")]
    SnowLumaUinDetected { bot_id: BotId, uin: String },
    #[serde(rename = "snowluma_login_state_changed")]
    SnowLumaLoginStateChanged {
        bot_id: BotId,
        state: SnowLumaLoginState,
    },
    #[serde(rename = "snowluma_pid_set_changed")]
    SnowLumaPidSetChanged { bot_id: BotId, pids: Vec<u32> },
    #[serde(rename = "snowluma_daemon_log")]
    SnowLumaDaemonLog { line: String },
    #[serde(rename = "snowluma_docker_endpoints_ready")]
    SnowLumaDockerEndpointsReady { bot_id: BotId },
    #[serde(rename = "component_action_progress")]
    ComponentActionProgress {
        task_id: String,
        event: ProgressEvent,
    },
    #[serde(rename = "docker_deploy_progress")]
    DockerDeployProgress {
        task_id: String,
        event: ProgressEvent,
    },
    #[serde(rename = "docker_install_progress")]
    DockerInstallProgress {
        task_id: String,
        event: ProgressEvent,
    },
    #[serde(rename = "desktop_log_appended")]
    DesktopLogAppended { line: String },
    #[serde(rename = "host_connection_lost")]
    HostConnectionLost {
        server_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        consecutive_failures: u32,
    },
    #[serde(rename = "host_connection_recovered")]
    HostConnectionRecovered { server_id: String, latency_ms: u64 },
}

pub const DOMAIN_EVENT_ENVELOPE_VERSION: u32 = 1;

impl DomainEvent {
    pub fn to_envelope_json(&self) -> Result<String, serde_json::Error> {
        let mut value = serde_json::to_value(self)?;
        if let serde_json::Value::Object(map) = &mut value {
            map.insert(
                "v".to_string(),
                serde_json::Value::from(DOMAIN_EVENT_ENVELOPE_VERSION),
            );
        }
        serde_json::to_string(&value)
    }

    pub fn kind(&self) -> DomainEventKind {
        match self {
            Self::BotStateChanged { .. } => DomainEventKind::BotStateChanged,
            Self::BotStatusChanged { .. } => DomainEventKind::BotStatusChanged,
            Self::BotLogAppended { .. } => DomainEventKind::BotLogAppended,
            Self::BotError { .. } => DomainEventKind::BotError,
            Self::TaskProgress { .. } => DomainEventKind::TaskProgress,
            Self::NapCatWebuiAvailable { .. } => DomainEventKind::NapCatWebuiAvailable,
            Self::BotProcessExited { .. } => DomainEventKind::BotProcessExited,
            Self::NapCatLoginQrcode { .. } => DomainEventKind::NapCatLoginQrcode,
            Self::NapCatLoginQrcodeRemoved { .. } => DomainEventKind::NapCatLoginQrcodeRemoved,
            Self::NapCatLoginOnline { .. } => DomainEventKind::NapCatLoginOnline,
            Self::NapCatLoginInvalidated { .. } => DomainEventKind::NapCatLoginInvalidated,
            Self::SnowLumaDaemonStateChanged { .. } => DomainEventKind::SnowLumaDaemonStateChanged,
            Self::SnowLumaBotInjected { .. } => DomainEventKind::SnowLumaBotInjected,
            Self::SnowLumaUinDetected { .. } => DomainEventKind::SnowLumaUinDetected,
            Self::SnowLumaLoginStateChanged { .. } => DomainEventKind::SnowLumaLoginStateChanged,
            Self::SnowLumaPidSetChanged { .. } => DomainEventKind::SnowLumaPidSetChanged,
            Self::SnowLumaDaemonLog { .. } => DomainEventKind::SnowLumaDaemonLog,
            Self::SnowLumaDockerEndpointsReady { .. } => {
                DomainEventKind::SnowLumaDockerEndpointsReady
            }
            Self::ComponentActionProgress { .. } => DomainEventKind::ComponentActionProgress,
            Self::DockerDeployProgress { .. } => DomainEventKind::DockerDeployProgress,
            Self::DockerInstallProgress { .. } => DomainEventKind::DockerInstallProgress,
            Self::DesktopLogAppended { .. } => DomainEventKind::DesktopLogAppended,
            Self::HostConnectionLost { .. } => DomainEventKind::HostConnectionLost,
            Self::HostConnectionRecovered { .. } => DomainEventKind::HostConnectionRecovered,
        }
    }

    pub fn tauri_event_name(&self) -> &'static str {
        match self {
            Self::BotStateChanged { .. } => "bot_state_changed",
            Self::BotStatusChanged { .. } => "bot_status_changed",
            Self::BotLogAppended { .. } => "bot_log_appended",
            Self::BotError { .. } => "bot_error",
            Self::TaskProgress { .. } => "task_progress",
            Self::NapCatWebuiAvailable { .. } => "napcat_webui_available",
            Self::BotProcessExited { .. } => "bot_process_exited",
            Self::NapCatLoginQrcode { .. } => "napcat_login_qrcode",
            Self::NapCatLoginQrcodeRemoved { .. } => "napcat_login_qrcode_removed",
            Self::NapCatLoginOnline { .. } => "napcat_login_online",
            Self::NapCatLoginInvalidated { .. } => "napcat_login_invalidated",
            Self::SnowLumaDaemonStateChanged { .. } => "snowluma_daemon_state_changed",
            Self::SnowLumaBotInjected { .. } => "snowluma_bot_injected",
            Self::SnowLumaUinDetected { .. } => "snowluma_uin_detected",
            Self::SnowLumaLoginStateChanged { .. } => "snowluma_login_state_changed",
            Self::SnowLumaPidSetChanged { .. } => "snowluma_pid_set_changed",
            Self::SnowLumaDaemonLog { .. } => "snowluma_daemon_log",
            Self::SnowLumaDockerEndpointsReady { .. } => "snowluma_docker_endpoints_ready",
            Self::ComponentActionProgress { .. } => "component_action_progress",
            Self::DockerDeployProgress { .. } => "docker_deploy_progress",
            Self::DockerInstallProgress { .. } => "docker_install_progress",
            Self::DesktopLogAppended { .. } => "desktop_log_appended",
            Self::HostConnectionLost { .. } => "host_connection_lost",
            Self::HostConnectionRecovered { .. } => "host_connection_recovered",
        }
    }

    pub fn bot_id(&self) -> Option<&BotId> {
        match self {
            Self::BotStateChanged { snapshot, .. } => Some(&snapshot.bot_id),
            Self::BotStatusChanged { status, .. } => Some(&status.bot_id),
            Self::BotLogAppended { bot_id, .. } => Some(bot_id),
            Self::BotError { bot_id, .. } => Some(bot_id),
            Self::TaskProgress { .. } => None,
            Self::NapCatWebuiAvailable { bot_id, .. } => Some(bot_id),
            Self::BotProcessExited { bot_id, .. } => Some(bot_id),
            Self::NapCatLoginQrcode { bot_id, .. } => Some(bot_id),
            Self::NapCatLoginQrcodeRemoved { bot_id, .. } => Some(bot_id),
            Self::NapCatLoginOnline { bot_id, .. } => Some(bot_id),
            Self::NapCatLoginInvalidated { bot_id, .. } => Some(bot_id),
            Self::SnowLumaDaemonStateChanged { .. } => None,
            Self::SnowLumaBotInjected { bot_id, .. } => Some(bot_id),
            Self::SnowLumaUinDetected { bot_id, .. } => Some(bot_id),
            Self::SnowLumaLoginStateChanged { bot_id, .. } => Some(bot_id),
            Self::SnowLumaPidSetChanged { bot_id, .. } => Some(bot_id),
            Self::SnowLumaDaemonLog { .. } => None,
            Self::SnowLumaDockerEndpointsReady { bot_id, .. } => Some(bot_id),
            Self::ComponentActionProgress { .. } => None,
            Self::DockerDeployProgress { .. } => None,
            Self::DockerInstallProgress { .. } => None,
            Self::DesktopLogAppended { .. } => None,
            Self::HostConnectionLost { .. } => None,
            Self::HostConnectionRecovered { .. } => None,
        }
    }

    // -- helper constructors --

    pub fn bot_state_changed(snapshot: BotActorSnapshot, reason: impl Into<String>) -> Self {
        Self::BotStateChanged {
            snapshot,
            reason: Some(reason.into()),
        }
    }

    pub fn bot_log(bot_id: impl Into<BotId>, line: impl Into<String>) -> Self {
        Self::BotLogAppended {
            bot_id: bot_id.into(),
            line: line.into(),
            channel: None,
        }
    }

    pub fn bot_status_changed(status: BotStatus, source: impl Into<String>) -> Self {
        Self::BotStatusChanged {
            status,
            source: Some(source.into()),
        }
    }

    pub fn bot_error(
        bot_id: impl Into<BotId>,
        message: impl Into<String>,
        hint: Option<String>,
    ) -> Self {
        Self::BotError {
            bot_id: bot_id.into(),
            message: message.into(),
            hint,
        }
    }

    pub fn task_progress(
        task_id: impl Into<String>,
        progress: u8,
        message: impl Into<String>,
    ) -> Self {
        Self::TaskProgress {
            task_id: task_id.into(),
            progress,
            message: message.into(),
        }
    }

    pub fn napcat_webui_available(
        bot_id: impl Into<BotId>,
        port: u16,
        token: impl Into<String>,
    ) -> Self {
        Self::NapCatWebuiAvailable {
            bot_id: bot_id.into(),
            port,
            token: token.into(),
        }
    }

    pub fn bot_process_exited(
        bot_id: impl Into<BotId>,
        exit_code: Option<i32>,
        reason: Option<String>,
    ) -> Self {
        Self::BotProcessExited {
            bot_id: bot_id.into(),
            exit_code,
            reason,
        }
    }

    pub fn napcat_login_qrcode(bot_id: impl Into<BotId>, qrcode_url: impl Into<String>) -> Self {
        Self::NapCatLoginQrcode {
            bot_id: bot_id.into(),
            qrcode_url: qrcode_url.into(),
        }
    }

    pub fn napcat_login_qrcode_removed(bot_id: impl Into<BotId>) -> Self {
        Self::NapCatLoginQrcodeRemoved {
            bot_id: bot_id.into(),
        }
    }

    pub fn napcat_login_online(bot_id: impl Into<BotId>, online: bool) -> Self {
        Self::NapCatLoginOnline {
            bot_id: bot_id.into(),
            online,
        }
    }

    pub fn napcat_login_invalidated(
        bot_id: impl Into<BotId>,
        reason: NapCatLoginInvalidationReason,
    ) -> Self {
        Self::NapCatLoginInvalidated {
            bot_id: bot_id.into(),
            reason,
        }
    }

    pub const SNOWLUMA_DAEMON_SCOPE_LOCAL: &str = "local";

    pub fn snowluma_daemon_state_changed(
        state: DaemonState,
        ref_count: u32,
        reason: Option<String>,
        server_id: Option<String>,
    ) -> Self {
        Self::SnowLumaDaemonStateChanged {
            state,
            ref_count,
            reason,
            server_id,
        }
    }

    pub fn snowluma_bot_injected(bot_id: impl Into<BotId>, qq_pid: u32) -> Self {
        Self::SnowLumaBotInjected {
            bot_id: bot_id.into(),
            qq_pid,
        }
    }

    pub fn snowluma_uin_detected(bot_id: impl Into<BotId>, uin: impl Into<String>) -> Self {
        Self::SnowLumaUinDetected {
            bot_id: bot_id.into(),
            uin: uin.into(),
        }
    }

    pub fn snowluma_login_state_changed(
        bot_id: impl Into<BotId>,
        state: SnowLumaLoginState,
    ) -> Self {
        Self::SnowLumaLoginStateChanged {
            bot_id: bot_id.into(),
            state,
        }
    }

    pub fn snowluma_pid_set_changed(bot_id: impl Into<BotId>, pids: Vec<u32>) -> Self {
        Self::SnowLumaPidSetChanged {
            bot_id: bot_id.into(),
            pids,
        }
    }

    pub fn snowluma_daemon_log(line: impl Into<String>) -> Self {
        Self::SnowLumaDaemonLog { line: line.into() }
    }

    pub fn component_action_progress(task_id: impl Into<String>, event: ProgressEvent) -> Self {
        Self::ComponentActionProgress {
            task_id: task_id.into(),
            event,
        }
    }

    pub fn docker_deploy_progress(task_id: impl Into<String>, event: ProgressEvent) -> Self {
        Self::DockerDeployProgress {
            task_id: task_id.into(),
            event,
        }
    }

    pub fn docker_install_progress(task_id: impl Into<String>, event: ProgressEvent) -> Self {
        Self::DockerInstallProgress {
            task_id: task_id.into(),
            event,
        }
    }

    pub fn desktop_log_appended(line: impl Into<String>) -> Self {
        Self::DesktopLogAppended { line: line.into() }
    }
}
