// Bot 列表卡徽标语义：与组件卡同一视觉规则（soft Badge + dot 约定）。

import type { BotActorState } from '../../../../core/ipc/generated/BotActorState';
import { botStateBadge } from '../../../../core/domain/bot/status';

export type BotBadgeSpec = {
    tone: 'neutral' | 'brand' | 'success' | 'warning' | 'danger' | 'info';
    label: string;
    dot?: boolean;
};

function mapBotStateTone(
    color: ReturnType<typeof botStateBadge>['color'],
): BotBadgeSpec['tone'] {
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
        case 'tiny':
        case 'subtle':
        case 'neutral':
        default:
            return 'neutral';
    }
}

export function botLifecycleBadge(state: BotActorState): BotBadgeSpec {
    const { color, label } = botStateBadge(state);
    return {
        tone: mapBotStateTone(color),
        label,
        dot: state === 'running' || state === 'stopped',
    };
}

export function botAlertBadges(args: {
    pendingRestart: boolean;
    needsQrLogin: boolean;
}): BotBadgeSpec[] {
    const out: BotBadgeSpec[] = [];
    if (args.pendingRestart) {
        out.push({ tone: 'warning', label: '待重启', dot: true });
    }
    if (args.needsQrLogin) {
        out.push({ tone: 'warning', label: '待扫码', dot: true });
    }
    return out;
}