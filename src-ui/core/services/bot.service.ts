// Bot 子领域 IPC 服务（生命周期 / 配置 / 日志 / QQ 进程枚举 / QQ 登录探测）。
// 集中所有 `*_bot` / `*_bot_config` / `tail_bot_log` / `list_qq_processes`
// / `probe_qq_login_info` 类 Tauri command 的字面量。

import { invoke, isTauri } from '../ipc/transport';
import type { BatchResultResponse, BotActorSnapshot } from '../ipc/types';
import type { BotConfig } from '../ipc/generated/domain/BotConfig';
import type { BackendType } from '../ipc/generated/domain/BackendType';
import type { QqLoginInfo } from '../ipc/generated/QqLoginInfo';
import type { ConfigDrift } from '../ipc/generated/ConfigDrift';
import type { DriftDecision } from '../ipc/generated/DriftDecision';

export interface SnowLumaAgreementDoc {
    id: string;
    title: string;
    declared_version: string;
    text: string;
}

export interface SnowLumaAgreementsPayload {
    version: string;
    consent_required: boolean;
    documents: SnowLumaAgreementDoc[];
}
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

    /** 内存中的 NapCat WebUI 端点(多实例 port 可能 +1);冷启动 hydrate 用 */
    listNapcatWebuiBindings: async (): Promise<
        Array<{ bot_id: string; port: number; token: string }>
    > => {
        if (isTauri) {
            return invoke('list_napcat_webui_bindings');
        }
        return [];
    },

    /** SnowLuma daemon + per-bot 登录/隧道;冷启动 hydrate 用 */
    listSnowlumaUiSnapshot: async (): Promise<{
        daemon_state: import('../ipc/generated/DaemonState').DaemonState | null;
        bots: Array<{
            bot_id: string;
            injected: boolean;
            uin: string | null;
            login_state: import('../ipc/generated/SnowLumaLoginState').SnowLumaLoginState | null;
            endpoints_ready: boolean;
        }>;
    }> => {
        if (isTauri) {
            return invoke('list_snowluma_ui_snapshot');
        }
        return { daemon_state: null, bots: [] };
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

    /// 带用户决议保存配置。前端保存时如果检测到 drift 并确认了决议后调此命令。
    upsertConfigWithDecisions: async (
        config: BotConfig,
        decisions: DriftDecision[],
    ): Promise<BotActorSnapshot> => {
        if (isTauri) return invoke<BotActorSnapshot>('upsert_bot_config_with_decisions', { config, decisions });
        return mockUpsertBotConfig(config);
    },

    // ── Config drift 检测 ────────────────────────────────────────────────
    /// 启动前检测派生配置文件是否被外部修改。返回 null 表示无差异可直接启动。
    detectConfigDrift: async (botId: string): Promise<ConfigDrift | null> => {
        if (isTauri) return invoke<ConfigDrift | null>('detect_bot_config_drift', { botId });
        // 浏览器 mock：无 drift
        return null;
    },

    /// 带用户决议启动 Bot。前端在 ConfigDriftDialog 确认后调此命令。
    startWithDecisions: async (
        botId: string,
        decisions: DriftDecision[],
    ): Promise<BotActorSnapshot> => {
        if (isTauri) return invoke<BotActorSnapshot>('start_bot_with_drift_decisions', { botId, decisions });
        return mockStartBot(botId);
    },

    // ── 系统进程枚举（QQ.exe）─────────────────────────────────────────────
    listQQProcesses: async (): Promise<QQProcessInfo[]> => {
        if (isTauri) return invoke<QQProcessInfo[]>('list_qq_processes');
        return Promise.resolve([
            { pid: 12345, name: 'QQ.exe', started_at: 0, command_line: '' },
            { pid: 23456, name: 'QQ.exe', started_at: 0, command_line: '' },
        ]);
    },

    /// 探测指定 PID 当前登录的 QQ 账号（HOT 模式 PID picker 用）。
    /// 走 QQ NT 自带的 9210-9219 tencent:// HTTP 端点，response 里夹带的 JWT
    /// payload 包含 uin / nickName。`null` = 端口全部不响应或当前未登录。
    /// 实测每个 PID 命中通常 < 200ms，全端口扫超时上限 10s。
    probeQQLoginInfo: async (pid: number): Promise<QqLoginInfo | null> => {
        if (isTauri) return invoke<QqLoginInfo | null>('probe_qq_login_info', { pid });
        // 浏览器 mock：偶数 PID 当作已登录，构造一个伪 uin。
        return new Promise((resolve) =>
            setTimeout(() => {
                if (pid % 2 === 0) {
                    resolve({
                        port: 9210,
                        uin: String(10000 + pid),
                        uid: '',
                        nickname: `Mock-${pid}`,
                        logged_in: true,
                    });
                } else {
                    resolve(null);
                }
            }, 80),
        );
    },

    getSnowLumaAgreements: async (botId: string): Promise<SnowLumaAgreementsPayload> => {
        if (isTauri) return invoke<SnowLumaAgreementsPayload>('get_snowluma_agreements', { botId });
        return {
            version: 'mock',
            consent_required: true,
            documents: [
                {
                    id: 'eula',
                    title: 'SnowLuma 用户协议',
                    declared_version: 'mock',
                    text: '浏览器预览模式下的 SnowLuma 用户协议占位文本。',
                },
                {
                    id: 'privacy',
                    title: 'SnowLuma 隐私政策',
                    declared_version: 'mock',
                    text: '浏览器预览模式下的 SnowLuma 隐私政策占位文本。',
                },
            ],
        };
    },

    acceptSnowLumaAgreements: async (botId: string, version: string): Promise<void> => {
        if (isTauri) return invoke<void>('accept_snowluma_agreements', { botId, version });
    },

    prepareSnowLumaAgreements: async (botId: string): Promise<SnowLumaAgreementsPayload | null> => {
        if (isTauri) return invoke<SnowLumaAgreementsPayload | null>('prepare_snowluma_agreements', { botId });
        return null;
    },

    releaseSnowLumaAgreementSession: async (botId: string): Promise<void> => {
        if (isTauri) return invoke<void>('release_snowluma_agreement_session', { botId });
    },
};
