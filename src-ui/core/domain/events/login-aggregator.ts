// NapCat 登录态聚合器。
//
// 后端会广播 5 类事件：
//   - napcat_webui_available     → (port, token)
//   - napcat_login_qrcode        → qrcode_url（待扫码）
//   - napcat_login_qrcode_removed→ qrcode 已扫除
//   - napcat_login_online        → online: bool
//   - napcat_login_probe_unavailable → 连续探测失败，在线态回到 unknown
//   - napcat_login_invalidated   → reason: kicked | logged_out
//
// 这里把它们 reducer 化，输入是事件，输出是稳定的 `NapcatLoginState`。
// React hook 只负责把它们绑到 useReducer / useState 上。

import type { DomainEvent, NapCatLoginInvalidationReason } from '../../ipc/types';

export interface NapcatBotLogin {
    webui: { port: number; token: string } | null;
    qrcodeUrl: string | null;
    online: boolean | null;
    invalidationReason: NapCatLoginInvalidationReason | null;
}

export interface NapcatLoginState {
    byBot: Record<string, NapcatBotLogin>;
    /// 每次发生 `napcat_login_invalidated` 都会得到一个新的 epoch，UI 可以
    /// 据此触发 3s 自动隐藏定时器；epoch 一致就不重复重启定时器。
    invalidationEpoch: Record<string, number>;
}

export const initialNapcatLoginState: NapcatLoginState = {
    byBot: {},
    invalidationEpoch: {},
};

const emptyBotLogin: NapcatBotLogin = {
    webui: null,
    qrcodeUrl: null,
    online: null,
    invalidationReason: null,
};

function ensureBot(state: NapcatLoginState, botId: string): NapcatBotLogin {
    return state.byBot[botId] ?? emptyBotLogin;
}

export function reduceNapcatLogin(
    state: NapcatLoginState,
    event: DomainEvent,
): NapcatLoginState {
    switch (event.kind) {
        case 'napcat_webui_available': {
            const previous = ensureBot(state, event.bot_id);
            const sameBinding =
                previous.webui?.port === event.port &&
                previous.webui.token === event.token;
            return {
                ...state,
                byBot: {
                    ...state.byBot,
                    [event.bot_id]: {
                        ...previous,
                        webui: { port: event.port, token: event.token },
                        online: sameBinding ? previous.online : null,
                        qrcodeUrl: sameBinding ? previous.qrcodeUrl : null,
                        invalidationReason: sameBinding
                            ? previous.invalidationReason
                            : null,
                    },
                },
            };
        }

        case 'bot_process_exited': {
            const prev = state.byBot[event.bot_id];
            if (!prev) return state;
            if (prev.webui === null && prev.qrcodeUrl === null && prev.online === null) {
                return state;
            }
            return {
                ...state,
                byBot: {
                    ...state.byBot,
                    [event.bot_id]: {
                        ...prev,
                        webui: null,
                        qrcodeUrl: null,
                        online: null,
                    },
                },
            };
        }

        case 'bot_state_changed': {
            // stop/crash 后清 binding,避免按钮仍点到死端口
            // webui_tunnel_unreachable:隧道失效但 Bot 仍 Running,同样要灭灯
            const st = event.snapshot.state;
            const reason = event.reason ?? '';
            const tunnelDead = reason === 'webui_tunnel_unreachable';
            if (!tunnelDead && st !== 'stopped' && st !== 'crashed') return state;
            const botId = event.snapshot.bot_id;
            const cur = state.byBot[botId];
            if (!cur) return state;
            if (cur.webui === null && cur.qrcodeUrl === null && cur.online === null) {
                return state;
            }
            return {
                ...state,
                byBot: {
                    ...state.byBot,
                    [botId]: {
                        ...cur,
                        webui: null,
                        qrcodeUrl: null,
                        online: null,
                    },
                },
            };
        }

        case 'napcat_login_qrcode':
            return {
                ...state,
                byBot: {
                    ...state.byBot,
                    [event.bot_id]: {
                        ...ensureBot(state, event.bot_id),
                        qrcodeUrl: event.qrcode_url,
                    },
                },
            };

        case 'napcat_login_qrcode_removed': {
            const prev = state.byBot[event.bot_id];
            if (!prev || prev.qrcodeUrl === null) return state;
            return {
                ...state,
                byBot: {
                    ...state.byBot,
                    [event.bot_id]: { ...prev, qrcodeUrl: null },
                },
            };
        }

        case 'napcat_login_online':
            return {
                ...state,
                byBot: {
                    ...state.byBot,
                    [event.bot_id]: {
                        ...ensureBot(state, event.bot_id),
                        online: event.online,
                    },
                },
            };

        case 'napcat_login_probe_unavailable':
            return {
                ...state,
                byBot: {
                    ...state.byBot,
                    [event.bot_id]: {
                        ...ensureBot(state, event.bot_id),
                        online: null,
                    },
                },
            };

        case 'napcat_login_invalidated': {
            const epoch = (state.invalidationEpoch[event.bot_id] ?? 0) + 1;
            return {
                ...state,
                byBot: {
                    ...state.byBot,
                    [event.bot_id]: {
                        ...ensureBot(state, event.bot_id),
                        invalidationReason: event.reason,
                    },
                },
                invalidationEpoch: {
                    ...state.invalidationEpoch,
                    [event.bot_id]: epoch,
                },
            };
        }

        default:
            return state;
    }
}

/// 清掉 invalidationReason（3s 定时器到期后调用）。
export function clearInvalidation(
    state: NapcatLoginState,
    botId: string,
): NapcatLoginState {
    const prev = state.byBot[botId];
    if (!prev || prev.invalidationReason === null) return state;
    return {
        ...state,
        byBot: {
            ...state.byBot,
            [botId]: { ...prev, invalidationReason: null },
        },
    };
}
