// 通道状态和名字的展示文案，以及「去哪解决」的出口。

import type { AppRoute } from '../ui/route';
import type { DebugChannelId } from '../../ipc/generated/debug/DebugChannelId';
import type { DebugChannelStatus } from '../../ipc/generated/debug/DebugChannelStatus';

type Tone = 'success' | 'warning' | 'danger' | 'neutral';
interface StatusCopy {
    text: string;
    tone: Tone;
}

// 键是后端 DebugChannelStatus 的 kind，缺一种就编译不过
const STATUS_COPY = {
    unknown: () => ({ text: '未测试', tone: 'neutral' }),
    available: () => ({ text: '可用', tone: 'success' }),
    tunneled: (s) => ({ text: `已开隧道（本地端口 ${s.local_port}）`, tone: 'success' }),
    unreachable: (s) => ({ text: `连不上（${s.reason}）`, tone: 'danger' }),
    auth_failed: (s) => ({ text: `token 错误（${s.status}）`, tone: 'danger' }),
    // 「太老」只靠探测判断，不比版本号，文案统一告诉用户怎么办
    upstream_too_old: () => ({
        text: '上游版本太老，升级到最新版 NapCat / SnowLuma 后可用',
        tone: 'warning',
    }),
    bot_not_running: () => ({ text: 'Bot 未运行', tone: 'warning' }),
    not_logged_in: () => ({ text: 'QQ 未登录', tone: 'warning' }),
    unsupported: (s) => ({ text: `不支持（${s.reason}）`, tone: 'neutral' }),
} satisfies {
    [K in DebugChannelStatus['kind']]: (s: Extract<DebugChannelStatus, { kind: K }>) => StatusCopy;
};

export function channelStatusCopy(s: DebugChannelStatus): { text: string; tone: Tone } {
    const build = STATUS_COPY[s.kind] as (status: DebugChannelStatus) => StatusCopy;
    return build(s);
}

/** 错误卡片或通道下拉里「去能解决的页面」的出口；页面没给 onNavigate 时按钮不画 */
export interface ChannelExit {
    route: AppRoute;
    label: string;
}

/** 「全都不可用」时的两个出口：组件页装 / 修对应运行时，机器人配置里开一条 WS 服务（规格 §3.2） */
export const NO_CHANNEL_EXITS: ChannelExit[] = [
    { route: 'components', label: '去「组件」页装/修运行时' },
    { route: 'bots', label: '去「机器人」页开 WS 服务' },
];

/** 「上游版本太老」的出口：组件页里升级 NapCat / SnowLuma */
export const UPGRADE_RUNTIME_EXIT: ChannelExit = { route: 'components', label: '去「组件」页升级' };

/** 顶栏和历史里用的短名 */
export function channelShortLabel(id: DebugChannelId): string {
    switch (id.kind) {
        case 'auto':
            return '自动';
        case 'internal':
            return '内部通道';
        case 'http':
            return `HTTP · ${id.name}`;
        case 'ws':
            return `WS · ${id.name}`;
        default: {
            const _exhaustive: never = id;
            return _exhaustive;
        }
    }
}
