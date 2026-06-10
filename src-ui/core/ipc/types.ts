// NapCatQQ Desktop P2 UI - Core Types

export type BootstrapStatus = 'ready' | 'migrating' | 'repair_required' | 'failed';

export type RepairAction = 'open_data_dir' | 'export_migration_report' | 'restore_backup' | 'reauthenticate';

export type SchemaVersion = string;

export type MigrationStage = 'pending' | 'running' | 'completed' | 'repair_required' | 'failed';

export type MigrationOutcome = 'no_change' | 'updated' | 'needs_repair';

export interface MigrationWarning {
    code: string;
    message: string;
}

export interface BackupInfo {
    backup_dir: string;
    timestamp: number;
}

export interface MigrationSource {
    path: string;
    detected_version: string;
}

export interface MigrationReport {
    stage: MigrationStage;
    outcome: MigrationOutcome;
    warnings: MigrationWarning[];
    source: MigrationSource | null;
    backup: BackupInfo | null;
    rules_applied: string[];
    repair_actions: RepairAction[];
}

export interface BootstrapSnapshot {
    status: BootstrapStatus;
    schema_version: SchemaVersion;
    report: MigrationReport;
    data_root: string;
    local_versions: LocalVersionSnapshot;
}

// 与后端 ts-rs 派生的强类型对齐——直接 re-export 生成版本，避免手写漂移。
export type { BotActorState } from './generated/BotActorState';
export type { BotActorSnapshot } from './generated/BotActorSnapshot';
export type { LocalVersionSnapshot } from './generated/domain/LocalVersionSnapshot';
export type { ReleaseInfo } from './generated/domain/ReleaseInfo';
export type { ReleaseSnapshot } from './generated/domain/ReleaseSnapshot';
export type { AppSettings } from './generated/domain/AppSettings';
export type { AppSettingsDto } from './generated/domain/AppSettingsDto';
export type { WebUiPollerSettings } from './generated/domain/WebUiPollerSettings';
export type { ConfigExportResult } from './generated/ConfigExportResult';
export type { ConfigImportResult } from './generated/ConfigImportResult';
export type { ConfigImportPreview } from './generated/ConfigImportPreview';

// ProgressEvent / ProgressKind / LogLevel 由 ts-rs 自动生成，re-export 保证
// wire format 与后端一致。注意 ProgressEvent.timestamp_ms 是 bigint
// (Rust u64 默认映射)，UI 侧消费时应在 domain 层用 Number() 转 number；
// 边界写入时反过来用 BigInt() 包一下 Date.now()。
export type { LogLevel } from './generated/domain/LogLevel';
export type { ProgressLogLevel } from './generated/domain/ProgressLogLevel';
export type { ProgressKind } from './generated/domain/ProgressKind';
export type { ProgressEvent } from './generated/domain/ProgressEvent';

// ─── Component 层强类型（ts-rs 生成产物 re-export，杜绝手写漂移） ───
//
// 与后端 `crates/ncd-component/src/types.rs` + `crates/ncd-deploy/src/plan.rs`
// + `crates/ncd-host/src/host.rs` 派生产物对齐。

export type { ComponentId } from './generated/domain/ComponentId';
export type { ComponentCategory } from './generated/domain/ComponentCategory';
export type { ComponentInfo } from './generated/domain/ComponentInfo';
export type { SupportedTarget } from './generated/domain/SupportedTarget';
export type { DetectedVersion } from './generated/domain/DetectedVersion';
export type { StepKind } from './generated/domain/StepKind';
export type { Os } from './generated/domain/Os';
export type { Locality } from './generated/domain/Locality';

// `ComponentDetectResult` 是 Tauri command 出参，定义在 src-tauri，导出到
// `generated/domain/`（与其它 domain 类型同目录）。
export type { ComponentDetectResult } from './generated/domain/ComponentDetectResult';

// ─── Docker 管理面强类型（对齐 crates/ncd-domain/src/docker.rs） ───
export type { DockerStatus } from './generated/domain/DockerStatus';
export type { ContainerInfo } from './generated/domain/ContainerInfo';
export type { ContainerState } from './generated/domain/ContainerState';
export type { ContainerAction } from './generated/domain/ContainerAction';
export type { DockerFlavor } from './generated/domain/DockerFlavor';
export type { DockerDeploySpec } from './generated/domain/DockerDeploySpec';
export type { DeployedContainer } from './generated/domain/DeployedContainer';
export type { PortMapping } from './generated/domain/PortMapping';
export type { DockerInstallReport } from './generated/domain/DockerInstallReport';
export type { DockerInstallStatus } from './generated/domain/DockerInstallStatus';

import type { BotActorState } from './generated/BotActorState';
import type { BotActorSnapshot } from './generated/BotActorSnapshot';
import type { LocalVersionSnapshot } from './generated/domain/LocalVersionSnapshot';
import type { ProgressEvent } from './generated/domain/ProgressEvent';

