import { describe, expect, it } from 'vitest';
import { emptyAccount, ingestMessage, openConversation } from './model';
import { archiveOf, restoreArchive, mergeRecentConversations } from './archive';
import type { Segment } from '../debug/segments';

const event = (id = 1, time = 100) => ({
    message_type: 'group',
    group_id: 12,
    user_id: 22,
    message_id: id,
    time,
    message: '你好',
});
describe('chat archive projection', () => {
    it('cleans nested self-forwarded image sources while retaining protocol and recovery metadata', () => {
        const content = [
            {
                type: 'node',
                data: {
                    message_id: '70',
                    user_id: '99',
                    nickname: '我',
                    content: [
                        {
                            type: 'image',
                            data: {
                                file: 'ncd-inline-image://private',
                                inline_ref: 'ncd-inline-image://private',
                                local_file: 'base64://' + 'A'.repeat(64 * 1024),
                                source_message_id: '70',
                                source_image_index: 0,
                                width: 800,
                                height: 400,
                            },
                        },
                        {
                            type: 'forward',
                            data: {
                                id: 'nested-resource',
                                messages: [
                                    {
                                        message_id: '71',
                                        sender: { user_id: '22', nickname: '朋友' },
                                        message: [
                                            {
                                                type: 'image',
                                                data: {
                                                    file: 'protocol-image',
                                                    local_file: 'ncd-local-file://D:/private.png',
                                                    url: 'https://cdn.example/recoverable.png',
                                                    base64: 'R0lGODlh',
                                                    source_message_id: '71',
                                                    source_image_index: 1,
                                                },
                                            },
                                        ],
                                    },
                                ],
                            },
                        },
                        {
                            type: 'image',
                            data: {
                                file: 'ncd-local-file://D:/temporary.png',
                                url: 'blob:private',
                            },
                        },
                    ],
                },
            },
        ];
        const state = ingestMessage(emptyAccount('99'), {
            ...event(),
            message: [{ type: 'forward', data: { id: 'root-resource', content } }],
        });
        const saved = archiveOf(state);
        const serialized = JSON.stringify(saved);
        expect(serialized).not.toMatch(
            /ncd-inline-image|ncd-local-file|base64|blob:|inline_ref|local_file/,
        );
        expect(serialized.length).toBeLessThan(2048);
        const root = saved.messages[0].segments[0];
        const node = (root.data.content as typeof content)[0];
        expect(node.data).toMatchObject({ message_id: '70', user_id: '99', nickname: '我' });
        expect(node.data.content[0]).toEqual({
            type: 'image',
            data: { source_message_id: '70', source_image_index: 0, width: 800, height: 400 },
        });
        expect(serialized).toContain('protocol-image');
        expect(serialized).toContain('https://cdn.example/recoverable.png');
        expect(serialized).toContain('source_image_index');
        expect(content[0].data.content[0].data.local_file).toContain('base64://');
    });
    it('bounds nested forward archive traversal without persisting an unvisited byte payload', () => {
        let segment: Segment = { type: 'image', data: { file: 'base64://R0lGODlh' } };
        for (let depth = 0; depth < 20; depth++) {
            segment = {
                type: 'forward',
                data: {
                    id: `resource-${depth}`,
                    content: [
                        {
                            type: 'node',
                            data: { message_id: `${depth + 1}`, content: [segment] },
                        },
                    ],
                },
            };
        }
        const state = ingestMessage(emptyAccount('99'), { ...event(), message: [segment] });
        const saved = JSON.stringify(archiveOf(state));
        expect(saved).not.toContain('base64://');
        expect(saved).toContain('resource-19');
        expect(saved).not.toContain('resource-0');
    });
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
