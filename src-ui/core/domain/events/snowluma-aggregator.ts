// SnowLuma 状态聚合器。
//
// 后端事件：
//   snowluma_daemon_state_changed  → 全局 DaemonState
//   snowluma_bot_injected          → per-bot injected: bool
//   snowluma_uin_detected          → per-bot uin: string
//   snowluma_login_state_changed   → per-bot SnowLumaLoginState
//   snowluma_pid_set_changed       → per-bot pid 集合（当前 UI 不展示）
//   snowluma_daemon_log            → 日志行（由 BotLogPage 自己消费）
//
// 与 NapCat 不同的是 daemon 是单例，daemon Crashed 时所有 SL Bot 失效。

import type { DaemonState } from '../../ipc/generated/DaemonState';
import type { SnowLumaLoginState } from '../../ipc/generated/SnowLumaLoginState';
import type { DomainEvent } from '../../ipc/types';

export interface SnowlumaBotState {
    injected: boolean;
    uin: string | null;
    loginState: SnowLumaLoginState | null;
}

export interface SnowlumaState {
    daemonState: DaemonState | null;
    byBot: Record<string, SnowlumaBotState>;
}

export const initialSnowlumaState: SnowlumaState = {
    daemonState: null,
    byBot: {},
};

const emptyBot: SnowlumaBotState = { injected: false, uin: null, loginState: null };

function ensureBot(s: SnowlumaState, id: string): SnowlumaBotState {
    return s.byBot[id] ?? emptyBot;
}

export function reduceSnowluma(s: SnowlumaState, event: DomainEvent): SnowlumaState {
    switch (event.kind) {
        case 'snowluma_daemon_state_changed': {
            // daemon Crashed → 清空 per-bot 注入 / 登录态。
            if (event.state === 'crashed') {
                const cleared: Record<string, SnowlumaBotState> = {};
                for (const [id, prev] of Object.entries(s.byBot)) {
                    cleared[id] = { ...prev, injected: false, loginState: null };
                }
                return { daemonState: event.state, byBot: cleared };
            }
            return { ...s, daemonState: event.state };
        }

        case 'snowluma_bot_injected':
            return {
                ...s,
                byBot: {
                    ...s.byBot,
                    [event.bot_id]: { ...ensureBot(s, event.bot_id), injected: true },
                },
            };

        case 'snowluma_uin_detected':
            return {
                ...s,
                byBot: {
                    ...s.byBot,
                    [event.bot_id]: { ...ensureBot(s, event.bot_id), uin: event.uin },
                },
            };

        case 'snowluma_login_state_changed':
            return {
                ...s,
                byBot: {
                    ...s.byBot,
                    [event.bot_id]: { ...ensureBot(s, event.bot_id), loginState: event.state },
                },
            };

        // 暂不消费的事件直接放过。
        case 'snowluma_pid_set_changed':
        case 'snowluma_daemon_log':
        default:
            return s;
    }
}