export type BotFlavor = 'napcat' | 'snowluma';

export type BackendKind = 'local' | 'remote_ssh';

export interface BotStatus {
    bot_id: string;
    state: BotActorState;
    pid?: number | null;
    started_at?: number | null;
    memory_rss_bytes?: number | null;
    server_total_memory_bytes?: number | null;
    extra?: Record<string, any>;
    flavor?: BotFlavor; // client-side decorator if needed, or in extra
    backend_kind?: BackendKind; // local or remote_ssh
    runtime_target?: string;
}

export interface ConnectRemoteHostRequest {
    remote_id: string;
    host: string;
    port?: number;
    username: string;
    password?: string | null;
    webui_url?: string | null;
}

export interface RemoteHostConnectionInfo {
    remote_id: string;
    host: string;
    port: number;
    username: string;
    webui_url?: string | null;
}

export interface RemoteRuntimeStatusResponse {
    remote_id: string;
    bot_id: string;
    status: BotStatus;
    backend_kind?: BackendKind | null;
    runtime_target?: string | null;
}

export interface RemoteWebuiEndpointResponse {
    remote_id: string;
    bot_id: string;
    webui_url?: string | null;
}

export interface RemoteFileEntry {
    name: string;
    is_dir: boolean;
    size: number;
}

export interface BatchResultResponse {
    succeeded: string[];
    failed: [string, string][];
}

export type NapCatLoginInvalidationReason = 'kicked' | 'logged_out';

// SnowLuma 后端类型—— re-export ts-rs 生成产物
export type { DaemonState } from './generated/DaemonState';
export type { SnowLumaStartMode } from './generated/domain/SnowLumaStartMode';
export type { SnowLumaLoginState } from './generated/SnowLumaLoginState';
export type { HookProcessStatus } from './generated/HookProcessStatus';
export type { HookProcessInfo } from './generated/HookProcessInfo';
export type { OneBotInstanceInfo } from './generated/OneBotInstanceInfo';
export type { SnowLumaAppConfig } from './generated/domain/SnowLumaAppConfig';

import type { DaemonState } from './generated/DaemonState';
import type { SnowLumaLoginState } from './generated/SnowLumaLoginState';

// 按 kind 区分的判别联合(payload body)。统一通过下方 DomainEvent 带上 v envelope。
type DomainEventBody =
    | {
        kind: 'bot_state_changed';
        snapshot: BotActorSnapshot;
        reason?: string | null;
    }
    | {
        kind: 'bot_status_changed';
        status: BotStatus;
        source?: string | null;
    }
    | {
        kind: 'bot_log_appended';
        bot_id: string;
        line: string;
        channel?: string | null;
    }
    | {
        kind: 'bot_error';
        bot_id: string;
        message: string;
        hint?: string | null;
    }
    | {
        kind: 'task_progress';
        task_id: string;
        progress: number;
        message: string;
    }
    | {
        kind: 'napcat_webui_available';
        bot_id: string;
        port: number;
        token: string;
    }
    | {
        kind: 'bot_process_exited';
        bot_id: string;
        exit_code?: number | null;
        reason?: string | null;
    }
    | {
        kind: 'napcat_login_qrcode';
        bot_id: string;
        qrcode_url: string;
    }
    | {
        kind: 'napcat_login_qrcode_removed';
        bot_id: string;
    }
    | {
        kind: 'napcat_login_online';
        bot_id: string;
        online: boolean;
    }
    | {
        kind: 'napcat_login_invalidated';
        bot_id: string;
        reason: NapCatLoginInvalidationReason;
    }
    | {
        kind: 'snowluma_daemon_state_changed';
        state: DaemonState;
        ref_count: number;
        reason?: string | null;
    }
    | {
        kind: 'snowluma_bot_injected';
        bot_id: string;
        qq_pid: number;
    }
    | {
        kind: 'snowluma_uin_detected';
        bot_id: string;
        uin: string;
    }
    | {
        kind: 'snowluma_login_state_changed';
        bot_id: string;
        state: SnowLumaLoginState;
    }
    | {
        kind: 'snowluma_pid_set_changed';
        bot_id: string;
        pids: number[];
    }
    | {
        kind: 'snowluma_daemon_log';
        line: string;
    }
    | {
        kind: 'component_action_progress';
        task_id: string;
        event: ProgressEvent;
    }
    | {
        kind: 'docker_deploy_progress';
        task_id: string;
        event: ProgressEvent;
    }
    | {
        kind: 'desktop_log_appended';
        line: string;
    };

// 所有发到 webview 的 IPC 事件 payload 都带顶层 v 版本号 envelope(R14:版本化)。
// 形如 { v: 1, kind: 'bot_log_appended', ... }。v 暂为可选,兼容历史 payload 与
// mock 事件;按 kind 判别的逻辑不受影响。
export type DomainEvent = DomainEventBody & { v?: number };
