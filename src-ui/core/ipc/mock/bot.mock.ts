// 浏览器预览模式下的 Bot 假数据库 + 假状态机。
// 真 IPC 实装在 `core/services/bot.service.ts` 等文件。

import type {
    BatchResultResponse,
    BotActorSnapshot,
    BotStatus,
} from '../types';
import type { BotConfig } from '../generated/domain/BotConfig';
import { emitMockEvent } from './events.mock';

export const mockBots: BotStatus[] = [
    {
        bot_id: '10001',
        state: 'running',
        pid: 14052,
        started_at: Math.floor((Date.now() - 7200000) / 1000),
        memory_rss_bytes: 124518400,
        server_total_memory_bytes: 17179869184,
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
        state: 'stopped',
        pid: null,
        started_at: null,
        memory_rss_bytes: null,
        server_total_memory_bytes: 17179869184,
        backend_kind: 'local',
        runtime_target: 'local',
        extra: {},
    },
];

export const mockSnapshots: BotActorSnapshot[] = [
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

/// 一个标准的 mock BotConfig（用于浏览器预览）。
export function buildMockBotConfig(botId: string): BotConfig {
    return {
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
                },
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
    };
}

/// 模拟一次状态机迁移：先把 snapshot 切到 transitional，1.5s 后再到 final。
function scheduleStateTransition(
    snap: BotActorSnapshot,
    transitional: BotActorSnapshot['state'],
    finalState: BotActorSnapshot['state'],
    transitionalTransition: string,
    finalTransition: string,
    startReason: string,
    endReason: string,
): void {
    snap.state = transitional;
    snap.revision += 1;
    snap.last_transition = transitionalTransition;
    emitMockEvent({
        kind: 'bot_state_changed',
        snapshot: { ...snap },
        reason: startReason,
    });

    setTimeout(() => {
        snap.state = finalState;
        snap.revision += 1;
        snap.last_transition = finalTransition;
        emitMockEvent({
            kind: 'bot_state_changed',
            snapshot: { ...snap },
            reason: endReason,
        });
    }, 1500);
}

export async function mockStartBot(botId: string): Promise<BotActorSnapshot> {
    const snap = mockSnapshots.find((s) => s.bot_id === botId);
    if (!snap) throw new Error(`Bot not found: ${botId}`);
    scheduleStateTransition(
        snap,
        'starting',
        'running',
        'Manual starting',
        'Start SUCCESS',
        'manual_start',
        'startup_complete',
    );
    return new Promise((resolve) => setTimeout(() => resolve({ ...snap }), 200));
}

export async function mockStopBot(botId: string): Promise<BotActorSnapshot> {
    const snap = mockSnapshots.find((s) => s.bot_id === botId);
    if (!snap) throw new Error(`Bot not found: ${botId}`);
    scheduleStateTransition(
        snap,
        'stopping',
        'stopped',
        'Manual stopping',
        'Stopped gracefully',
        'manual_stop',
        'shutdown_complete',
    );
    return new Promise((resolve) => setTimeout(() => resolve({ ...snap }), 200));
}

export async function mockBatchStart(botIds: string[]): Promise<BatchResultResponse> {
    const succeeded: string[] = [];
    const failed: [string, string][] = [];
    for (const id of botIds) {
        const snap = mockSnapshots.find((s) => s.bot_id === id);
        if (!snap) {
            failed.push([id, 'Bot config not found']);
            continue;
        }
        scheduleStateTransition(
            snap,
            'starting',
            'running',
            'Batch manual starting',
            'Start SUCCESS',
            'batch_start',
            'startup_complete',
        );
        succeeded.push(id);
    }
    return new Promise((resolve) =>
        setTimeout(() => resolve({ succeeded, failed }), 300),
    );
}

export async function mockBatchStop(botIds: string[]): Promise<BatchResultResponse> {
    const succeeded: string[] = [];
    const failed: [string, string][] = [];
    for (const id of botIds) {
        const snap = mockSnapshots.find((s) => s.bot_id === id);
        if (!snap) {
            failed.push([id, 'Bot config not found']);
            continue;
        }
        scheduleStateTransition(
            snap,
            'stopping',
            'stopped',
            'Batch manual stopping',
            'Stopped gracefully',
            'batch_stop',
            'shutdown_complete',
        );
        succeeded.push(id);
    }
    return new Promise((resolve) =>
        setTimeout(() => resolve({ succeeded, failed }), 300),
    );
}

export async function mockBatchDelete(botIds: string[]): Promise<BatchResultResponse> {
    const succeeded: string[] = [];
    const failed: [string, string][] = [];
    for (const id of botIds) {
        const idx = mockSnapshots.findIndex((s) => s.bot_id === id);
        if (idx === -1) {
            failed.push([id, 'Bot config not found']);
            continue;
        }
        mockSnapshots.splice(idx, 1);
        succeeded.push(id);
    }
    return new Promise((resolve) =>
        setTimeout(() => resolve({ succeeded, failed }), 300),
    );
}

export async function mockUpsertBotConfig(config: BotConfig): Promise<BotActorSnapshot> {
    const botId = String(config.bot.QQID);
    const existing = mockSnapshots.find((s) => s.bot_id === botId);
    if (existing) {
        existing.revision += 1;
        existing.last_transition = 'Config updated';
        return new Promise((resolve) =>
            setTimeout(() => resolve({ ...existing }), 200),
        );
    }
    const created: BotActorSnapshot = {
        bot_id: botId,
        state: 'stopped',
        revision: 1,
        token_generation: 1,
        pending_restart: false,
        last_transition: 'Config created',
        last_error: undefined,
    };
    mockSnapshots.push(created);
    return new Promise((resolve) =>
        setTimeout(() => resolve({ ...created }), 200),
    );
}

export async function mockDeleteBotConfig(botId: string): Promise<void> {
    const idx = mockSnapshots.findIndex((s) => s.bot_id === botId);
    if (idx === -1) throw new Error(`Bot not found: ${botId}`);
    mockSnapshots.splice(idx, 1);
    return new Promise((resolve) => setTimeout(() => resolve(), 200));
}

export const mockLogSnapshot = {
    lines: [
        '[mock] BotLogPage 处于浏览器预览模式',
        '[mock] 真实日志会在 Tauri 应用内显示',
    ],
    total_lines: 2,
};
