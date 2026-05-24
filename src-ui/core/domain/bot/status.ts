// Bot 状态 → UI 派生（颜色/文案）。纯函数，无 React / Fluent 依赖。
//
// `BotActorState` 后端用 `#[serde(rename_all = "snake_case")]` 序列化，
// 所以前端拿到的是 `'running'` / `'starting'` 等小写值。

import type { BotActorState } from '../../ipc/generated/BotActorState';

export type BadgeColor =
    | 'brand'
    | 'danger'
    | 'important'
    | 'informative'
    | 'severe'
    | 'subtle'
    | 'success'
    | 'warning';

export interface BotStateBadge {
    /// Fluent UI Badge 的 `color` prop。`tiny` / `neutral` 不是合法值，但
    /// 历史 BotCard / StatusBadge 用过，做兼容保留。
    color: BadgeColor | 'tiny' | 'neutral';
    label: string;
}

/// 把 BotActorState 映射成 Badge 颜色 + 文案。
export function botStateBadge(state: BotActorState): BotStateBadge {
    switch (state) {
        case 'running':
            return { color: 'success', label: '运行中' };
        case 'starting':
            return { color: 'brand', label: '启动中' };
        case 'stopping':
            return { color: 'warning', label: '停止中' };
        case 'stopped':
            return { color: 'tiny', label: '已停止' };
        case 'crashed':
            return { color: 'danger', label: '崩溃' };
        case 'repairing':
            return { color: 'warning', label: '修复中' };
        default:
            return { color: 'neutral', label: String(state) };
    }
}

/// 谓词集合，用于 UI 控件 disabled / 图标切换。
export const isBotRunning = (s: BotActorState): boolean => s === 'running';
export const isBotStarting = (s: BotActorState): boolean => s === 'starting';
export const isBotStopping = (s: BotActorState): boolean => s === 'stopping';
export const isBotRepairing = (s: BotActorState): boolean => s === 'repairing';

/// "正在活跃"（被批量计数的那种）。
export function isBotActive(s: BotActorState): boolean {
    return s === 'running' || s === 'starting' || s === 'stopping';
}

/// 是否允许触发 Start（按钮 disabled 取反）。
export function canStartBot(s: BotActorState): boolean {
    return !(isBotStarting(s) || isBotRepairing(s));
}

/// 是否允许触发 Stop。
export function canStopBot(s: BotActorState): boolean {
    return !isBotStopping(s);
}
