import { describe, expect, it } from 'vitest';
import {
    markdownContent,
    markdownKeyboard,
    markdownPlainText,
    stripQQMarkdownMetadata,
} from './markdown';
import { segmentPreview } from './segments';

const metadata = '[](%7B%22version%22%3A2%7D)';

describe('QQ markdown envelope', () => {
    it('removes only the recognized leading version envelope from nested protocol bodies', () => {
        const content = `${metadata}\n# 标题\n[文档](https://example.com/docs)`;
        expect(markdownContent({ data: JSON.stringify({ markdown: { content } }) })).toBe(
            '# 标题\n[文档](https://example.com/docs)',
        );
        expect(markdownPlainText(content)).toBe('标题 文档');
    });
    it('preserves ordinary links, malformed metadata and identical text in code', () => {
        for (const content of [
            '[](https://example.com)',
            '[](%INVALID)',
            '[](%7B%22version%22%3A99%7D)',
            `文字 ${metadata}`,
            `\`${metadata}\``,
            `\`\`\`text\n${metadata}\n\`\`\``,
        ])
            expect(stripQQMarkdownMetadata(content)).toBe(content);
    });
});

describe('QQ inline keyboard protocol', () => {
    it('normalizes SnowLuma flattened buttons while preserving precise app IDs and action flags', () => {
        const keyboard = markdownKeyboard({
            bot_appid: '9007199254740993',
            rows: [
                {
                    buttons: [
                        {
                            id: 'run',
                            label: '运行',
                            visited_label: '已发送',
                            style: 1,
                            type: 2,
                            data: '/status',
                            permission_type: 2,
                            enter: true,
                            is_reply: true,
                        },
                    ],
                },
            ],
        });
        expect(keyboard).toMatchObject({
            botAppid: '9007199254740993',
            rows: [
                [
                    {
                        id: 'run',
                        label: '运行',
                        visitedLabel: '已发送',
                        primary: true,
                        permissionType: 2,
                        action: { kind: 'command', text: '/status', enter: true, reply: true },
                    },
                ],
            ],
        });
    });
    it('supports the official nested keyboard schema and serialised wrappers', () => {
        const keyboard = markdownKeyboard({
            data: JSON.stringify({
                keyboard: {
                    content: {
                        rows: [
                            {
                                buttons: [
                                    {
                                        id: 'site',
                                        render_data: { label: '文档', style: 0 },
                                        action: {
                                            type: 0,
                                            data: 'https://example.com/docs',
                                            permission: { type: 0, specify_user_ids: ['99'] },
                                        },
                                    },
                                ],
                            },
                        ],
                    },
                },
            }),
        });
        expect(keyboard?.rows[0][0]).toMatchObject({
            label: '文档',
            permissionType: 0,
            permittedUsers: ['99'],
            action: { kind: 'link', url: 'https://example.com/docs' },
        });
    });
    it('distinguishes callbacks and unsupported native actions instead of sending their data as text', () => {
        const buttons = markdownKeyboard({
            rows: [
                {
                    buttons: [
                        { label: '回调', type: 1, data: 'opaque', permission_type: 2 },
                        { label: '小程序', type: 0, data: 'mqqapi://app/open', permission_type: 2 },
                        { label: '选图', type: 2, data: '图片', anchor: 1, permission_type: 2 },
                    ],
                },
            ],
        })?.rows[0];
        expect(buttons?.map((button) => button.action.kind)).toEqual([
            'callback',
            'unsupported',
            'unsupported',
        ]);
    });
    it('bounds the documented row and button limits and tolerates malformed payloads', () => {
        expect(markdownKeyboard({ data: '{broken' })).toBeNull();
        expect(markdownKeyboard({ rows: [{ buttons: [null, {}, 2] }] })).toBeNull();
        const rows = Array.from({ length: 8 }, () => ({
            buttons: Array.from({ length: 8 }, () => ({
                label: '按钮',
                type: 2,
                data: '/help',
                permission_type: 2,
            })),
        }));
        const keyboard = markdownKeyboard({ rows });
        expect(keyboard?.rows).toHaveLength(5);
        expect(keyboard?.rows.every((row) => row.length === 5)).toBe(true);
    });
    it('previews button labels instead of exposing an unknown segment placeholder', () => {
        expect(
            segmentPreview({
                type: 'inline_keyboard',
                data: {
                    rows: [
                        {
                            buttons: [
                                { label: '上一页', type: 1, permission_type: 2 },
                                { label: '下一页', type: 1, permission_type: 2 },
                            ],
                        },
                    ],
                },
            }),
        ).toBe(' [按钮] 上一页 · 下一页');
    });
});
