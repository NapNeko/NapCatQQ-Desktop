import { describe, expect, it, vi } from 'vitest';
import { ChatAccountStore } from './chatStore';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import type { DebugCallResponse } from '../../core/ipc/generated/debug/DebugCallResponse';
import type { DebugSubscribeResponse } from '../../core/ipc/generated/debug/DebugSubscribeResponse';
import type { DebugEventBatch } from '../../core/ipc/generated/debug/DebugEventBatch';
const target: DebugTarget = { bot_id: 'bot', name: '测试', qq_id: 99, backend: 'napcat', host: { kind: 'local' }, running: true, online: true };
const subscription: DebugSubscribeResponse = { subscription_id: 'subscription', receiver: { bot_id: 'bot', state: { state: 'connected' }, source: { kind: 'internal' }, buffered: 0, first_seq: 1, viewers: 1, dropped_total: 0 } };
const ok = (data: unknown): DebugCallResponse => ({ request_id: 'req', result: { kind: 'ok', outcome: { ok: true, data, status: 'ok', retcode: 0, message: '', wording: '', raw: {}, channel: { kind: 'internal' }, elapsed_ms: 1, size_bytes: 0, truncated: false } } });
function setup() {
    const transport = { call: vi.fn(async () => ok({ message_id: 8 })), subscribe: vi.fn(async (_bot: string, _batch: (batch: DebugEventBatch) => void) => subscription), unsubscribe: vi.fn(async () => {}) };
    return { transport, store: new ChatAccountStore(target, transport) };
}
describe('chat lifecycle', () => {
    it('loads recent history once after connection, including an empty conversation', async () => {
        const { store, transport } = setup();
        transport.call.mockResolvedValue(ok({ messages: [] }));
        await store.ensureHistory('private:12');
        expect(transport.call).not.toHaveBeenCalled();
        await store.connect();
        await Promise.all([store.ensureHistory('private:12'), store.ensureHistory('private:12')]);
        await store.ensureHistory('private:12');
        expect(transport.call).toHaveBeenCalledTimes(1);
        expect(store.getSnapshot().history['private:12']).toMatchObject({ loaded: true, done: true });
    });
    it('waits for an explicit history retry after failure', async () => {
        const { store, transport } = setup(); await store.connect();
        transport.call.mockRejectedValueOnce(new Error('历史不可用'));
        await store.ensureHistory('group:12');
        await store.ensureHistory('group:12');
        expect(transport.call).toHaveBeenCalledTimes(1);
        expect(store.getSnapshot().history['group:12'].error).toBe('历史不可用');
        transport.call.mockResolvedValue(ok({ messages: [] }));
        await store.history('group:12');
        expect(store.getSnapshot().history['group:12']).toMatchObject({ loaded: true, error: '' });
    });
    it('can initialize history again after a connection interrupts the first read', async () => {
        const { store, transport } = setup(); await store.connect();
        let resolve!: (result: DebugCallResponse) => void;
        transport.call.mockImplementationOnce(() => new Promise(done => { resolve = done; }));
        const first = store.ensureHistory('private:12');
        await store.disconnect(); await store.connect();
        transport.call.mockResolvedValue(ok({ messages: [] }));
        await store.ensureHistory('private:12');
        resolve(ok({ messages: [{ message_id: 77, time: 1, user_id: 12, message: '过期响应' }] }));
        await first;
        expect(transport.call).toHaveBeenCalledTimes(2);
        expect(store.getSnapshot().account.messages).toHaveLength(0);
        expect(store.getSnapshot().history['private:12'].loaded).toBe(true);
    });
    it('releases a subscription that finishes after disconnect', async () => {
        const { store, transport } = setup();
        let resolve!: (result: DebugSubscribeResponse) => void;
        transport.subscribe.mockImplementation(() => new Promise(done => { resolve = done; }));
        const connect = store.connect(); await store.disconnect(); resolve(subscription); await connect;
        expect(transport.unsubscribe).toHaveBeenCalledWith('subscription');
        expect(store.getSnapshot().connection.state).toBe('stopped');
    });
    it('keeps edits made while an earlier message is sending', async () => {
        const { store, transport } = setup(); await store.connect();
        let resolve!: (result: DebugCallResponse) => void; transport.call.mockImplementation(() => new Promise(done => { resolve = done; }));
        store.draft('group:12', { text: '第一条', reply: null, attachments: [] });
        const pending = store.send('group:12');
        store.draft('group:12', { text: '第二条草稿', reply: null, attachments: [] });
        resolve(ok({ message_id: 8 })); await pending;
        expect(store.getSnapshot().account.drafts['group:12'].text).toBe('第二条草稿');
        expect(store.getSnapshot().account.messages[0].status).toBe('sent');
    });
    it('does not resend an unknown result', async () => {
        const { store, transport } = setup(); await store.connect();
        transport.call.mockResolvedValue({ request_id: 'req', result: { kind: 'err', error: { kind: 'timeout', ms: 30000 } } });
        store.draft('group:12', { text: '可能已经送达', reply: null, attachments: [] }); await store.send('group:12');
        expect(store.getSnapshot().account.messages[0].status).toBe('unknown');
        expect(transport.call).toHaveBeenCalledTimes(1);
    });
    it('marks in-flight sends unknown when the connection is explicitly released', async () => {
        const { store, transport } = setup(); await store.connect();
        let resolve!: (result: DebugCallResponse) => void; transport.call.mockImplementation(() => new Promise(done => { resolve = done; }));
        store.draft('group:12', { text: '离线前的消息', reply: null, attachments: [] });
        const pending = store.send('group:12'); await store.disconnect(); resolve(ok({ message_id: 8 })); await pending;
        expect(store.getSnapshot().account.messages[0].status).toBe('unknown');
    });
    it('passes pasted images as base64 rather than local file paths', async () => {
        const calls: unknown[][] = []; const { store, transport } = setup(); await store.connect();
        transport.call.mockImplementation(async (...args: unknown[]) => { calls.push(args); return ok({ message_id: 8 }); });
        store.draft('group:12', { text: '', reply: null, attachments: [{ key: 'image', name: '截图.png', path: 'base64://abc', type: 'image' }] });
        await store.send('group:12');
        expect(calls[0][2]).toMatchObject({ message: [{ type: 'image', data: { file: 'base64://abc' } }] });
    });
    it('releases interrupted sends on reconnect without releasing a newer send lock', async () => {
        const { store, transport } = setup(); await store.connect();
        let resolveOld!: (result: DebugCallResponse) => void;
        let resolveNew!: (result: DebugCallResponse) => void;
        transport.call.mockImplementationOnce(() => new Promise(done => { resolveOld = done; }));
        transport.call.mockImplementationOnce(() => new Promise(done => { resolveNew = done; }));
        store.draft('group:12', { text: '重连前', reply: null, attachments: [] });
        const oldSend = store.send('group:12');
        transport.subscribe.mock.calls[0][1]({ v: 1, bot_id: 'bot', events: [{ seq: 1, at_ms: 1, body: { kind: 'receiver', state: { state: 'stopped', reason: '连接中断' } } }] });
        await store.connect();
        expect(store.getSnapshot().account.messages[0].status).toBe('unknown');
        store.draft('group:12', { text: '重连后', reply: null, attachments: [] });
        const newSend = store.send('group:12');
        expect(transport.call).toHaveBeenCalledTimes(2);
        resolveOld(ok({ message_id: 8 })); await oldSend;
        store.draft('group:12', { text: '继续编辑', reply: null, attachments: [] });
        await store.send('group:12');
        expect(transport.call).toHaveBeenCalledTimes(2);
        resolveNew(ok({ message_id: 9 })); await newSend;
        expect(store.getSnapshot().account.messages.map(message => message.status)).toEqual(['unknown', 'sent']);
        expect(store.getSnapshot().account.drafts['group:12'].text).toBe('继续编辑');
    });
    it('retains attachments not submitted when the first send is interrupted', async () => {
        const { store, transport } = setup(); await store.connect();
        let resolve!: (result: DebugCallResponse) => void; transport.call.mockImplementation(() => new Promise(done => { resolve = done; }));
        store.draft('group:12', { text: '附件在这里', reply: null, attachments: [{ key: 'file', name: '说明.txt', path: 'D:/说明.txt', type: 'file' }] });
        const pending = store.send('group:12'); await store.disconnect(); resolve(ok({ message_id: 8 })); await pending;
        expect(transport.call).toHaveBeenCalledTimes(1);
        expect(store.getSnapshot().account.messages.some(message => message.status === 'failed' && message.segments[0].type === 'file')).toBe(true);
    });
    it('ignores events from another signed-in account', async () => {
        const { store, transport } = setup(); await store.connect();
        transport.subscribe.mock.calls[0][1]({ v: 1, bot_id: 'bot', events: [{ seq: 1, at_ms: 1, body: { kind: 'ob11', payload: { self_id: 100, message_type: 'group', group_id: 12, user_id: 13, message_id: 8, message: 'wrong account' } } }] });
        expect(store.getSnapshot().account.messages).toHaveLength(0);
    });
    it.each(['before', 'after'] as const)('merges a file echo arriving %s the upload response', async (order) => {
        const { store, transport } = setup(); store.target = { ...target, backend: 'snowluma' };
        await store.connect();
        let resolve!: (result: DebugCallResponse) => void;
        transport.call.mockImplementationOnce(() => new Promise(done => { resolve = done; }));
        store.draft('private:88', { text: '', reply: null, attachments: [{ key: 'file', name: 'inline.txt', path: 'D:/inline.txt', type: 'file' }] });
        const pending = store.send('private:88');
        const echo = () => transport.subscribe.mock.calls[0][1]({ v: 1, bot_id: 'bot', events: [{ seq: 1, at_ms: Date.now(), body: { kind: 'ob11', payload: { post_type: 'message_sent', message_type: 'private', self_id: 99, user_id: 99, target_id: 88, message_id: 8, time: Math.floor(Date.now() / 1000), message: [{ type: 'file', data: { file_id: 'uploaded-file-uuid', file: 'inline.txt', file_size: 3 } }] } } }] });
        if (order === 'before') echo();
        resolve(ok({ file_id: 'uploaded-file-uuid' })); await pending;
        if (order === 'after') echo();
        const messages = store.getSnapshot().account.messages;
        expect(messages).toHaveLength(1);
        expect(messages[0]).toMatchObject({ status: 'sent', id: '8', session: 'private:88' });
        expect(messages[0].segments[0].data.file).toBe('inline.txt');
    });
});
