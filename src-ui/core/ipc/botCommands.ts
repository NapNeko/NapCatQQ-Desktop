// NapCatQQ Desktop P2 UI - Bot Commands IPC Client
import { invoke } from '@tauri-apps/api/core';
import { isTauri } from './client';
import { BotActorSnapshot, BatchResultResponse } from './types';
import { BotConfig } from './generated/BotConfig';
import { emitMockEvent } from './events';

/// 日志快照（与 Rust `LogSnapshot` 对齐）。
export interface LogSnapshot {
  lines: string[];
  total_lines: number;
}

// In-memory mock database for browser preview mode
let mockSnapshots: BotActorSnapshot[] = [
  {
    bot_id: '10001',
    state: 'running',
    revision: 1,
    token_generation: 1,
    pending_restart: false,
    last_transition: 'Start SUCCESS',
    last_error: undefined,
  },
  {
    bot_id: '10002',
    state: 'stopped',
    revision: 1,
    token_generation: 1,
    pending_restart: false,
    last_transition: 'Stopped gracefully',
    last_error: undefined,
  },
];

export const botCommands = {
  listBotSnapshots: async (): Promise<BotActorSnapshot[]> => {
    if (isTauri) {
      return await invoke<BotActorSnapshot[]>('list_bot_snapshots');
    }
    return new Promise((resolve) => setTimeout(() => resolve([...mockSnapshots]), 200));
  },

  getBotSnapshot: async (botId: string): Promise<BotActorSnapshot> => {
    if (isTauri) {
      return await invoke<BotActorSnapshot>('get_bot_snapshot', { botId });
    }
    return new Promise((resolve, reject) => {
      setTimeout(() => {
        const snap = mockSnapshots.find(s => s.bot_id === botId);
        if (snap) {
          resolve({ ...snap });
        } else {
          reject(`Bot not found: ${botId}`);
        }
      }, 200);
    });
  },

  getBotConfig: async (botId: string): Promise<BotConfig | null> => {
    if (isTauri) {
      return await invoke<BotConfig | null>('get_bot_config', { botId });
    }
    return new Promise((resolve) => {
      setTimeout(() => {
        const snap = mockSnapshots.find(s => s.bot_id === botId);
        if (snap) {
          resolve({
            bot: {
              name: `Bot-${botId.slice(-2)}`,
              QQID: Number(botId),
              musicSignUrl: 'http://sign.example.com/api',
              autoRestartSchedule: { enable: false, time_unit: 'h', duration: 6 },
              offlineAutoRestart: false,
              runtime_target: 'local',
              backend_type: 'napcat',
            },
            connect: {
              httpServers: [
                {
                  enable: true,
                  name: 'HTTP-API',
                  messagePostFormat: 'array',
                  token: 'secret-123',
                  debug: false,
                  host: '127.0.0.1',
                  port: 3000,
                  enableCors: true,
                  enableWebsocket: false,
                  path: '/',
                }
              ],
              httpSseServers: [],
              httpClients: [],
              websocketServers: [],
              websocketClients: [],
              plugins: [],
            },
            advanced: {
              autoStart: false,
              offlineNotice: true,
              parseMultMsg: true,
              packetServer: '',
              packetBackend: 'auto',
              enableLocalFile2Url: false,
              fileLog: false,
              consoleLog: true,
              fileLogLevel: 'debug',
              consoleLogLevel: 'info',
              o3HookMode: 1,
              bypass: { hook: false, window: false, module: false, process: false, container: false, js: false },
            },
          });
        } else {
          resolve(null);
        }
      }, 200);
    });
  },

  startBot: async (botId: string): Promise<BotActorSnapshot> => {
    if (isTauri) {
      return await invoke<BotActorSnapshot>('start_bot', { botId });
    }
    return new Promise((resolve, reject) => {
      setTimeout(() => {
        const snap = mockSnapshots.find(s => s.bot_id === botId);
        if (snap) {
          // 1. Transition to Starting
          snap.state = 'starting';
          snap.revision += 1;
          snap.last_transition = 'Manual starting';
          emitMockEvent({
            kind: 'bot_state_changed',
            snapshot: { ...snap },
            reason: 'manual_start'
          });

          // 2. Schedule transition to Running after 1.5s
          setTimeout(() => {
            snap.state = 'running';
            snap.revision += 1;
            snap.last_transition = 'Start SUCCESS';
            emitMockEvent({
              kind: 'bot_state_changed',
              snapshot: { ...snap },
              reason: 'startup_complete'
            });
          }, 1500);

          resolve({ ...snap });
        } else {
          reject(`Bot not found: ${botId}`);
        }
      }, 200);
    });
  },

  stopBot: async (botId: string): Promise<BotActorSnapshot> => {
    if (isTauri) {
      return await invoke<BotActorSnapshot>('stop_bot', { botId });
    }
    return new Promise((resolve, reject) => {
      setTimeout(() => {
        const snap = mockSnapshots.find(s => s.bot_id === botId);
        if (snap) {
          // 1. Transition to Stopping
          snap.state = 'stopping';
          snap.revision += 1;
          snap.last_transition = 'Manual stopping';
          emitMockEvent({
            kind: 'bot_state_changed',
            snapshot: { ...snap },
            reason: 'manual_stop'
          });

          // 2. Schedule transition to Stopped after 1.5s
          setTimeout(() => {
            snap.state = 'stopped';
            snap.revision += 1;
            snap.last_transition = 'Stopped gracefully';
            emitMockEvent({
              kind: 'bot_state_changed',
              snapshot: { ...snap },
              reason: 'shutdown_complete'
            });
          }, 1500);

          resolve({ ...snap });
        } else {
          reject(`Bot not found: ${botId}`);
        }
      }, 200);
    });
  },

  batchStartBots: async (botIds: string[]): Promise<BatchResultResponse> => {
    if (isTauri) {
      return await invoke<BatchResultResponse>('batch_start_bots', { botIds });
    }
    return new Promise((resolve) => {
      setTimeout(() => {
        const succeeded: string[] = [];
        const failed: [string, string][] = [];
        for (const id of botIds) {
          const snap = mockSnapshots.find(s => s.bot_id === id);
          if (snap) {
            snap.state = 'starting';
            snap.revision += 1;
            snap.last_transition = 'Batch manual starting';
            succeeded.push(id);
            emitMockEvent({
              kind: 'bot_state_changed',
              snapshot: { ...snap },
              reason: 'batch_start'
            });

            setTimeout(() => {
              snap.state = 'running';
              snap.revision += 1;
              snap.last_transition = 'Start SUCCESS';
              emitMockEvent({
                kind: 'bot_state_changed',
                snapshot: { ...snap },
                reason: 'startup_complete'
              });
            }, 1500);
          } else {
            failed.push([id, 'Bot config not found']);
          }
        }
        resolve({ succeeded, failed });
      }, 300);
    });
  },

  batchStopBots: async (botIds: string[]): Promise<BatchResultResponse> => {
    if (isTauri) {
      return await invoke<BatchResultResponse>('batch_stop_bots', { botIds });
    }
    return new Promise((resolve) => {
      setTimeout(() => {
        const succeeded: string[] = [];
        const failed: [string, string][] = [];
        for (const id of botIds) {
          const snap = mockSnapshots.find(s => s.bot_id === id);
          if (snap) {
            snap.state = 'stopping';
            snap.revision += 1;
            snap.last_transition = 'Batch manual stopping';
            succeeded.push(id);
            emitMockEvent({
              kind: 'bot_state_changed',
              snapshot: { ...snap },
              reason: 'batch_stop'
            });

            setTimeout(() => {
              snap.state = 'stopped';
              snap.revision += 1;
              snap.last_transition = 'Stopped gracefully';
              emitMockEvent({
                kind: 'bot_state_changed',
                snapshot: { ...snap },
                reason: 'shutdown_complete'
              });
            }, 1500);
          } else {
            failed.push([id, 'Bot config not found']);
          }
        }
        resolve({ succeeded, failed });
      }, 300);
    });
  },

  batchDeleteBots: async (botIds: string[]): Promise<BatchResultResponse> => {
    if (isTauri) {
      return await invoke<BatchResultResponse>('batch_delete_bots', { botIds });
    }
    return new Promise((resolve) => {
      setTimeout(() => {
        const succeeded: string[] = [];
        const failed: [string, string][] = [];
        for (const id of botIds) {
          const idx = mockSnapshots.findIndex(s => s.bot_id === id);
          if (idx !== -1) {
            mockSnapshots.splice(idx, 1);
            succeeded.push(id);
          } else {
            failed.push([id, 'Bot config not found']);
          }
        }
        resolve({ succeeded, failed });
      }, 300);
    });
  },

  activeBotCount: async (): Promise<number> => {
    if (isTauri) {
      return await invoke<number>('active_bot_count');
    }
    return mockSnapshots.filter(s => s.state === 'running' || s.state === 'starting' || s.state === 'stopping').length;
  },

  /// 拉取 Bot 最近 `lines` 行历史日志。
  /// BotLogPage 开页时一次调用，再叠加 `log_appended` 实时事件。
  tailBotLog: async (botId: string, lines: number = 1000): Promise<LogSnapshot> => {
    if (isTauri) {
      return await invoke<LogSnapshot>('tail_bot_log', { botId, lines });
    }
    // 浏览器预览模式：返回若干条占位日志。
    return new Promise((resolve) => {
      setTimeout(() => {
        resolve({
          lines: [
            '[mock] BotLogPage 处于浏览器预览模式',
            `[mock] bot_id=${botId} lines=${lines}`,
            '[mock] 真实日志会在 Tauri 应用内显示',
          ],
          total_lines: 3,
        });
      }, 100);
    });
  },

  deleteBotConfig: async (botId: string): Promise<void> => {
    if (isTauri) {
      return await invoke<void>('delete_bot_config', { botId });
    }
    return new Promise((resolve, reject) => {
      setTimeout(() => {
        const idx = mockSnapshots.findIndex(s => s.bot_id === botId);
        if (idx !== -1) {
          mockSnapshots.splice(idx, 1);
          resolve();
        } else {
          reject(`Bot not found: ${botId}`);
        }
      }, 200);
    });
  },

  upsertBotConfig: async (config: BotConfig): Promise<BotActorSnapshot> => {
    if (isTauri) {
      return await invoke<BotActorSnapshot>('upsert_bot_config', { config });
    }
    return new Promise((resolve) => {
      setTimeout(() => {
        const botId = String(config.bot.QQID);
        const existing = mockSnapshots.find(s => s.bot_id === botId);
        if (existing) {
          existing.revision += 1;
          existing.last_transition = 'Config updated';
          resolve({ ...existing });
        } else {
          const newSnap: BotActorSnapshot = {
            bot_id: botId,
            state: 'stopped',
            revision: 1,
            token_generation: 1,
            pending_restart: false,
            last_transition: 'Config created',
            last_error: undefined,
          };
          mockSnapshots.push(newSnap);
          resolve({ ...newSnap });
        }
      }, 200);
    });
  }
};
