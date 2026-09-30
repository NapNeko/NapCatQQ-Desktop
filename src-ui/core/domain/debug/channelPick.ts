// 顶栏两个通道下拉（调用 / 事件）要的判断：哪条能选、为什么不能选、「自动」眼下落在哪条。

import type { DebugChannelId } from '../../ipc/generated/debug/DebugChannelId';
import type { DebugChannelInfo } from '../../ipc/generated/debug/DebugChannelInfo';
import type { DebugChannels } from '../../ipc/generated/debug/DebugChannels';
import { channelShortLabel } from './channelCopy';

export type ChannelPurpose = 'call' | 'events';

export const AUTO_CHANNEL: DebugChannelId = { kind: 'auto' };

export function sameChannel(a: DebugChannelId, b: DebugChannelId): boolean {
    if (a.kind !== b.kind) return false;
    if ((a.kind === 'http' || a.kind === 'ws') && (b.kind === 'http' || b.kind === 'ws')) return a.name === b.name;
    return true;
}

/**
 * 这条通道能不能被选来干这件事。
 * 状态不好（连不上、token 错）的照样能选：用户可能正要去修，选上再测一次最方便；
 * 只有「根本做不了」的才禁用。
 */
export function channelSelectable(info: DebugChannelInfo, purpose: ChannelPurpose): { ok: boolean; reason?: string } {
    if (info.status.kind === 'unsupported') return { ok: false, reason: info.status.reason };
    if (purpose === 'call' && !info.can_call) return { ok: false, reason: '这条通道只能收事件，不能发起调用' };
    if (purpose === 'events' && !info.can_receive) return { ok: false, reason: '这条通道只能调用，收不了事件' };
    return { ok: true };
}

/** 选的是「自动」时后端眼下会用哪条；选了具体通道就是它自己 */
export function effectiveChannelId(
    channels: DebugChannels | undefined,
    choice: DebugChannelId,
    purpose: ChannelPurpose,
): DebugChannelId | null {
    if (choice.kind !== 'auto') return choice;
    if (!channels) return null;
    return purpose === 'call' ? channels.auto_call : channels.auto_events;
}

export function findChannel(channels: DebugChannels | undefined, id: DebugChannelId | null): DebugChannelInfo | null {
    if (!channels || !id) return null;
    return channels.channels.find((c) => sameChannel(c.id, id)) ?? null;
}

/**
 * 下拉按钮上的字：`自动（内部通道）` / `自动（没有可用通道）` / 选中那条的短名。
 * `missing` 表示用户选过的通道已经不在列表里了（配置改了），按钮要提醒。
 */
export function channelTriggerLabel(
    channels: DebugChannels | undefined,
    choice: DebugChannelId,
    purpose: ChannelPurpose,
): { text: string; missing: boolean; none: boolean } {
    if (choice.kind === 'auto') {
        if (!channels) return { text: '自动', missing: false, none: false };
        const auto = purpose === 'call' ? channels.auto_call : channels.auto_events;
        return auto
            ? { text: `自动（${channelShortLabel(auto)}）`, missing: false, none: false }
            : { text: '自动（没有可用通道）', missing: false, none: true };
    }
    const missing = !!channels && !findChannel(channels, choice);
    return { text: channelShortLabel(choice), missing, none: false };
}

