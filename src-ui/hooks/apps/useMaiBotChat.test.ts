import { describe, expect, it } from 'vitest';
import type { MaiBotChatMessage } from '../../core/domain/apps/maibotChat';
import { chatReducer, type MaiBotChatState } from './useMaiBotChat';

const msg = (id: string, fromBot: boolean, text: string, at = 1): MaiBotChatMessage => ({
    id,
    fromBot,
    senderName: fromBot ? '麦麦' : '人类',
    segments: [{ type: 'text', text }],
    at,
});

const empty: MaiBotChatState = { status: 'ready', botName: '麦麦', items: [], typing: false, seq: 0 };

describe('chatReducer', () => {
    it('history replaces the list; later messages append, echoes with a known id are dropped', () => {
        let s = chatReducer(empty, { type: 'event', event: { kind: 'history', messages: [msg('a', false, '在吗'), msg('', true, '在')] } });
        expect(s.items.map((i) => i.key)).toEqual(['a', 'h1']);
        s = chatReducer(s, { type: 'event', event: { kind: 'message', message: msg('u2', false, '吃了吗') } });
        s = chatReducer(s, { type: 'event', event: { kind: 'message', message: msg('u2', false, '吃了吗') } });
        s = chatReducer(s, { type: 'event', event: { kind: 'message', message: msg('', true, '吃了') } });
        s = chatReducer(s, { type: 'event', event: { kind: 'message', message: msg('', true, '你呢') } });
        const keys = s.items.map((i) => i.key);
        expect(keys).toHaveLength(5);
        expect(new Set(keys).size).toBe(5);
        // 重连时整段重发：还是换掉，不叠两份
        s = chatReducer(s, { type: 'event', event: { kind: 'history', messages: [msg('a', false, '在吗')] } });
        expect(s.items).toHaveLength(1);
    });

    it('typing clears when the connection is not ready', () => {
        let s = chatReducer(empty, { type: 'event', event: { kind: 'typing', typing: true } });
        expect(s.typing).toBe(true);
        s = chatReducer(s, { type: 'status', status: 'reconnecting', reason: '连接断了' });
        expect(s).toMatchObject({ typing: false, status: 'reconnecting', reason: '连接断了' });
    });

    it('notices sit in the timeline; session info renames the bot; cleared empties the list', () => {
        let s = chatReducer(empty, { type: 'event', event: { kind: 'session', botName: '小麦', botQq: '123456', userName: '人类' } });
        s = chatReducer(s, { type: 'event', event: { kind: 'notice', text: '已连接', at: 1, error: false } });
        expect(s).toMatchObject({ botName: '小麦', botQq: '123456' });
        expect(s.items[0]).toMatchObject({ kind: 'notice', text: '已连接' });
        expect(chatReducer(s, { type: 'cleared' }).items).toEqual([]);
    });
});
