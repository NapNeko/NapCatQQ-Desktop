// NapCatQQ Desktop UI - Core Types

import type { LocalVersionSnapshot } from './generated/domain/LocalVersionSnapshot';
import type { DataLayoutConsolidateSnapshot } from './generated/domain/DataLayoutConsolidateSnapshot';

export type BootstrapStatus = 'ready' | 'migrating' | 'repair_required' | 'failed';

export type RepairAction =
    'open_data_dir' | 'export_migration_report' | 'restore_backup' | 'reauthenticate';

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
    /** 布局收敛结果;未跑时为 null/undefined */
    layout_consolidate?: DataLayoutConsolidateSnapshot | null;
}

// 与后端 ts-rs 派生的强类型对齐——直接 re-export 生成版本，避免手写漂移。
export type { BotActorState } from './generated/BotActorState';
export type { BotActorSnapshot } from './generated/BotActorSnapshot';
export type { LogSnapshot } from './generated/domain/LogSnapshot';
export type { LocalVersionSnapshot } from './generated/domain/LocalVersionSnapshot';
export type { DataLayoutConsolidateSnapshot } from './generated/domain/DataLayoutConsolidateSnapshot';
export type { DataRootMigratePreview } from './generated/domain/DataRootMigratePreview';
export type { DataRootTreeEntry } from './generated/domain/DataRootTreeEntry';
export type { DataRootMigratePhase } from './generated/domain/DataRootMigratePhase';
export type { DataRootMigrateProgress } from './generated/domain/DataRootMigrateProgress';
export type { DataRootMigrateResult } from './generated/domain/DataRootMigrateResult';
export type { CacheCategory } from './generated/domain/CacheCategory';
export type { CacheCategoryUsage } from './generated/domain/CacheCategoryUsage';
export type { CacheScan } from './generated/domain/CacheScan';
export type { CacheCleanupResult } from './generated/domain/CacheCleanupResult';
export type { ReleaseInfo } from './generated/domain/ReleaseInfo';
export type { ReleaseSnapshot } from './generated/domain/ReleaseSnapshot';
export type { AppSettings } from './generated/domain/AppSettings';
export type { AppSettingsDto } from './generated/domain/AppSettingsDto';
export type { AppFrameworkId } from './generated/domain/AppFrameworkId';
export type { AppInstanceId } from './generated/domain/AppInstanceId';
export type { AppInstance } from './generated/domain/AppInstance';
export type { AppInstanceOrigin } from './generated/domain/AppInstanceOrigin';
export type { AppInstanceState } from './generated/domain/AppInstanceState';
export type { AppProjectProbe } from './generated/domain/AppProjectProbe';
export type { ImportAppInstanceRequest } from './generated/domain/ImportAppInstanceRequest';
export type { AppPlacement } from './generated/domain/AppPlacement';
export type { AppFrameworkManifest } from './generated/domain/AppFrameworkManifest';
export type { PackageVersions } from './generated/domain/PackageVersions';
export type { AppLinkRecord } from './generated/domain/AppLinkRecord';
export type { AppConfigWrite } from './generated/domain/AppConfigWrite';
export type { CreateAppInstanceRequest } from './generated/domain/CreateAppInstanceRequest';
export type { AppPluginAction } from './generated/domain/AppPluginAction';
export type { AppStoreResource } from './generated/domain/AppStoreResource';
export type { OneBotLinkEndpoint } from './generated/domain/OneBotLinkEndpoint';
export type { OneBotLinkMode } from './generated/domain/OneBotLinkMode';
export type { OneBotLinkPlan } from './generated/domain/OneBotLinkPlan';
export type { AppInstanceWebUi } from './generated/domain/AppInstanceWebUi';
export type { AppWebUiAuthKind } from './generated/domain/AppWebUiAuthKind';
export type { AppWebUiAccount } from './generated/domain/AppWebUiAccount';
export type { AppTermsDoc } from './generated/domain/AppTermsDoc';
export type { AppPendingTerms } from './generated/domain/AppPendingTerms';
export type { AppConfigDocument } from './generated/domain/AppConfigDocument';
export type { AppConfigText } from './generated/domain/AppConfigText';
export type { AppConfigFormat } from './generated/domain/AppConfigFormat';
export type { AppConfigIssue } from './generated/domain/AppConfigIssue';
export type { AppConfigError } from './generated/domain/AppConfigError';
export type { AppConfigErrorKind } from './generated/domain/AppConfigErrorKind';
export type { OneBotEndpointExport } from './generated/domain/OneBotEndpointExport';
export type { WebUiPollerSettings } from './generated/domain/WebUiPollerSettings';
export type { ConfigExportResult } from './generated/ConfigExportResult';
export type { ConfigFrontendPreferences } from './generated/ConfigFrontendPreferences';
export type { ConfigImportResult } from './generated/ConfigImportResult';
export type { ConfigImportPreview } from './generated/ConfigImportPreview';
export type { PrepareExitDesktopResponse } from './generated/PrepareExitDesktopResponse';

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
export type { UninstallSupport } from './generated/domain/UninstallSupport';
export type { SupportedTarget } from './generated/domain/SupportedTarget';
export type { DetectedVersion } from './generated/domain/DetectedVersion';
export type { UnusableInstall } from './generated/domain/UnusableInstall';
export type { StepKind } from './generated/domain/StepKind';
// 依赖图：组件 requirements() 声明与 runtime 解析结果（crates/ncd-component/src/requirement.rs）
export type { Requirement } from './generated/domain/Requirement';
export type { RequirementPhase } from './generated/domain/RequirementPhase';
export type { RequirementStatus } from './generated/domain/RequirementStatus';
export type { VersionReq } from './generated/domain/VersionReq';
export type { HostPackageGroup } from './generated/domain/HostPackageGroup';
export type { DependencyTarget } from './generated/domain/DependencyTarget';
export type { DependencyNode } from './generated/domain/DependencyNode';
export type { DependencyPlan } from './generated/domain/DependencyPlan';
export type { RuntimeReadiness } from './generated/domain/RuntimeReadiness';
export type { LocalQqSource } from './generated/domain/LocalQqSource';
import type { SnowLumaLinuxPackage } from './generated/domain/SnowLumaLinuxPackage';
export type { SnowLumaLinuxPackage };
export type SnowLumaPackage = SnowLumaLinuxPackage;
export type { NodeEnvironmentCandidate } from './generated/domain/NodeEnvironmentCandidate';
export type { NodeProbeResult } from './generated/domain/NodeProbeResult';
export type { NodeSourceKind } from './generated/domain/NodeSourceKind';
export type { DiscoveredRemoteBot } from './generated/domain/DiscoveredRemoteBot';
export type { DiscoveredRemoteBotSource } from './generated/domain/DiscoveredRemoteBotSource';
export type { ImportableRemoteBot } from './generated/domain/ImportableRemoteBot';
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
export type { DockerContainerDetails } from './generated/domain/DockerContainerDetails';
export type { DockerContainerStats } from './generated/domain/DockerContainerStats';
export type { DockerNetworkInfo } from './generated/domain/DockerNetworkInfo';
export type { DockerVolumeInfo } from './generated/domain/DockerVolumeInfo';
export type { DockerResourceKind } from './generated/domain/DockerResourceKind';
export type { ImageInfo } from './generated/domain/ImageInfo';
export type { ImageRemoveOptions } from './generated/domain/ImageRemoveOptions';
export type { DockerFlavor } from './generated/domain/DockerFlavor';
export type { DockerPullLayerSnapshot } from './generated/domain/DockerPullLayerSnapshot';
export type { DockerPullSpec } from './generated/domain/DockerPullSpec';
export type { DockerDeploySpec } from './generated/domain/DockerDeploySpec';
// Rust 侧 DeployedContainer 只是 DockerImageReady 的类型别名，ts-rs 不给别名单独生成文件
export type { DockerImageReady as DeployedContainer } from './generated/domain/DockerImageReady';
export type { PortMapping } from './generated/domain/PortMapping';
export type { DockerInstallReport } from './generated/domain/DockerInstallReport';
export type { DockerInstallStatus } from './generated/domain/DockerInstallStatus';
export type { DeploymentTaskKind } from './generated/domain/DeploymentTaskKind';
export type { DeploymentTaskStatus } from './generated/domain/DeploymentTaskStatus';
export type { DeploymentTaskResource } from './generated/domain/DeploymentTaskResource';
export type { DeploymentTaskSnapshot } from './generated/domain/DeploymentTaskSnapshot';
export type { DeploymentTaskList } from './generated/domain/DeploymentTaskList';

