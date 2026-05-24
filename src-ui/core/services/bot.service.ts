// Bot 子领域 IPC 服务（生命周期 / 配置 / 日志 / QQ 进程枚举）。
// 集中所有 `*_bot` / `*_bot_config` / `tail_bot_log` / `list_qq_processes` 类
// Tauri command 的字面量。

import { invoke, isTauri } from '../ipc/transport';
import type { BatchResultResponse, BotActorSnapshot } from '../ipc/types';
import type { BotConfig } from '../ipc/generated/domain/BotConfig';
import type { BackendType } from '../ipc/generated/domain/BackendType';
import {
    buildMockBotConfig,
    mockBatchDelete,
    mockBatchStart,
    mockBatchStop,
    mockDeleteBotConfig,
    mockLogSnapshot,
    mockSnapshots,
    mockStartBot,
    mockStopBot,
    mockUpsertBotConfig,
} from '../ipc/mock/bot.mock';

/// 日志快照（与 Rust `LogSnapshot` 对齐）。
export interface LogSnapshot {
    lines: string[];
    total_lines: number;
}

/// QQ 进程信息（SnowLuma HotStart 模式下的 PID picker）。
export interface QQProcessInfo {
    pid: number;
    name: string;
    started_at: number;
    command_line: string;
}

export const botService = {
    // ── 快照 / 配置 ────────────────────────────────────────────────────────
    listSnapshots: async (): Promise<BotActorSnapshot[]> => {
        if (isTauri) return invoke<BotActorSnapshot[]>('list_bot_snapshots');
        return new Promise((resolve) =>
            setTimeout(() => resolve([...mockSnapshots]), 200),
        );
    },

    getSnapshot: async (botId: string): Promise<BotActorSnapshot> => {
        if (isTauri) return invoke<BotActorSnapshot>('get_bot_snapshot', { botId });
        return new Promise((resolve, reject) => {
            setTimeout(() => {
                const snap = mockSnapshots.find((s) => s.bot_id === botId);
                if (snap) resolve({ ...snap });
                else reject(new Error(`Bot not found: ${botId}`));
            }, 200);
        });
    },

    getConfig: async (botId: string): Promise<BotConfig | null> => {
        if (isTauri) return invoke<BotConfig | null>('get_bot_config', { botId });
        return new Promise((resolve) => {
            setTimeout(() => {
                const snap = mockSnapshots.find((s) => s.bot_id === botId);
                resolve(snap ? buildMockBotConfig(botId) : null);
            }, 200);
        });
    },

    /// 批量拉所有 Bot 的 backend_type，避免列表页 N+1。
    /// Tauri 端实装见 `commands::bot::list_bot_flavors`。
    listFlavors: async (): Promise<Record<string, BackendType>> => {
        if (isTauri) return invoke<Record<string, BackendType>>('list_bot_flavors');
        return new Promise((resolve) => {
            setTimeout(() => {
                const out: Record<string, BackendType> = {};
                for (const s of mockSnapshots) out[s.bot_id] = 'napcat';
                resolve(out);
            }, 100);
        });
    },

    // ── 生命周期 ──────────────────────────────────────────────────────────
    start: async (botId: string): Promise<BotActorSnapshot> => {
        if (isTauri) return invoke<BotActorSnapshot>('start_bot', { botId });
        return mockStartBot(botId);
    },

    stop: async (botId: string): Promise<BotActorSnapshot> => {
        if (isTauri) return invoke<BotActorSnapshot>('stop_bot', { botId });
        return mockStopBot(botId);
    },

    batchStart: async (botIds: string[]): Promise<BatchResultResponse> => {
        if (isTauri) return invoke<BatchResultResponse>('batch_start_bots', { botIds });
        return mockBatchStart(botIds);
    },

    batchStop: async (botIds: string[]): Promise<BatchResultResponse> => {
        if (isTauri) return invoke<BatchResultResponse>('batch_stop_bots', { botIds });
        return mockBatchStop(botIds);
    },

    batchDelete: async (botIds: string[]): Promise<BatchResultResponse> => {
        if (isTauri) return invoke<BatchResultResponse>('batch_delete_bots', { botIds });
        return mockBatchDelete(botIds);
    },

    activeCount: async (): Promise<number> => {
        if (isTauri) return invoke<number>('active_bot_count');
        return mockSnapshots.filter(
            (s) => s.state === 'running' || s.state === 'starting' || s.state === 'stopping',
        ).length;
    },

    upsertConfig: async (config: BotConfig): Promise<BotActorSnapshot> => {
        if (isTauri) return invoke<BotActorSnapshot>('upsert_bot_config', { config });
        return mockUpsertBotConfig(config);
    },

    deleteConfig: async (botId: string): Promise<void> => {
        if (isTauri) return invoke<void>('delete_bot_config', { botId });
        return mockDeleteBotConfig(botId);
    },

    // ── 日志快照 ─────────────────────────────────────────────────────────
    /// 拉取 Bot 最近 `lines` 行历史日志。BotLogPage 开页一次调用。
    tailLog: async (botId: string, lines = 1000): Promise<LogSnapshot> => {
        if (isTauri) return invoke<LogSnapshot>('tail_bot_log', { botId, lines });
        return new Promise((resolve) => setTimeout(() => resolve(mockLogSnapshot), 100));
    },

    // ── 系统进程枚举（QQ.exe）─────────────────────────────────────────────
    listQQProcesses: async (): Promise<QQProcessInfo[]> => {
        if (isTauri) return invoke<QQProcessInfo[]>('list_qq_processes');
        return Promise.resolve([
            { pid: 12345, name: 'QQ.exe', started_at: 0, command_line: '' },
            { pid: 23456, name: 'QQ.exe', started_at: 0, command_line: '' },
        ]);
    },
};
