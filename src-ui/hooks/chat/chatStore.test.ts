import { describe, expect, it, vi } from 'vitest';
import { ChatAccountStore } from './chatStore';
import { archiveOf } from '../../core/domain/chat/archive';
import { recoverDraft } from '../../core/domain/chat/recoverDraft';
import { inlineImageService } from '../../core/services/inline-image.service';
import {
    emptyAccount,
    ingestMessage,
    type Contact,
    type Message,
} from '../../core/domain/chat/model';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import type { DebugCallResponse } from '../../core/ipc/generated/debug/DebugCallResponse';
import type { DebugSubscribeResponse } from '../../core/ipc/generated/debug/DebugSubscribeResponse';
import type { DebugEventBatch } from '../../core/ipc/generated/debug/DebugEventBatch';
const target: DebugTarget = {
    bot_id: 'bot',
    name: '测试',
    qq_id: 99,
    backend: 'napcat',
    host: { kind: 'local' },
    running: true,
    online: true,
};
const subscription: DebugSubscribeResponse = {
    subscription_id: 'subscription',
    receiver: {
        bot_id: 'bot',
        state: { state: 'connected' },
        source: { kind: 'internal' },
        buffered: 0,
        first_seq: 1,
        viewers: 1,
        dropped_total: 0,
    },
};
const ok = (data: unknown): DebugCallResponse => ({
    request_id: 'req',
    result: {
        kind: 'ok',
        outcome: {
            ok: true,
            data,
            status: 'ok',
            retcode: 0,
            message: '',
            wording: '',
            raw: {},
            channel: { kind: 'internal' },
            elapsed_ms: 1,
            size_bytes: 0,
            truncated: false,
        },
    },
});
function setup() {
    const transport = {
        call: vi.fn(async () => ok({ message_id: 8 })),
        subscribe: vi.fn(
            async (_bot: string, _batch: (batch: DebugEventBatch) => void) => subscription,
        ),
        unsubscribe: vi.fn(async () => {}),
    };
    return { transport, store: new ChatAccountStore(target, transport) };
}
function transferSetup() {
    const result = setup();
    let account = emptyAccount('99');
    for (const messageId of [2, 3])
        account = ingestMessage(account, {
            message_type: 'group',
            group_id: 12,
            user_id: messageId === 2 ? 99 : 20,
            message_id: messageId,
            time: messageId,
            message: `原消息 ${messageId}`,
        });
    result.store.getSnapshot().account = account;
    const destination: Contact = { key: 'private:20', type: 'private', id: '20', name: '好友' };
    result.store.getSnapshot().contacts = [destination];
    result.store.draft(destination.key, { text: '目标会话草稿', attachments: [], reply: null });
    return { ...result, destination };
}
describe('latest navigation and inline images', () => {
    it('waits for an earlier page then fetches the actual latest page without a cursor', async () => {
        const { store, transport } = setup();
        await store.connect();
        let resolve!: (response: DebugCallResponse) => void;
        transport.call.mockImplementationOnce(
            () =>
                new Promise((done) => {
                    resolve = done;
                }),
        );
        const earlier = store.history('group:12', '8');
        transport.call.mockResolvedValueOnce(
            ok({ messages: [{ message_id: 99, user_id: 22, time: 99, message: '最新' }] }),
        );
        const latest = store.latest('group:12');
        expect(transport.call).toHaveBeenCalledTimes(1);
        resolve(ok({ messages: [{ message_id: 7, user_id: 22, time: 7, message: '旧页' }] }));
        await Promise.all([earlier, latest]);
        expect(transport.call.mock.calls[1][2]).not.toHaveProperty('message_seq');
        expect(store.getSnapshot().account.messages.at(-1)?.id).toBe('99');
        expect(store.readingPosition('group:12')?.atBottom).toBe(true);
    });
    it('cancels a late latest response without replacing a newer detached reading anchor', async () => {
        const { store, transport } = transferSetup();
        await store.connect();
        let resolve!: (response: DebugCallResponse) => void;
        transport.call.mockImplementationOnce(
            () =>
                new Promise((done) => {
                    resolve = done;
                }),
        );
        const controller = new AbortController();
        const pending = store.latest('group:12', controller.signal);
        const anchor = { messageKey: 'group:12/2', messageId: '2', offset: 15, atBottom: false };
        store.readingPosition('group:12', anchor);
        controller.abort();
        resolve(
            ok({
                messages: [{ message_id: 100, user_id: 22, time: 100, message: '迟到的最新页' }],
            }),
        );
        await pending;
        expect(store.getSnapshot().account.messages.some((message) => message.id === '100')).toBe(
            false,
        );
        expect(store.readingPosition('group:12')).toEqual(anchor);
        expect(store.getSnapshot().history['group:12'].loading).toBe(false);
    });
    it('keeps a large failed pasted image visible and sends its recovered short reference as bytes', async () => {
        const { store, transport } = setup();
        store.target = { ...target, bot_id: 'large-inline-regression' };
        await store.connect();
        const source = 'base64://' + btoa('x'.repeat(5 * 1024 * 1024));
        const failure = ok(null);
        if (failure.result.kind === 'ok')
            Object.assign(failure.result.outcome, { ok: false, retcode: 100, message: '未发送' });
        transport.call.mockResolvedValue(failure);
        store.draft('group:12', {
            text: '',
            reply: null,
            attachments: [{ key: 'image', type: 'image', path: source, name: '粘贴图片.png' }],
        });
        await store.send('group:12');
        const failed = store.getSnapshot().account.messages[0];
        expect(failed.status).toBe('failed');
        expect(String(failed.segments[0].data.file)).toMatch(/^ncd-inline-image:\/\//);
        const recovered = recoverDraft(failed, { text: '', reply: null, attachments: [] });
        const bytes = inlineImageService.stats().bytes;
        store.draft('group:12', recovered);
        await store.send('group:12');
        expect(inlineImageService.stats().bytes).toBe(bytes);
        expect(transport.call.mock.calls[1][2]).toMatchObject({
            message: [{ type: 'image', data: { file: source } }],
        });
        expect(JSON.stringify(archiveOf(store.getSnapshot().account))).not.toContain(
            'ncd-inline-image://',
        );
        inlineImageService.release(store.target.bot_id, '99');
    });
});
describe('chat contacts', () => {
    it('loads real QQ friend categories and keeps existing conversations and drafts', async () => {
        const { transport } = setup();
        const call = vi.fn(async (_bot: string, action: string) =>
            ok(
                action === 'get_group_list'
                    ? [{ group_id: 12, group_name: '讨论群' }]
                    : [
                          {
                              categoryId: 0,
                              categoryName: '我的好友',
                              buddyList: [{ user_id: 20, nickname: '小林' }],
                          },
                          {
                              categoryId: 7,
                              categoryName: '开发伙伴',
                              buddyList: [{ user_id: 21, nickname: '阿澄', remark: '项目搭档' }],
                          },
                      ],
            ),
        );
        const store = new ChatAccountStore(target, { ...transport, call });
        store.open({ key: 'private:20', type: 'private', id: '20', name: '20' });
        store.draft('private:20', { text: '保留草稿', reply: null, attachments: [] });
        await store.loadContacts();
        expect(call.mock.calls.map((args) => args[1])).toEqual([
            'get_group_list',
            'get_friends_with_category',
        ]);
        expect(store.getSnapshot().contacts).toEqual([
            expect.objectContaining({ key: 'group:12', name: '讨论群' }),
            expect.objectContaining({
                key: 'private:20',
                categoryId: '0',
                categoryName: '我的好友',
            }),
            expect.objectContaining({
                key: 'private:21',
                name: '项目搭档',
                categoryId: '7',
                categoryName: '开发伙伴',
            }),
        ]);
        expect(store.getSnapshot().account.conversations['private:20'].name).toBe('小林');
        expect(store.getSnapshot().account.drafts['private:20'].text).toBe('保留草稿');
        expect(store.getSnapshot().error).toBe('');
    });
    it('falls back to the ordinary friend list when the protocol has no categories', async () => {
        const { transport } = setup();
        const call = vi.fn(async (_bot: string, action: string) => {
            if (action === 'get_friends_with_category') throw new Error('不支持此接口');
            return ok(
                action === 'get_friend_list'
                    ? [{ user_id: 20, nickname: '好友', category_id: 0, categoryName: '我的好友' }]
                    : [],
            );
        });
        const store = new ChatAccountStore(target, { ...transport, call });
        await store.loadContacts();
        expect(call.mock.calls.map((args) => args[1])).toEqual([
            'get_group_list',
            'get_friends_with_category',
            'get_friend_list',
        ]);
        expect(store.getSnapshot().contacts).toEqual([
            expect.objectContaining({
                key: 'private:20',
                name: '好友',
                categoryId: '0',
                categoryName: '我的好友',
            }),
        ]);
        expect(store.getSnapshot().error).toBe('');
    });
    it('retains known contacts when both grouped and fallback responses are unusable', async () => {
        const { transport } = setup();
        const call = vi.fn(async (_bot: string, action: string) =>
            ok(
                action === 'get_friends_with_category'
                    ? [{ categoryId: 1, categoryName: '损坏分组', buddyList: null }]
                    : null,
            ),
        );
        const store = new ChatAccountStore(target, { ...transport, call });
        const contacts: Contact[] = [
            { key: 'group:12', type: 'group', id: '12', name: '讨论群' },
            {
                key: 'private:20',
                type: 'private',
                id: '20',
                name: '好友',
                categoryId: '1',
                categoryName: '伙伴',
            },
        ];
        store.getSnapshot().contacts = contacts;
        await store.loadContacts();
        expect(store.getSnapshot().contacts).toEqual(contacts);
        expect(store.getSnapshot().error).toContain('列表返回格式无法识别');
    });
});
describe('chat lifecycle', () => {
    it('marks a conversation read locally without opening it, changing drafts, or stopping unread tracking', async () => {
        const { store, transport } = setup();
        await store.connect();
        store.open({ key: 'private:1', type: 'private', id: '1', name: '正在编辑' });
        store.draft('private:1', {
            text: '保留草稿',
            reply: { id: '7', name: '好友', preview: '回复' },
            attachments: [{ key: 'file', name: '说明.txt', type: 'file', path: 'D:/说明.txt' }],
        });
        const receive = (seq: number) =>
            transport.subscribe.mock.calls[0][1]({
                v: 1,
                bot_id: 'bot',
                events: [
                    {
                        seq,
                        at_ms: seq,
                        body: {
                            kind: 'ob11',
                            payload: {
                                self_id: 99,
                                message_type: 'private',
                                user_id: 12,
                                message_id: seq,
                                time: seq,
                                message: '未读消息',
                            },
                        },
                    },
                ],
            });
        receive(1);
        const before = store.getSnapshot().account;
        expect(before.conversations['private:12'].unread).toBe(1);
        store.markRead('private:12');
        const after = store.getSnapshot().account;
        expect(after.conversations['private:12'].unread).toBe(0);
        expect(after.active).toBe('private:1');
        expect(after.drafts).toBe(before.drafts);
        expect(after.messages).toBe(before.messages);
        expect(after.conversations['private:1']).toBe(before.conversations['private:1']);
        expect(transport.call).not.toHaveBeenCalled();
        receive(2);
        expect(store.getSnapshot().account.conversations['private:12'].unread).toBe(1);
    });
    it('does not publish changes for unknown or already read conversations', () => {
        const { store } = setup();
        store.open({ key: 'private:1', type: 'private', id: '1', name: '已读' });
        const before = store.getSnapshot();
        const listener = vi.fn();
        store.subscribe(listener);
        store.markRead('private:missing');
        store.markRead('private:1');
        expect(store.getSnapshot()).toBe(before);
        expect(listener).not.toHaveBeenCalled();
    });
    it('sends QQ faces and remote favorite images as native segments', async () => {
        const { store, transport } = setup();
        await store.connect();
        store.draft('private:12', {
            text: '',
            reply: null,
            attachments: [
                { key: 'face', type: 'face', id: '14', name: '微笑' },
                {
                    key: 'favorite',
                    type: 'image',
                    path: 'https://cdn.example/favorite.gif',
                    name: '收藏表情',
                    subType: 1,
                },
            ],
        });
        await store.send('private:12');
        expect(transport.call.mock.calls[0].slice(0, 3)).toMatchObject([
            'bot',
            'send_private_msg',
            {
                user_id: '12',
                message: [
                    { type: 'face', data: { id: '14' } },
                    {
                        type: 'image',
                        data: { file: 'https://cdn.example/favorite.gif', sub_type: 1 },
                    },
                ],
            },
        ]);
        expect(store.getSnapshot().account.drafts['private:12'].attachments).toEqual([]);
    });
    it('rejects unsupported faces before submitting and restores the draft with a clear error', async () => {
        const { transport } = setup();
        const call = vi.fn(async (_bot: string, action: string) =>
            ok(
                action === 'fetch_sys_faces'
                    ? { packs: [{ emojis: [{ q_sid: '14', q_des: '微笑' }] }] }
                    : { message_id: 8 },
            ),
        );
        const store = new ChatAccountStore(
            { ...target, bot_id: 'unsupported-face', backend: 'snowluma' },
            { ...transport, call },
        );
        await store.connect();
        store.draft('private:12', {
            text: '保留文本',
            reply: null,
            attachments: [{ key: 'face', type: 'face', id: '486', name: '开学啦2' }],
        });
        await store.send('private:12');
        expect(call).toHaveBeenCalledTimes(1);
        expect(call.mock.calls[0][1]).toBe('fetch_sys_faces');
        expect(store.getSnapshot().account.messages.at(-1)).toMatchObject({
            status: 'failed',
            error: expect.stringContaining('不支持 QQ 表情 486'),
        });
        expect(store.getSnapshot().account.drafts['private:12']).toMatchObject({
            text: '保留文本',
            attachments: [expect.objectContaining({ id: '486' })],
        });
    });
    it('refreshes face metadata and submits a supported new face exactly once', async () => {
        const { transport } = setup();
        const call = vi.fn(async (_bot: string, action: string, _params: unknown) =>
            ok(
                action === 'fetch_sys_faces'
                    ? {
                          packs: [
                              {
                                  emojis: [
                                      {
                                          q_sid: '486',
                                          q_des: '开学啦2',
                                          is_super: true,
                                          ani_sticker_pack_id: 1,
                                          ani_sticker_id: 2,
                                          ani_sticker_type: 3,
                                      },
                                  ],
                              },
                          ],
                      }
                    : { message_id: 8 },
            ),
        );
        const store = new ChatAccountStore(
            { ...target, bot_id: 'supported-face', backend: 'snowluma' },
            { ...transport, call },
        );
        await store.connect();
        store.draft('private:12', {
            text: '',
            reply: null,
            attachments: [{ key: 'face', type: 'face', id: '486', name: '开学啦2' }],
        });
        await store.send('private:12');
        expect(call.mock.calls.map((args) => args[1])).toEqual([
            'fetch_sys_faces',
            'send_private_msg',
        ]);
        expect(call.mock.calls[1][2]).toMatchObject({
            message: [{ type: 'face', data: { id: '486' } }],
        });
        expect(store.getSnapshot().account.messages.at(-1)).toMatchObject({
            status: 'sent',
            id: '8',
        });
    });
    it('keeps exhausted recent pages available while other conversations receive messages', async () => {
        const { transport } = setup();
        const seed = ingestMessage(emptyAccount('99'), {
            message_type: 'group',
            group_id: 12,
            message_id: 100,
            user_id: 22,
            time: 100,
            message: 'A',
        });
        seed.messages = Array.from({ length: 4999 }, (_, index) => ({
            ...seed.messages[0],
            id: String(index + 100),
            key: `group:12/${index + 100}`,
            at: index + 100000,
        }));
        const archive = { load: vi.fn(async () => archiveOf(seed)), save: vi.fn(async () => {}) };
        const store = new ChatAccountStore(target, transport, archive);
        await store.restore();
        await store.connect();
        transport.call.mockResolvedValue(
            ok({ messages: [{ message_id: 1, time: 1, user_id: 22, message: 'B' }] }),
        );
        store.open({ key: 'group:13', type: 'group', id: '13', name: 'B' });
        await store.ensureHistory('group:13');
        expect(store.getSnapshot().history['group:13'].done).toBe(true);
        store.open({ key: 'group:12', type: 'group', id: '12', name: 'A' });
        transport.subscribe.mock.calls[0][1]({
            v: 1,
            bot_id: 'bot',
            events: [
                {
                    seq: 1,
                    at_ms: 1,
                    body: {
                        kind: 'ob11',
                        payload: {
                            message_type: 'group',
                            group_id: 12,
                            message_id: 6000,
                            user_id: 22,
                            time: 1000,
                            message: 'new A',
                        },
                    },
                },
            ],
        });
        expect(
            store
                .getSnapshot()
                .account.messages.filter((message) => message.session === 'group:13'),
        ).toHaveLength(1);
        expect(store.getSnapshot().history['group:13']).toMatchObject({
            loaded: true,
            done: true,
        });
        store.open({ key: 'group:13', type: 'group', id: '13', name: 'B' });
        await store.ensureHistory('group:13');
        expect(transport.call).toHaveBeenCalledTimes(1);
        expect(
            store
                .getSnapshot()
                .account.messages.filter((message) => message.session === 'group:13'),
        ).toHaveLength(1);
    });
    it('continues beyond the page budget without resetting the history cursor and can return to latest', async () => {
        const { transport } = setup();
        const cursors: unknown[] = [];
        const call = vi.fn(async (_bot: string, _action: string, params: unknown) => {
            const cursor = (params as { message_seq?: string }).message_seq;
            cursors.push(cursor);
            const end = cursor ? Number(cursor) - 1 : 1000;
            return ok({
                messages: Array.from({ length: 50 }, (_, index) => ({
                    message_id: end - 49 + index,
                    time: end - 49 + index,
                    user_id: 22,
                    message: `消息${end - 49 + index}`,
                })),
            });
        });
        const store = new ChatAccountStore(target, { ...transport, call });
        await store.connect();
        store.open({ key: 'group:12', type: 'group', id: '12', name: '群' });
        for (let page = 0; page < 12; page++) {
            const first = store
                .getSnapshot()
                .account.messages.find((message) => message.session === 'group:12');
            if (first)
                store.readingPosition('group:12', {
                    messageKey: first.key,
                    messageId: first.id!,
                    offset: 14,
                    atBottom: false,
                });
            await store.history('group:12');
            expect(store.getSnapshot().account.messages.length).toBeLessThanOrEqual(200);
        }
        expect(cursors).toEqual([
            undefined,
            ...Array.from({ length: 11 }, (_, index) => String(951 - index * 50)),
        ]);
        expect(store.getSnapshot().account.messages[0].id).toBe('401');
        expect(store.getSnapshot().account.messages.some((message) => message.gapBefore)).toBe(
            true,
        );
        await store.latest('group:12');
        expect(cursors.at(-1)).toBeUndefined();
        expect(store.getSnapshot().account.messages.at(-1)?.id).toBe('1000');
        expect(store.getSnapshot().account.messages.some((message) => message.gapBefore)).toBe(
            false,
        );
    });
    it('syncs contacts and recent conversations when an initially connecting receiver becomes ready', async () => {
        const { store, transport } = setup();
        transport.subscribe.mockResolvedValue({
            ...subscription,
            receiver: { ...subscription.receiver, state: { state: 'connecting' } },
        });
        transport.call.mockResolvedValue(ok([]));
        await store.initialize();
        expect(transport.call).not.toHaveBeenCalled();
        const batch: DebugEventBatch = {
            v: 1,
            bot_id: 'bot',
            events: [
                { seq: 1, at_ms: 1, body: { kind: 'receiver', state: { state: 'connected' } } },
            ],
        };
        transport.subscribe.mock.calls[0][1](batch);
        await vi.waitFor(() => expect(transport.call).toHaveBeenCalledTimes(3));
        expect(transport.call.mock.calls.map((call) => (call as unknown[])[1]).sort()).toEqual([
            'get_friends_with_category',
            'get_group_list',
            'get_recent_contact',
        ]);
        transport.subscribe.mock.calls[0][1]({
            ...batch,
            events: [{ ...batch.events[0], seq: 2 }],
        });
        expect(transport.call).toHaveBeenCalledTimes(3);
    });
    it('uses the oldest message ID for NapCat message_seq, not the NT sequence, and deduplicates overlapping rows', async () => {
        const { store, transport } = setup();
        await store.connect();
        store.open({ key: 'group:12', type: 'group', id: '12', name: '群' });
        const page = (start: number) =>
            Array.from({ length: 50 }, (_, i) => ({
                message_id: start + i + 100000,
                message_seq: String(start + i),
                time: start + i,
                user_id: 22,
                message: `消息${start + i}`,
            }));
        transport.call
            .mockResolvedValueOnce(ok({ messages: page(51) }))
            .mockResolvedValueOnce(ok({ messages: page(2) }));
        await store.history('group:12');
        await store.history('group:12');
        expect(transport.call.mock.calls[1]).toMatchObject([
            'bot',
            'get_group_msg_history',
            { message_seq: '100051' },
        ]);
        expect(store.getSnapshot().account.messages).toHaveLength(99);
        expect(store.getSnapshot().account.messages[0].id).toBe('100002');
        expect(store.getSnapshot().account.conversations['group:12'].unread).toBe(0);
    });
    it('loads recent history once after connection, including an empty conversation', async () => {
        const { store, transport } = setup();
        transport.call.mockResolvedValue(ok({ messages: [] }));
        await store.ensureHistory('private:12');
        expect(transport.call).not.toHaveBeenCalled();
        await store.connect();
        await Promise.all([store.ensureHistory('private:12'), store.ensureHistory('private:12')]);
        await store.ensureHistory('private:12');
        expect(transport.call).toHaveBeenCalledTimes(1);
        expect(store.getSnapshot().history['private:12']).toMatchObject({
            loaded: true,
            done: true,
        });
    });
    it('waits for an explicit history retry after failure', async () => {
        const { store, transport } = setup();
        await store.connect();
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
        const { store, transport } = setup();
        await store.connect();
        let resolve!: (result: DebugCallResponse) => void;
        transport.call.mockImplementationOnce(
            () =>
                new Promise((done) => {
                    resolve = done;
                }),
        );
        const first = store.ensureHistory('private:12');
        await store.disconnect();
        await store.connect();
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
        transport.subscribe.mockImplementation(
            () =>
                new Promise((done) => {
                    resolve = done;
                }),
        );
        const connect = store.connect();
        await store.disconnect();
        resolve(subscription);
        await connect;
        expect(transport.unsubscribe).toHaveBeenCalledWith('subscription');
        expect(store.getSnapshot().connection.state).toBe('stopped');
    });
    it('keeps edits made while an earlier message is sending', async () => {
        const { store, transport } = setup();
        await store.connect();
        let resolve!: (result: DebugCallResponse) => void;
        transport.call.mockImplementation(
            () =>
                new Promise((done) => {
                    resolve = done;
                }),
        );
        store.draft('group:12', { text: '第一条', reply: null, attachments: [] });
        const pending = store.send('group:12');
        store.draft('group:12', { text: '第二条草稿', reply: null, attachments: [] });
        resolve(ok({ message_id: 8 }));
        await pending;
        expect(store.getSnapshot().account.drafts['group:12'].text).toBe('第二条草稿');
        expect(store.getSnapshot().account.messages[0].status).toBe('sent');
    });
    it('repeats the complete message once without consuming edits and keeps an uncertain result out of retry', async () => {
        const { store, transport } = transferSetup();
        await store.connect();
        const source = store.getSnapshot().account.messages[0];
        source.segments = [
            { type: 'text', data: { text: '一起发送' } },
            {
                type: 'image',
                data: { file: 'image-token', url: 'https://cdn.example/picture.gif', sub_type: 1 },
            },
            { type: 'face', data: { id: '14' } },
            { type: 'markdown', data: { content: '**同一段内容**' } },
            {
                type: 'markdown',
                data: { data: '{"content":"**旧档案正文**"}', template_id: 'legacy' },
            },
        ];
        store.draft(source.session, { text: '正在写', reply: null, attachments: [] });
        let resolve!: (response: DebugCallResponse) => void;
        transport.call.mockImplementation((...args: unknown[]) =>
            args[1] === 'get_image'
                ? Promise.resolve(
                      ok({ file: '/bot/cache/picture.gif', url: 'https://cdn.example/stale.gif' }),
                  )
                : new Promise((done) => {
                      resolve = done;
                  }),
        );
        const first = store.repeat(source.key);
        await store.repeat(source.key);
        await vi.waitFor(() => expect(transport.call).toHaveBeenCalledTimes(2));
        expect(transport.call.mock.calls[0].slice(0, 3)).toEqual([
            'bot',
            'get_image',
            { file: 'image-token' },
        ]);
        expect(transport.call.mock.calls[1].slice(0, 3)).toEqual([
            'bot',
            'send_group_msg',
            {
                group_id: '12',
                message: source.segments.map((segment) =>
                    segment.type === 'image'
                        ? { ...segment, data: { file: '/bot/cache/picture.gif', sub_type: 1 } }
                        : segment.type === 'markdown' && segment.data.content === undefined
                          ? { ...segment, data: { ...segment.data, content: '**旧档案正文**' } }
                          : segment,
                ),
            },
        ]);
        store.draft(source.session, { text: '继续写', reply: null, attachments: [] });
        resolve({
            request_id: 'req',
            result: { kind: 'err', error: { kind: 'timeout', ms: 30000 } },
        });
        await first;
        const pending = store.getSnapshot().account.messages.at(-1)!;
        expect(pending.status).toBe('unknown');
        expect(source.status).toBe('sent');
        expect(store.getSnapshot().account.drafts[source.session].text).toBe('继续写');
        await store.retry(pending.key);
        expect(transport.call).toHaveBeenCalledTimes(2);
    });
    it('marks a repeat image lookup failure as unsent without replaying the expired URL or consuming the draft', async () => {
        const { store, transport } = transferSetup();
        await store.connect();
        const source = store.getSnapshot().account.messages[0];
        source.segments = [
            {
                type: 'image',
                data: { file: 'expired-resource', url: 'https://cdn.example/stale?rkey=old' },
            },
        ];
        store.draft(source.session, { text: '保留草稿', reply: null, attachments: [] });
        const failure = ok(null);
        if (failure.result.kind === 'ok')
            Object.assign(failure.result.outcome, {
                ok: false,
                retcode: 100,
                message: '图片缓存已失效',
            });
        transport.call.mockResolvedValue(failure);
        await store.repeat(source.key);
        expect(
            transport.call.mock.calls.every((args) => (args as unknown[])[1] === 'get_image'),
        ).toBe(true);
        expect(store.getSnapshot().account.messages.at(-1)).toMatchObject({
            status: 'failed',
            error: '图片缓存已失效',
        });
        expect(store.getSnapshot().account.drafts[source.session].text).toBe('保留草稿');
    });
    it('preserves only explicit SnowLuma face variants when repeating a message', async () => {
        const { store, transport } = transferSetup();
        store.target = { ...target, bot_id: 'repeat-face-format', backend: 'snowluma' };
        await store.connect();
        const source = store.getSnapshot().account.messages[0];
        source.segments = [
            { type: 'face', data: { id: '14', raw: { faceType: 3 } } },
            { type: 'face', data: { id: '14', raw: { faceType: 1 } } },
            { type: 'face', data: { id: '14' } },
            { type: 'face', data: { id: '14', large: false, raw: { faceType: 3 } } },
        ];
        transport.call.mockImplementation((...args: unknown[]) =>
            Promise.resolve(
                ok(
                    args[1] === 'fetch_sys_faces'
                        ? { packs: [{ emojis: [{ q_sid: '14', q_des: '微笑' }] }] }
                        : { message_id: 8 },
                ),
            ),
        );
        await store.repeat(source.key);
        const send = transport.call.mock.calls.find(
            (args) => (args as unknown[])[1] === 'send_group_msg',
        )!;
        expect((send[2] as { message: unknown[] }).message).toEqual(
            source.segments.map((segment, index) =>
                index === 2
                    ? segment
                    : { ...segment, data: { ...segment.data, large: index === 0 } },
            ),
        );
        expect(source.segments[0].data.large).toBeUndefined();
        expect(source.segments[1].data.large).toBeUndefined();
    });
    it('does not submit a repeat after disconnect while the Bot image lookup is pending', async () => {
        const { store, transport } = transferSetup();
        await store.connect();
        const source = store.getSnapshot().account.messages[0];
        source.segments = [{ type: 'image', data: { file_id: 'pending-image' } }];
        let resolve!: (response: DebugCallResponse) => void;
        transport.call.mockImplementationOnce(
            () =>
                new Promise((done) => {
                    resolve = done;
                }),
        );
        const repeat = store.repeat(source.key);
        await store.disconnect();
        resolve(ok({ file: '/bot/cache/image.png' }));
        await repeat;
        expect(transport.call).toHaveBeenCalledOnce();
        expect(transport.call.mock.calls[0].slice(0, 3)).toEqual([
            'bot',
            'get_image',
            { file_id: 'pending-image' },
        ]);
    });
    it('does not resend an unknown result', async () => {
        const { store, transport } = setup();
        await store.connect();
        transport.call.mockResolvedValue({
            request_id: 'req',
            result: { kind: 'err', error: { kind: 'timeout', ms: 30000 } },
        });
        store.draft('group:12', { text: '可能已经送达', reply: null, attachments: [] });
        await store.send('group:12');
        expect(store.getSnapshot().account.messages[0].status).toBe('unknown');
        expect(transport.call).toHaveBeenCalledTimes(1);
    });
    it('marks in-flight sends unknown when the connection is explicitly released', async () => {
        const { store, transport } = setup();
        await store.connect();
        let resolve!: (result: DebugCallResponse) => void;
        transport.call.mockImplementation(
            () =>
                new Promise((done) => {
                    resolve = done;
                }),
        );
        store.draft('group:12', { text: '离线前的消息', reply: null, attachments: [] });
        const pending = store.send('group:12');
        await store.disconnect();
        resolve(ok({ message_id: 8 }));
        await pending;
        expect(store.getSnapshot().account.messages[0].status).toBe('unknown');
    });
    it('passes pasted images as base64 rather than local file paths', async () => {
        const calls: unknown[][] = [];
        const { store, transport } = setup();
        await store.connect();
        transport.call.mockImplementation(async (...args: unknown[]) => {
            calls.push(args);
            return ok({ message_id: 8 });
        });
        store.draft('group:12', {
            text: '',
            reply: null,
            attachments: [{ key: 'image', name: '截图.png', path: 'base64://abc', type: 'image' }],
        });
        await store.send('group:12');
        expect(calls[0][2]).toMatchObject({
            message: [{ type: 'image', data: { file: 'base64://abc' } }],
        });
    });
    it('keeps a large settled image through reopening the current conversation and returning to latest', async () => {
        const { store } = setup();
        await store.connect();
        const contact = {
            key: 'group:12' as const,
            type: 'group' as const,
            id: '12',
            name: '测试群',
        };
        const file = 'base64://' + 'A'.repeat(5 * 1024 * 1024);
        store.open(contact);
        store.draft('group:12', {
            text: '',
            reply: null,
            attachments: [{ key: 'large', name: '截图.png', path: file, type: 'image' }],
        });
        await store.send('group:12');
        const reference = store.getSnapshot().account.messages[0].segments[0].data.file;
        expect(reference).toMatch(/^ncd-inline-image:\/\//);
        store.open(contact);
        expect(store.getSnapshot().account.messages[0].segments[0].data.file).toBe(reference);
        await store.latest('group:12');
        expect(store.getSnapshot().account.messages[0].segments[0].data.file).toBe(reference);
        expect(
            archiveOf(store.getSnapshot().account).messages[0].segments[0].data.file,
        ).toBeUndefined();
    });
    it('retries the complete failed image and text together without touching a newer draft or duplicating the message', async () => {
        const { store, transport } = setup();
        await store.connect();
        const failed = ok(null);
        if (failed.result.kind === 'ok')
            Object.assign(failed.result.outcome, {
                ok: false,
                retcode: 1200,
                message: 'An unknown error occurred.',
            });
        transport.call.mockResolvedValueOnce(failed);
        store.draft('private:12', {
            text: '我的项目有点雏形了',
            reply: { id: '7', name: '朋友', preview: '前文' },
            attachments: [{ key: 'image', name: '截图.png', path: 'base64://abc', type: 'image' }],
        });
        await store.send('private:12');
        const message = store.getSnapshot().account.messages[0];
        expect(message).toMatchObject({
            status: 'failed',
            error: 'An unknown error occurred.（错误码 1200）',
        });
        expect(transport.call.mock.calls[0][2]).toMatchObject({
            user_id: '12',
            message: [
                { type: 'reply', data: { id: '7' } },
                { type: 'text', data: { text: '我的项目有点雏形了' } },
                { type: 'image', data: { file: 'base64://abc', name: '截图.png', sub_type: 0 } },
            ],
        });
        const nextDraft = {
            text: '新草稿',
            reply: null,
            attachments: [
                { key: 'next', name: '下一张.png', path: 'D:/下一张.png', type: 'image' as const },
            ],
        };
        store.draft('private:12', nextDraft);
        let resolve!: (response: DebugCallResponse) => void;
        transport.call.mockImplementationOnce(
            () =>
                new Promise((done) => {
                    resolve = done;
                }),
        );
        const retry = store.retry(message.key);
        await store.retry(message.key);
        expect(transport.call).toHaveBeenCalledTimes(2);
        expect(transport.call.mock.calls[1].slice(0, 3)).toEqual(
            transport.call.mock.calls[0].slice(0, 3),
        );
        expect(transport.call.mock.calls[1][3]).not.toBe(transport.call.mock.calls[0][3]);
        expect(store.getSnapshot().account.messages).toHaveLength(1);
        expect(store.getSnapshot().account.messages[0].status).toBe('sending');
        expect(store.getSnapshot().account.drafts['private:12']).toEqual(nextDraft);
        resolve(ok({ message_id: 9 }));
        await retry;
        expect(store.getSnapshot().account.messages).toHaveLength(1);
        expect(store.getSnapshot().account.messages[0]).toMatchObject({
            key: message.key,
            status: 'sent',
            id: '9',
        });
    });
    it('retries only the failed upload when the accompanying text already succeeded', async () => {
        const { store, transport } = setup();
        await store.connect();
        const failed = ok(null);
        if (failed.result.kind === 'ok')
            Object.assign(failed.result.outcome, { ok: false, retcode: 100, message: '上传失败' });
        transport.call.mockResolvedValueOnce(ok({ message_id: 8 })).mockResolvedValueOnce(failed);
        store.draft('group:12', {
            text: '文件在这里',
            reply: null,
            attachments: [{ key: 'file', name: '说明.txt', path: 'D:/说明.txt', type: 'file' }],
        });
        await store.send('group:12');
        const file = store
            .getSnapshot()
            .account.messages.find((message) => message.status === 'failed')!;
        transport.call.mockResolvedValueOnce(ok({ file_id: 'uploaded-file' }));
        await store.retry(file.key);
        expect(transport.call).toHaveBeenCalledTimes(3);
        expect(transport.call.mock.calls[2].slice(0, 3)).toEqual([
            'bot',
            'upload_group_file',
            {
                group_id: '12',
                file: 'ncd-local-file://D:/说明.txt',
                name: '说明.txt',
                upload_file: true,
            },
        ]);
        expect(store.getSnapshot().account.messages).toHaveLength(2);
    });
    it('does not retry unknown results or a failed attachment whose bytes were removed from the saved archive', async () => {
        const { store, transport } = setup();
        await store.connect();
        transport.call.mockResolvedValueOnce({
            request_id: 'req',
            result: { kind: 'err', error: { kind: 'timeout', ms: 30000 } },
        });
        store.draft('group:12', { text: '可能已经送达', reply: null, attachments: [] });
        await store.send('group:12');
        await store.retry(store.getSnapshot().account.messages[0].key);
        expect(transport.call).toHaveBeenCalledTimes(1);
        const failed = ok(null);
        if (failed.result.kind === 'ok')
            Object.assign(failed.result.outcome, { ok: false, retcode: 100 });
        transport.call.mockResolvedValueOnce(failed);
        store.draft('group:12', {
            text: '',
            reply: null,
            attachments: [{ key: 'image', name: '截图.png', path: 'base64://abc', type: 'image' }],
        });
        await store.send('group:12');
        const message = store.getSnapshot().account.messages[1];
        message.segments = [{ type: 'image', data: { name: '截图.png' } }];
        await expect(store.retry(message.key)).rejects.toThrow('原附件已不可用');
        expect(transport.call).toHaveBeenCalledTimes(2);
        expect(message.status).toBe('failed');
    });
    it('releases interrupted sends on reconnect without releasing a newer send lock', async () => {
        const { store, transport } = setup();
        await store.connect();
        let resolveOld!: (result: DebugCallResponse) => void;
        let resolveNew!: (result: DebugCallResponse) => void;
        transport.call.mockImplementationOnce(
            () =>
                new Promise((done) => {
                    resolveOld = done;
                }),
        );
        transport.call.mockImplementationOnce(
            () =>
                new Promise((done) => {
                    resolveNew = done;
                }),
        );
        store.draft('group:12', { text: '重连前', reply: null, attachments: [] });
        const oldSend = store.send('group:12');
        transport.subscribe.mock.calls[0][1]({
            v: 1,
            bot_id: 'bot',
            events: [
                {
                    seq: 1,
                    at_ms: 1,
                    body: { kind: 'receiver', state: { state: 'stopped', reason: '连接中断' } },
                },
            ],
        });
        await store.connect();
        expect(store.getSnapshot().account.messages[0].status).toBe('unknown');
        store.draft('group:12', { text: '重连后', reply: null, attachments: [] });
        const newSend = store.send('group:12');
        expect(transport.call).toHaveBeenCalledTimes(2);
        resolveOld(ok({ message_id: 8 }));
        await oldSend;
        store.draft('group:12', { text: '继续编辑', reply: null, attachments: [] });
        await store.send('group:12');
        expect(transport.call).toHaveBeenCalledTimes(2);
        resolveNew(ok({ message_id: 9 }));
        await newSend;
        expect(store.getSnapshot().account.messages.map((message) => message.status)).toEqual([
            'unknown',
            'sent',
        ]);
        expect(store.getSnapshot().account.drafts['group:12'].text).toBe('继续编辑');
    });
    it('retains attachments not submitted when the first send is interrupted', async () => {
        const { store, transport } = setup();
        await store.connect();
        let resolve!: (result: DebugCallResponse) => void;
        transport.call.mockImplementation(
            () =>
                new Promise((done) => {
                    resolve = done;
                }),
        );
        store.draft('group:12', {
            text: '附件在这里',
            reply: null,
            attachments: [{ key: 'file', name: '说明.txt', path: 'D:/说明.txt', type: 'file' }],
        });
        const pending = store.send('group:12');
        await store.disconnect();
        resolve(ok({ message_id: 8 }));
        await pending;
        expect(transport.call).toHaveBeenCalledTimes(1);
        expect(
            store
                .getSnapshot()
                .account.messages.some(
                    (message) => message.status === 'failed' && message.segments[0].type === 'file',
                ),
        ).toBe(true);
    });
    it('ignores events from another signed-in account', async () => {
        const { store, transport } = setup();
        await store.connect();
        transport.subscribe.mock.calls[0][1]({
            v: 1,
            bot_id: 'bot',
            events: [
                {
                    seq: 1,
                    at_ms: 1,
                    body: {
                        kind: 'ob11',
                        payload: {
                            self_id: 100,
                            message_type: 'group',
                            group_id: 12,
                            user_id: 13,
                            message_id: 8,
                            message: 'wrong account',
                        },
                    },
                },
            ],
        });
        expect(store.getSnapshot().account.messages).toHaveLength(0);
    });
    it.each(['before', 'after'] as const)(
        'merges a file echo arriving %s the upload response',
        async (order) => {
            const { store, transport } = setup();
            store.target = { ...target, backend: 'snowluma' };
            await store.connect();
            let resolve!: (result: DebugCallResponse) => void;
            transport.call.mockImplementationOnce(
                () =>
                    new Promise((done) => {
                        resolve = done;
                    }),
            );
            store.draft('private:88', {
                text: '',
                reply: null,
                attachments: [
                    { key: 'file', name: 'inline.txt', path: 'D:/inline.txt', type: 'file' },
                ],
            });
            const pending = store.send('private:88');
            const echo = () =>
                transport.subscribe.mock.calls[0][1]({
                    v: 1,
                    bot_id: 'bot',
                    events: [
                        {
                            seq: 1,
                            at_ms: Date.now(),
                            body: {
                                kind: 'ob11',
                                payload: {
                                    post_type: 'message_sent',
                                    message_type: 'private',
                                    self_id: 99,
                                    user_id: 99,
                                    target_id: 88,
                                    message_id: 8,
                                    time: Math.floor(Date.now() / 1000),
                                    message: [
                                        {
                                            type: 'file',
                                            data: {
                                                file_id: 'uploaded-file-uuid',
                                                file: 'inline.txt',
                                                file_size: 3,
                                            },
                                        },
                                    ],
                                },
                            },
                        },
                    ],
                });
            if (order === 'before') echo();
            resolve(ok({ file_id: 'uploaded-file-uuid' }));
            await pending;
            if (order === 'after') echo();
            const messages = store.getSnapshot().account.messages;
            expect(messages).toHaveLength(1);
            expect(messages[0]).toMatchObject({ status: 'sent', id: '8', session: 'private:88' });
            expect(messages[0].segments[0].data.file).toBe('inline.txt');
        },
    );
});

describe('chat message transfer', () => {
    it('recalls an owned sent message in both working set and archive and preserves recall on history replay', async () => {
        const { store, transport, destination } = transferSetup();
        await store.connect();
        const draft = store.getSnapshot().account.drafts[destination.key];
        await store.recall('group:12/2');
        expect(transport.call.mock.calls[0]).toMatchObject([
            'bot',
            'delete_msg',
            { message_id: '2' },
        ]);
        expect(store.getSnapshot().account.messages[0].recalled).toBe(true);
        expect(store.getSnapshot().account.archiveMessages?.[0].recalled).toBe(true);
        store.getSnapshot().account.messages = [];
        transport.call.mockResolvedValueOnce(
            ok({ messages: [{ message_id: 2, user_id: 99, time: 2, message: '旧历史' }] }),
        );
        await store.history('group:12');
        expect(store.getSnapshot().account.messages[0].recalled).toBe(true);
        expect(
            store.getSnapshot().account.archiveMessages?.find((message) => message.id === '2')
                ?.recalled,
        ).toBe(true);
        expect(store.getSnapshot().account.drafts[destination.key]).toBe(draft);
    });
    it.each([
        { mine: false },
        { id: undefined },
        { recalled: true },
        { status: 'failed' as const },
        { status: 'sending' as const },
        { status: 'unknown' as const },
    ])('does not submit an ineligible recall (%j)', async (patch) => {
        const { store, transport } = transferSetup();
        await store.connect();
        Object.assign(store.getSnapshot().account.messages[0], patch);
        await store.recall('group:12/2');
        expect(transport.call).not.toHaveBeenCalled();
    });
    it('deduplicates recall clicks and leaves the original message intact on protocol failure', async () => {
        const { store, transport } = transferSetup();
        await store.connect();
        let resolve!: (response: DebugCallResponse) => void;
        transport.call.mockImplementationOnce(
            () =>
                new Promise((done) => {
                    resolve = done;
                }),
        );
        const first = store.recall('group:12/2');
        const rejected = expect(first).rejects.toThrow('超过撤回时间');
        await store.recall('group:12/2');
        const failed = ok(null);
        if (failed.result.kind === 'ok')
            Object.assign(failed.result.outcome, {
                ok: false,
                retcode: 100,
                wording: '超过撤回时间',
            });
        resolve(failed);
        await rejected;
        expect(transport.call).toHaveBeenCalledTimes(1);
        expect(store.getSnapshot().account.messages[0].recalled).not.toBe(true);
    });
    it('ignores a late recall response after the account identity changes', async () => {
        const { store, transport } = transferSetup();
        await store.connect();
        let resolve!: (response: DebugCallResponse) => void;
        transport.call.mockImplementationOnce(
            () =>
                new Promise((done) => {
                    resolve = done;
                }),
        );
        const recalling = store.recall('group:12/2');
        store.target = { ...target, qq_id: 100 };
        resolve(ok(null));
        await recalling;
        expect(store.getSnapshot().account.messages[0].recalled).not.toBe(true);
    });
    it.each(['napcat', 'snowluma'] as const)(
        'forwards ordered message IDs using %s and keeps both drafts',
        async (backend) => {
            const { store, transport, destination } = transferSetup();
            store.target = { ...target, backend };
            await store.connect();
            store.draft('group:12', { text: '原会话草稿', attachments: [], reply: null });
            const drafts = store.getSnapshot().account.drafts;
            await store.forward(['group:12/3', 'group:12/2'], destination);
            expect(transport.call.mock.calls[0].slice(0, 3)).toEqual([
                'bot',
                'send_private_forward_msg',
                {
                    user_id: backend === 'snowluma' ? 20 : '20',
                    messages: [2, 3].map((id) => ({
                        type: 'node',
                        data: { id: backend === 'snowluma' ? id : String(id) },
                    })),
                },
            ]);
            expect(store.getSnapshot().account.drafts).toBe(drafts);
            expect(store.getSnapshot().account.messages.at(-1)).toMatchObject({
                session: destination.key,
                status: 'sent',
                id: '8',
                segments: [{ type: 'forward' }],
            });
            expect(store.getSnapshot().account.conversations[destination.key].name).toBe('好友');
        },
    );
    it('rejects too many messages and targets outside the current account before submitting', async () => {
        const { store, transport, destination } = transferSetup();
        await store.connect();
        await expect(
            store.forward(
                Array.from({ length: 21 }, (_, index) => `message/${index}`),
                destination,
            ),
        ).rejects.toThrow('1–20');
        await expect(
            store.forward(['group:12/2'], {
                key: 'private:44',
                type: 'private',
                id: '44',
                name: '另一个账号的好友',
            }),
        ).rejects.toThrow('当前账号');
        expect(transport.call).not.toHaveBeenCalled();
    });
    it('leaves uncertain forwards visible without retrying them or clearing the target draft', async () => {
        const { store, transport, destination } = transferSetup();
        await store.connect();
        const draft = store.getSnapshot().account.drafts[destination.key];
        transport.call.mockResolvedValueOnce({
            request_id: 'req',
            result: { kind: 'err', error: { kind: 'timeout', ms: 30_000 } },
        });
        await expect(store.forward(['group:12/2'], destination)).rejects.toMatchObject({
            uncertain: true,
        });
        const pending = store.getSnapshot().account.messages.at(-1)!;
        expect(pending.status).toBe('unknown');
        await store.retry(pending.key);
        expect(transport.call).toHaveBeenCalledTimes(1);
        expect(store.getSnapshot().account.drafts[destination.key]).toBe(draft);
    });
    it('blocks duplicate forwards and ignores successful responses after disconnect', async () => {
        const { store, transport, destination } = transferSetup();
        await store.connect();
        let resolve!: (response: DebugCallResponse) => void;
        transport.call.mockImplementationOnce(
            () =>
                new Promise((done) => {
                    resolve = done;
                }),
        );
        const first = store.forward(['group:12/2'], destination);
        const rejected = expect(first).rejects.toMatchObject({ uncertain: true });
        await expect(store.forward(['group:12/2'], destination)).rejects.toThrow('仍在发送');
        await store.disconnect();
        resolve(ok({ message_id: 77 }));
        await rejected;
        expect(store.getSnapshot().account.messages.at(-1)).toMatchObject({
            status: 'unknown',
        });
        expect(store.getSnapshot().account.messages.at(-1)?.id).toBeUndefined();
        expect(transport.call).toHaveBeenCalledTimes(1);
    });
    it('restores an archived search hit with nearby messages and a detached reading anchor', () => {
        const { store } = transferSetup();
        const seed = store.getSnapshot().account.messages[0];
        const messages: Message[] = Array.from({ length: 500 }, (_, index) => ({
            ...seed,
            key: `group:12/${index + 1}`,
            id: String(index + 1),
            at: index,
        }));
        store.getSnapshot().account = {
            ...store.getSnapshot().account,
            messages: messages.slice(-50),
            archiveMessages: messages,
        };
        const draft = store.getSnapshot().account.drafts;
        const hit = store.revealArchivedMessage(messages[10]);
        expect(hit?.key).toBe('group:12/11');
        expect(store.getSnapshot().account.active).toBe('group:12');
        expect(
            store.getSnapshot().account.messages.some((message) => message.key === hit?.key),
        ).toBe(true);
        expect(store.initialReadingPosition('group:12')).toMatchObject({
            messageKey: 'group:12/11',
            atBottom: false,
        });
        expect(store.getSnapshot().account.drafts).toBe(draft);
        expect(store.revealArchivedMessage('another-account/unknown')).toBeNull();
    });
    it('pages before a restored archive window and keeps a newer search anchor through a late history response', async () => {
        const { store, transport } = transferSetup();
        const seed = store.getSnapshot().account.messages[0];
        const saved: Message[] = Array.from({ length: 1000 }, (_, index) => ({
            ...seed,
            key: `group:12/${index + 1}`,
            id: String(index + 1),
            at: (index + 1) * 1000,
        }));
        store.getSnapshot().account = {
            ...store.getSnapshot().account,
            active: 'group:12',
            messages: saved.slice(-50),
            archiveMessages: saved,
        };
        await store.connect();
        const page = (start: number) => ({
            messages: Array.from({ length: 50 }, (_, index) => ({
                message_id: start + index,
                time: start + index,
                user_id: 99,
                message: `消息 ${start + index}`,
            })),
        });
        transport.call.mockResolvedValueOnce(ok(page(951)));
        await store.history('group:12');
        store.revealArchivedMessage('group:12/501');
        let resolve!: (response: DebugCallResponse) => void;
        transport.call.mockImplementationOnce(
            () =>
                new Promise((done) => {
                    resolve = done;
                }),
        );
        const earlier = store.history('group:12');
        expect(transport.call.mock.calls[1][2]).toMatchObject({ message_seq: '451' });
        store.revealArchivedMessage('group:12/101');
        resolve(ok(page(401)));
        await earlier;
        expect(store.getSnapshot().account.messages.some((message) => message.id === '101')).toBe(
            true,
        );
        expect(store.initialReadingPosition('group:12')?.messageKey).toBe('group:12/101');
        const oldest = store
            .getSnapshot()
            .account.messages.find((message) => message.session === 'group:12');
        expect(Number(oldest?.id)).toBeLessThanOrEqual(101);
        transport.call.mockResolvedValueOnce(ok({ messages: [] }));
        await store.history('group:12');
        expect(transport.call.mock.calls[2][2]).toMatchObject({ message_seq: oldest?.id });
    });
});
