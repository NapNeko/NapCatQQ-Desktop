// NapCatQQ Desktop P2 UI - IPC Client
import { invoke } from '@tauri-apps/api/core';
import {
  BootstrapSnapshot,
  BotStatus,
  SpawnLocalBotRequest,
  StopLocalBotRequest,
  ConnectRemoteHostRequest,
  RemoteHostConnectionInfo,
  RemoteFileEntry,
  RemoteRuntimeStatusResponse,
  RemoteWebuiEndpointResponse,
} from './types';

// Check if running inside Tauri
export const isTauri = typeof window !== 'undefined' && (window as any).__TAURI_INTERNALS__ !== undefined;

// In-memory mock database for browser preview mode
let mockBootstrap: BootstrapSnapshot = {
  status: 'ready',
  schema_version: 'v3',
  report: {
    stage: 'completed',
    outcome: 'updated',
    warnings: [
      { code: 'W001', message: '发现旧版配置文件残留，已自动进行合并。' },
    ],
    source: {
      path: 'C:\\Users\\QIAO\\AppData\\Roaming\\NapCatQQ-Legacy',
      detected_version: 'v2.1.0',
    },
    backup: {
      backup_dir: 'C:\\Users\\QIAO\\AppData\\Roaming\\NapCatQQ-Desktop\\backup_v2_v3',
      timestamp: Date.now() - 3600000,
    },
    rules_applied: [
      'MigrateLocalAccounts',
      'NormalizePortBindings',
      'CleanLegacyTempCache'
    ],
    repair_actions: ['open_data_dir', 'export_migration_report'],
  },
};

let mockBots: BotStatus[] = [
  {
    bot_id: '10001',
    state: 'Running',
    pid: 14052,
    started_at: Math.floor((Date.now() - 7200000) / 1000),
    memory_rss_bytes: 124518400, // ~118MB
    server_total_memory_bytes: 17179869184, // 16GB
    backend_kind: 'local',
    runtime_target: 'local',
    extra: {
      ws_port: 3001,
      http_port: 3002,
      active_connections: 5,
    },
  },
  {
    bot_id: '10002',
    state: 'Stopped',
    pid: null,
    started_at: null,
    memory_rss_bytes: null,
    server_total_memory_bytes: 17179869184,
    backend_kind: 'local',
    runtime_target: 'local',
    extra: {},
  },
];

let mockRemoteConnections = new Map<string, RemoteHostConnectionInfo>();
let mockRemoteFiles = new Map<string, RemoteFileEntry[]>();

// Initialize some mock remote files
mockRemoteFiles.set('/', [
  { name: 'app', is_dir: true, size: 0 },
  { name: 'config', is_dir: true, size: 0 },
  { name: 'napcat_starter.sh', is_dir: false, size: 2048 },
  { name: 'README.md', is_dir: false, size: 450 },
]);
mockRemoteFiles.set('/config', [
  { name: 'onebot11.json', is_dir: false, size: 1024 },
  { name: 'quickstart.json', is_dir: false, size: 512 },
]);

