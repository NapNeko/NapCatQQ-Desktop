import { describe, expect, it, vi } from 'vitest';
import { ChatAccountStore } from './chatStore';
import { archiveOf } from '../../core/domain/chat/archive';
import { emptyAccount, ingestMessage, ingestMessages } from '../../core/domain/chat/model';
import type { ChatArchive } from '../../core/ipc/generated/chat/ChatArchive';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
const target: DebugTarget = {
    bot_id: 'bot',
    qq_id: 99,
    name: '测试',
    running: false,
    online: false,
    backend: 'napcat',
    host: { kind: 'local' },
};
const data = () =>
    archiveOf(
        ingestMessage(emptyAccount('99'), {
            message_type: 'group',
            group_id: 12,
            message_id: 1,
            user_id: 22,
            time: 100,
            message: '上次的聊天',
        }),
    );
const transport = () => ({ call: vi.fn(), subscribe: vi.fn(), unsubscribe: vi.fn() });
describe('chat archive lifecycle', () => {
    it('does not overwrite older archived messages when only recent pages remain in memory', async () => {
        const saved = archiveOf(
            ingestMessages(
                emptyAccount('99'),
                Array.from({ length: 600 }, (_, index) => ({
                    message_type: 'group',
                    group_id: 12,
                    user_id: 22,
                    message_id: index + 1,
                    time: index + 1,
                    message: '档案消息',
                })),
            ),
        );
        const archive = {
            load: vi.fn(async () => saved),
            save: vi.fn(async (_bot: string, _self: string, _saved: ChatArchive) => {}),
        };
        const store = new ChatAccountStore(target, transport(), archive);
        await store.restore();
        expect(store.getSnapshot().account.messages).toHaveLength(50);
        store.open({ key: 'group:12', type: 'group', id: '12', name: '群' });
        store.pin('group:12');
        await store.flushArchive();
        expect(archive.save.mock.calls.at(-1)?.[2].messages).toHaveLength(600);
        expect(archive.save.mock.calls.at(-1)?.[2].messages[0].id).toBe('1');
    });
    it('restores the list while offline and persists pins and box membership', async () => {
        const archive = { load: vi.fn(async () => data()), save: vi.fn(async () => {}) };
        const store = new ChatAccountStore(target, transport(), archive);
        await store.initialize();
        expect(store.getSnapshot().account.conversations['group:12'].preview).toBe('上次的聊天');
        store.pin('group:12');
        store.box('group:12');
        await store.flushArchive();
        expect(archive.save.mock.calls.at(-1)).toMatchObject([
            'bot',
            '99',
            { conversations: [{ pinned: true, boxed: true }] },
        ]);
    });
    it('does not overwrite unreadable history and offers a successful restore retry', async () => {
        const archive = {
            load: vi
                .fn<() => Promise<ChatArchive | null>>()
                .mockRejectedValueOnce(new Error('文件损坏'))
                .mockResolvedValue(data()),
            save: vi.fn(async () => {}),
        };
        const store = new ChatAccountStore(target, transport(), archive);
        await store.restore();
        store.open({ key: 'private:88', id: '88', type: 'private', name: '朋友' });
        await store.flushArchive();
        expect(archive.save).not.toHaveBeenCalled();
        expect(store.getSnapshot().archiveError).toContain('文件损坏');
        await store.restore();
        await store.flushArchive();
        expect(Object.keys(store.getSnapshot().account.conversations)).toEqual(
            expect.arrayContaining(['group:12', 'private:88']),
        );
        expect(store.getSnapshot().archiveError).toBe('');
    });
    it('serializes writes and keeps newer changes made during an in-flight save', async () => {
        let resolve!: () => void;
        const save = vi
            .fn<(...args: unknown[]) => Promise<void>>()
            .mockImplementationOnce(
                () =>
                    new Promise((done) => {
                        resolve = done;
                    }),
            )
            .mockResolvedValue(undefined);
        const archive = { load: vi.fn(async () => data()), save };
        const store = new ChatAccountStore(target, transport(), archive);
        await store.restore();
        store.box('group:12');
        store.pin('group:12');
        expect(save).toHaveBeenCalledTimes(1);
        resolve();
        await store.flushArchive();
        expect(save).toHaveBeenCalledTimes(2);
        expect(save.mock.calls[1][2]).toMatchObject({
            conversations: [{ boxed: true, pinned: true }],
        });
    });
});
