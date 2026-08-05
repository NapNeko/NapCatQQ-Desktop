// SnowLuma 状态聚合器。
//
// 后端事件：
//   snowluma_daemon_state_changed  → 按 local / server_id 隔离的 DaemonState
//   snowluma_bot_injected          → per-bot injected: bool
//   snowluma_uin_detected          → per-bot uin: string
//   snowluma_login_state_changed   → per-bot SnowLumaLoginState
//   snowluma_login_probe_unavailable → per-bot 登录态回到 unknown
//   snowluma_pid_set_changed       → per-bot pid 集合（当前 UI 不展示）
//   snowluma_daemon_log            → 日志行（由 BotLogPage 自己消费）
//
// daemon 只在单个运行主机内共享；本机与不同 server_id 之间互不影响。

import type { DaemonState } from '../../ipc/generated/DaemonState';
import type { SnowLumaLoginState } from '../../ipc/generated/SnowLumaLoginState';
import type { DomainEvent } from '../../ipc/types';

export interface SnowlumaBotState {
    injected: boolean;
    uin: string | null;
    loginState: SnowLumaLoginState | null;
    /** 远端 Docker 隧道就绪后可开 WebUI */
    dockerEndpointsReady: boolean;
}

export interface SnowlumaState {
    daemonStates: Record<string, DaemonState>;
    byBot: Record<string, SnowlumaBotState>;
}

export const initialSnowlumaState: SnowlumaState = {
    daemonStates: {},
    byBot: {},
};

const emptyBot: SnowlumaBotState = {
    injected: false,
    uin: null,
    loginState: null,
    dockerEndpointsReady: false,
};

function ensureBot(s: SnowlumaState, id: string): SnowlumaBotState {
    return s.byBot[id] ?? emptyBot;
}

function clearBot(s: SnowlumaState, botId: string): SnowlumaState {
    if (!s.byBot[botId]) return s;
    const byBot = { ...s.byBot };
    delete byBot[botId];
    return { ...s, byBot };
}

export function reduceSnowluma(s: SnowlumaState, event: DomainEvent): SnowlumaState {
    switch (event.kind) {
        case 'snowluma_daemon_state_changed': {
            const scope = event.server_id?.trim() || 'local';
            return {
                ...s,
                daemonStates: { ...s.daemonStates, [scope]: event.state },
            };
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

        case 'snowluma_docker_endpoints_ready':
            return {
                ...s,
                byBot: {
                    ...s.byBot,
                    [event.bot_id]: {
                        ...ensureBot(s, event.bot_id),
                        dockerEndpointsReady: true,
                    },
                },
            };

        case 'snowluma_login_probe_unavailable':
            return {
                ...s,
                byBot: {
                    ...s.byBot,
                    [event.bot_id]: { ...ensureBot(s, event.bot_id), loginState: null },
                },
            };

        case 'bot_process_exited':
            return clearBot(s, event.bot_id);

        case 'bot_state_changed':
            return event.snapshot.state === 'stopped' || event.snapshot.state === 'crashed'
                ? clearBot(s, event.snapshot.bot_id)
                : s;

        // 暂不消费的事件直接放过。
        case 'snowluma_pid_set_changed':
        case 'snowluma_daemon_log':
        default:
            return s;
    }
}
