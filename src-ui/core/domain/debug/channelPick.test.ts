import { describe, expect, it } from 'vitest';
import type { DebugChannelInfo } from '../../ipc/generated/debug/DebugChannelInfo';
import type { DebugChannels } from '../../ipc/generated/debug/DebugChannels';
import { channelSelectable, channelTriggerLabel, effectiveChannelId, findChannel, sameChannel } from './channelPick';

function info(patch: Partial<DebugChannelInfo>): DebugChannelInfo {
    return {
        id: { kind: 'internal' },
        label: '内部',
        can_call: true,
        can_receive: true,
        status: { kind: 'available' },
        endpoint: null,
        token_hint: null,
        ...patch,
    };
}

const channels: DebugChannels = {
    bot_id: 'b',
    channels: [
        info({}),
        info({ id: { kind: 'http', name: 'h' }, label: 'HTTP', can_receive: false }),
        info({ id: { kind: 'ws', name: 'w' }, label: 'WS', status: { kind: 'unsupported', reason: '容器没有映射 8080 端口' } }),
    ],
    auto_call: { kind: 'internal' },
    auto_events: null,
};

describe('channelSelectable', () => {
    it('不支持的、干不了这件事的禁用并给原因；状态不好的照样能选', () => {
        expect(channelSelectable(channels.channels[2], 'call')).toEqual({ ok: false, reason: '容器没有映射 8080 端口' });
        expect(channelSelectable(channels.channels[1], 'events').ok).toBe(false);
        expect(channelSelectable(channels.channels[1], 'call').ok).toBe(true);
        expect(channelSelectable(info({ status: { kind: 'auth_failed', status: 401 } }), 'call').ok).toBe(true);
    });
});

describe('effectiveChannelId / findChannel', () => {
    it('自动时看后端算好的那条', () => {
        expect(effectiveChannelId(channels, { kind: 'auto' }, 'call')).toEqual({ kind: 'internal' });
        expect(effectiveChannelId(channels, { kind: 'auto' }, 'events')).toBeNull();
        expect(effectiveChannelId(undefined, { kind: 'auto' }, 'call')).toBeNull();
        expect(effectiveChannelId(channels, { kind: 'http', name: 'h' }, 'events')).toEqual({ kind: 'http', name: 'h' });
    });

    it('按 kind + name 找', () => {
        expect(findChannel(channels, { kind: 'http', name: 'h' })?.label).toBe('HTTP');
        expect(findChannel(channels, { kind: 'http', name: 'x' })).toBeNull();
        expect(sameChannel({ kind: 'ws', name: 'a' }, { kind: 'http', name: 'a' })).toBe(false);
    });
});

describe('channelTriggerLabel', () => {
    it('自动时带上眼下落在哪条', () => {
        expect(channelTriggerLabel(channels, { kind: 'auto' }, 'call')).toEqual({ text: '自动（内部通道）', missing: false, none: false });
        expect(channelTriggerLabel(channels, { kind: 'auto' }, 'events')).toEqual({ text: '自动（没有可用通道）', missing: false, none: true });
        expect(channelTriggerLabel(undefined, { kind: 'auto' }, 'call').text).toBe('自动');
    });

    it('选过的通道不在了要标出来', () => {
        expect(channelTriggerLabel(channels, { kind: 'ws', name: 'gone' }, 'call')).toEqual({ text: 'WS · gone', missing: true, none: false });
        expect(channelTriggerLabel(channels, { kind: 'http', name: 'h' }, 'call').missing).toBe(false);
    });
});