export type BotFlavor = 'napcat' | 'snowluma';

export type BackendKind = 'local' | 'remote_ssh';

export type { BotStatus } from './generated/domain/BotStatus';
import type { BotStatus } from './generated/domain/BotStatus';

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
export type { NapCatWebuiAuthIssue } from './generated/NapCatWebuiAuthIssue';

// SnowLuma 后端类型—— re-export ts-rs 生成产物
export type { DaemonState } from './generated/DaemonState';
export type { SnowLumaStartMode } from './generated/domain/SnowLumaStartMode';
export type { SnowLumaLoginState } from './generated/SnowLumaLoginState';
export type { HookProcessStatus } from './generated/HookProcessStatus';
export type { HookProcessInfo } from './generated/HookProcessInfo';
export type { OneBotInstanceInfo } from './generated/OneBotInstanceInfo';
export type { SnowLumaAppConfig } from './generated/domain/SnowLumaAppConfig';

type DomainEventBody = import('./generated/domain/DomainEvent').DomainEvent;

// 所有发到 webview 的 IPC 事件 payload 都带顶层 v 版本号 envelope,后端改了形状前端能按版本分辨。
// 形如 { v: 1, kind: 'bot_log_appended', ... }。v 暂为可选,兼容历史 payload 与
// mock 事件;按 kind 判别的逻辑不受影响。
export type DomainEvent = DomainEventBody & { v?: number };
