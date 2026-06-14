// Bot 实例在 UI 上的状态展示规范（列表卡 / 概览 / 配置页共用文案来源）。
//
// 分层：
//   1. 进程 — BotActor 生命周期（启停、崩溃、修复）
//   2. 账号 — NC/SL 统一的 QQ 登录会话语义（与 flavor 无关的对外词汇）
//   3. 告警 — 与进程并行的运维标记（待重启），不用长字符串拼进主徽章

import type { BotActorState } from '../../ipc/generated/BotActorState';
import type { SnowLumaLoginState } from '../../ipc/generated/SnowLumaLoginState';
import type { DaemonState } from '../../ipc/generated/DaemonState';
import type { Flavor } from './flavor';
import { botStateBadge } from './status';

export type StatusTone =
    | 'neutral'
    | 'brand'
    | 'success'
    | 'warning'
    | 'danger'
    | 'info';

export type StatusBadgeSpec = {
    tone: StatusTone;
    label: string;
    dot?: boolean;
};

export type BotListCardStatus = {
    lifecycle: StatusBadgeSpec;
    session: StatusBadgeSpec | null;
    alert: StatusBadgeSpec | null;
};

function mapLegacyBotColor(
    color: ReturnType<typeof botStateBadge>['color'],
): StatusTone {
    switch (color) {
        case 'success':
            return 'success';
        case 'warning':
            return 'warning';
        case 'danger':
        case 'severe':
        case 'important':
            return 'danger';
        case 'brand':
            return 'brand';
        case 'informative':
            return 'info';
        default:
            return 'neutral';
    }
}

/** 进程生命周期徽章（底栏左 1，始终存在）。 */
export function botProcessBadge(state: BotActorState): StatusBadgeSpec {
    const { color, label } = botStateBadge(state);
    return {
        tone: mapLegacyBotColor(color),
        label,
        dot: state === 'running' || state === 'stopped',
    };
}

const SESSION_LABELS = {
    loggedIn: 'QQ 已登录',
    notLoggedIn: 'QQ 未登录',
    needQr: '待扫码',
    preparing: '登录准备',
    probing: '探测登录',
    disconnected: 'QQ 已掉线',
} as const;

function napcatSessionBadge(args: {
    state: BotActorState;
    isOnline: boolean | null | undefined;
    needsQrLogin: boolean;
}): StatusBadgeSpec | null {
    if (args.state !== 'running' && args.state !== 'starting') {
        return null;
    }
    if (args.needsQrLogin) {
        return { tone: 'warning', label: SESSION_LABELS.needQr, dot: true };
    }
    if (args.state === 'running') {
        if (args.isOnline === true) {
            return { tone: 'success', label: SESSION_LABELS.loggedIn, dot: true };
        }
        if (args.isOnline === false) {
            return { tone: 'warning', label: SESSION_LABELS.notLoggedIn };
        }
        return { tone: 'info', label: SESSION_LABELS.probing };
    }
    return { tone: 'brand', label: SESSION_LABELS.preparing };
}

function snowlumaSessionBadge(args: {
    state: BotActorState;
    loginState: SnowLumaLoginState | null | undefined;
    needsQrLogin: boolean;
    daemonState: DaemonState | null | undefined;
}): StatusBadgeSpec | null {
    if (args.daemonState === 'crashed') {
        return { tone: 'danger', label: 'Daemon 崩溃' };
    }
    if (args.daemonState === 'starting' || args.daemonState === 'stopping') {
        return { tone: 'brand', label: SESSION_LABELS.preparing };
    }
    if (args.state !== 'running' && args.state !== 'starting') {
        return null;
    }
    if (args.needsQrLogin) {
        return { tone: 'warning', label: SESSION_LABELS.needQr, dot: true };
    }
    const ls = args.loginState;
    if (!ls) {
        if (args.state === 'running') {
            return { tone: 'info', label: SESSION_LABELS.probing };
        }
        return { tone: 'brand', label: SESSION_LABELS.preparing };
    }
    switch (ls) {
        case 'logged_in':
            return { tone: 'success', label: SESSION_LABELS.loggedIn, dot: true };
        case 'waiting_for_qr_scan':
            return { tone: 'warning', label: SESSION_LABELS.needQr, dot: true };
        case 'disconnected':
            return { tone: 'warning', label: SESSION_LABELS.disconnected };
        case 'starting':
            return {
                tone: args.state === 'running' ? 'brand' : 'brand',
                label: SESSION_LABELS.preparing,
            };
        default:
            return null;
    }
}

/** 账号会话徽章（底栏左 2，NC/SL 同词汇表；进程未活跃时不展示）。 */
export function botSessionBadge(args: {
    flavor: Flavor | null;
    state: BotActorState;
    needsQrLogin: boolean;
    isOnline?: boolean | null;
    snowlumaLoginState?: SnowLumaLoginState | null;
    snowlumaDaemonState?: DaemonState | null;
}): StatusBadgeSpec | null {
    if (args.flavor === 'napcat') {
        return napcatSessionBadge({
            state: args.state,
            isOnline: args.isOnline,
            needsQrLogin: args.needsQrLogin,
        });
    }
    if (args.flavor === 'snowluma') {
        return snowlumaSessionBadge({
            state: args.state,
            loginState: args.snowlumaLoginState,
            needsQrLogin: args.needsQrLogin,
            daemonState: args.snowlumaDaemonState,
        });
    }
    if (args.needsQrLogin) {
        return { tone: 'warning', label: SESSION_LABELS.needQr, dot: true };
    }
    return null;
}

function botAlertBadge(pendingRestart: boolean): StatusBadgeSpec | null {
    if (!pendingRestart) return null;
    return { tone: 'warning', label: '待重启', dot: true };
}

/** 列表卡底栏状态组：最多 3 枚短徽章，禁止拼接长句。 */
export function buildBotListCardStatus(args: {
    state: BotActorState;
    flavor: Flavor | null;
    pendingRestart: boolean;
    needsQrLogin: boolean;
    isOnline?: boolean | null;
    snowlumaLoginState?: SnowLumaLoginState | null;
    snowlumaDaemonState?: DaemonState | null;
}): BotListCardStatus {
    return {
        lifecycle: botProcessBadge(args.state),
        session: botSessionBadge({
            flavor: args.flavor,
            state: args.state,
            needsQrLogin: args.needsQrLogin,
            isOnline: args.isOnline,
            snowlumaLoginState: args.snowlumaLoginState,
            snowlumaDaemonState: args.snowlumaDaemonState,
        }),
        alert: botAlertBadge(args.pendingRestart),
    };
}

/** meta 副标题：只放账号 UIN 等补充信息，不重复底栏会话文案。 */
export function botListCardMetaLine(args: {
    flavor: Flavor | null;
    state: BotActorState;
    snowlumaLoginState: SnowLumaLoginState | null | undefined;
    snowlumaUin: string | null | undefined;
}): string | null {
    if (args.flavor !== 'snowluma') return null;
    if (args.state !== 'running') return null;
    if (args.snowlumaLoginState !== 'logged_in') return null;
    const uin = args.snowlumaUin?.trim();
    if (!uin) return null;
    return `UIN ${uin}`;
}