// Core IPC Client Methods wrapping Tauri commands with browser fallback
export const client = {
  getBootstrapStatus: async (): Promise<BootstrapSnapshot> => {
    if (isTauri) {
      return await invoke<BootstrapSnapshot>('get_bootstrap_status');
    }
    return new Promise((resolve) => setTimeout(() => resolve(mockBootstrap), 250));
  },

  getAllBotStatuses: async (): Promise<BotStatus[]> => {
    if (isTauri) {
      return await invoke<BotStatus[]>('get_all_bot_statuses');
    }
    return new Promise((resolve) => setTimeout(() => resolve([...mockBots]), 300));
  },

  spawnLocalBot: async (request: SpawnLocalBotRequest): Promise<BotStatus> => {
    if (isTauri) {
      return await invoke<BotStatus>('spawn_local_bot', { request });
    }
    return new Promise((resolve) => {
      setTimeout(() => {
        const bot = mockBots.find((b) => b.bot_id === request.bot_id);
        if (bot) {
          bot.state = 'Running';
          bot.pid = Math.floor(Math.random() * 20000) + 1000;
          bot.started_at = Math.floor(Date.now() / 1000);
          bot.memory_rss_bytes = 48000000 + Math.floor(Math.random() * 50000000);
          resolve(bot);
        } else {
          const newBot: BotStatus = {
            bot_id: request.bot_id,
            state: 'Running',
            pid: Math.floor(Math.random() * 20000) + 1000,
            started_at: Math.floor(Date.now() / 1000),
            memory_rss_bytes: 64000000,
            server_total_memory_bytes: 17179869184,
            backend_kind: 'local',
            runtime_target: 'local',
            extra: {},
          };
          mockBots.push(newBot);
          resolve(newBot);
        }
      }, 500);
    });
  },

  stopLocalBot: async (request: StopLocalBotRequest): Promise<void> => {
    if (isTauri) {
      return await invoke<void>('stop_local_bot', { request });
    }
    return new Promise((resolve, reject) => {
      setTimeout(() => {
        const bot = mockBots.find((b) => b.bot_id === request.bot_id);
        if (bot) {
          bot.state = 'Stopped';
          bot.pid = null;
          bot.started_at = null;
          bot.memory_rss_bytes = null;
          resolve();
        } else {
          reject(`未找到 Bot ID: ${request.bot_id}`);
        }
      }, 500);
    });
  },

  connectRemoteHost: async (request: ConnectRemoteHostRequest): Promise<RemoteHostConnectionInfo> => {
    if (isTauri) {
      return await invoke<RemoteHostConnectionInfo>('connect_remote_host', { request });
    }
    return new Promise((resolve) => {
      setTimeout(() => {
        const info: RemoteHostConnectionInfo = {
          remote_id: request.remote_id,
          host: request.host,
          port: request.port || 22,
          username: request.username,
          webui_url: request.webui_url || `http://${request.host}:6099/webui`,
        };
        mockRemoteConnections.set(request.remote_id, info);
        resolve(info);
      }, 800);
    });
  },

  listRemoteFiles: async (remote_id: string, path: string): Promise<RemoteFileEntry[]> => {
    if (isTauri) {
      return await invoke<RemoteFileEntry[]>('list_remote_files', {
        request: { remote_id, path },
      });
    }
    return new Promise((resolve) => {
      setTimeout(() => {
        const normalized = path === '' || path === '/' ? '/' : path;
        const files = mockRemoteFiles.get(normalized) || [
          { name: 'mock_file_1.log', is_dir: false, size: 4096 },
          { name: 'mock_file_2.conf', is_dir: false, size: 1024 },
        ];
        resolve(files);
      }, 400);
    });
  },

  getRemoteRuntimeStatus: async (remote_id: string, bot_id: string): Promise<RemoteRuntimeStatusResponse> => {
    if (isTauri) {
      return await invoke<RemoteRuntimeStatusResponse>('get_remote_runtime_status', {
        request: { remote_id, bot_id },
      });
    }
    return new Promise((resolve) => {
      setTimeout(() => {
        resolve({
          remote_id,
          bot_id,
          status: {
            bot_id,
            state: 'Running',
            pid: 8848,
            started_at: Math.floor((Date.now() - 3600000) / 1000),
            memory_rss_bytes: 145000000,
            server_total_memory_bytes: 8589934592, // 8GB
            backend_kind: 'remote_ssh',
            runtime_target: 'remote_ssh',
            extra: {
              active_connections: 12,
              webui_enabled: true,
            },
          },
          backend_kind: 'remote_ssh',
          runtime_target: 'remote_ssh',
        });
      }, 500);
    });
  },

  getRemoteWebuiEndpoint: async (remote_id: string, bot_id: string): Promise<RemoteWebuiEndpointResponse> => {
    if (isTauri) {
      return await invoke<RemoteWebuiEndpointResponse>('get_remote_webui_endpoint', {
        request: { remote_id, bot_id },
      });
    }
    return new Promise((resolve) => {
      setTimeout(() => {
        const conn = mockRemoteConnections.get(remote_id);
        resolve({
          remote_id,
          bot_id,
          webui_url: conn?.webui_url || 'http://127.0.0.1:6099/webui',
        });
      }, 300);
    });
  },

  openDataDir: async (): Promise<string> => {
    if (isTauri) {
      return await invoke<string>('open_data_dir');
    }
    return 'C:\\Users\\QIAO\\AppData\\Roaming\\NapCatQQ-Desktop\\data';
  },

  exportMigrationReport: async (): Promise<string> => {
    if (isTauri) {
      return await invoke<string>('export_migration_report');
    }
    return 'C:\\Users\\QIAO\\AppData\\Roaming\\NapCatQQ-Desktop\\exports\\migration-report-1234567.json';
  },

  publishRuntimeStatus: async (): Promise<void> => {
    if (isTauri) {
      return await invoke<void>('publish_runtime_status');
    }
  },

  publishDemoEvent: async (): Promise<void> => {
    if (isTauri) {
      return await invoke<void>('publish_demo_event');
    }
  },
};
