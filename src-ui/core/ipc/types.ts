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
}

export type BotActorState = 'Stopped' | 'Starting' | 'Running' | 'Stopping' | 'Crashed' | 'Repairing';

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

export interface BotActorSnapshot {
  bot_id: string;
  state: BotActorState;
  revision: number;
  token_generation: number;
  pending_restart: boolean;
  last_transition?: string | null;
  last_error?: string | null;
}

export interface BatchResultResponse {
  succeeded: string[];
  failed: [string, string][];
}

export type NapCatLoginInvalidationReason = 'kicked' | 'logged_out';

export type DomainEvent =
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
  };
