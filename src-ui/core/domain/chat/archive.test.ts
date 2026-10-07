import { describe, expect, it } from 'vitest';
import { emptyAccount, ingestMessage, openConversation } from './model';
import { archiveOf, restoreArchive, mergeRecentConversations } from './archive';

const event = (id = 1, time = 100) => ({
    message_type: 'group',
    group_id: 12,
    user_id: 22,
    message_id: id,
    time,
    message: '你好',
});
describe('chat archive projection', () => {
    it('persists recoverable protocol ids but never process-local image refs or inline bytes', () => {
        const state = ingestMessage(emptyAccount('99'), {
            ...event(),
            message: [
                {
                    type: 'image',
                    data: {
                        file: 'ncd-inline-image://private',
                        inline_ref: 'ncd-inline-image://private',
                        local_file: 'base64://aGVsbG8=',
                        url: 'https://cdn.example/image',
                        source_message_id: '1',
                    },
                },
            ],
        });
        expect(archiveOf(state).messages[0].segments).toEqual([
            {
                type: 'image',
                data: { url: 'https://cdn.example/image', source_message_id: '1' },
            },
        ]);
        expect(state.messages[0].segments[0].data.inline_ref).toBe('ncd-inline-image://private');
    });
    it('restores conversations, unread, pins and box membership without drafts or event cursors', () => {
        const state = ingestMessage(emptyAccount('99'), event());
        state.conversations['group:12'] = {
            ...state.conversations['group:12'],
            pinned: true,
            boxed: true,
        };
        state.drafts['group:12'] = { text: '不落盘', attachments: [], reply: null };
        state.lastSeq = 77;
        const saved = archiveOf(state);
        const restored = restoreArchive(emptyAccount('99'), saved);
        expect(restored.conversations['group:12']).toMatchObject({
            unread: 1,
            pinned: true,
            boxed: true,
        });
        expect(restored.messages).toHaveLength(1);
        expect(restored.drafts).toEqual({});
        expect(restored.lastSeq).toBe(0);
    });
    it('merges live events received during hydration without duplicates or stale read status', () => {
        const archived = archiveOf(ingestMessage(emptyAccount('99'), event()));
        let live = ingestMessage(emptyAccount('99'), event(2, 200));
        live = openConversation(live, live.conversations['group:12']);
        const restored = restoreArchive(live, archived);
        expect(restored.messages.map((m) => m.id)).toEqual(['1', '2']);
        expect(restored.conversations['group:12']).toMatchObject({ unread: 0, lastAt: 200000 });
    });
    it('refuses another identity and preserves unknown send outcomes', () => {
        const archived = archiveOf(ingestMessage(emptyAccount('99'), event()));
        archived.messages[0].status = 'sending';
        expect(() => restoreArchive(emptyAccount('100'), archived)).toThrow();
        expect(restoreArchive(emptyAccount('99'), archived).messages[0].status).toBe('unknown');
    });
    it('refreshes saved Markdown previews while preserving recall wording', () => {
        const state = ingestMessage(emptyAccount('99'), {
            ...event(),
            message: [{ type: 'markdown', data: { data: { content: '# 标题\n\n**正文**' } } }],
        });
        const archived = archiveOf(state);
        archived.conversations[0].preview = '[Markdown]';
        expect(restoreArchive(emptyAccount('99'), archived).conversations['group:12'].preview).toBe(
            '标题 正文',
        );
        archived.messages[0].recalled = true;
        archived.conversations[0].preview = '消息已撤回';
        expect(restoreArchive(emptyAccount('99'), archived).conversations['group:12'].preview).toBe(
            '消息已撤回',
        );
    });
    it('imports recent contacts without marking old messages unread or moving the active conversation', () => {
        let state = ingestMessage(emptyAccount('99'), event());
        state = openConversation(state, state.conversations['group:12']);
        const next = mergeRecentConversations(state, [
            {
                chatType: 1,
                peerUin: '88',
                peerName: '朋友',
                msgTime: '123',
                lastestMsg: {
                    message_type: 'private',
                    user_id: 88,
                    message_id: 5,
                    message: '上次的消息',
                    time: 123,
                },
            },
            { chatType: 2, peerUin: '12', peerName: '开发群', msgTime: '50', lastestMsg: {} },
        ]);
        expect(next.active).toBe('group:12');
        expect(next.conversations['private:88']).toMatchObject({
            name: '朋友',
            preview: '上次的消息',
            unread: 0,
        });
        expect(next.conversations['group:12']).toMatchObject({ lastAt: 100000, preview: '你好' });
    });
});
