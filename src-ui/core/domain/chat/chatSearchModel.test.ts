import { describe, expect, it } from 'vitest';
import type { Message } from './model';
import {
    buildSearchPattern,
    buildSearchableMessages,
    collectSenders,
    countConversations,
    dateGroup,
    filterMatches,
    inCategory,
    mergeSearchMessages,
    splitHighlight,
} from './chatSearchModel';

function message(overrides: Partial<Message> & Pick<Message, 'key'>): Message {
    return {
        session: 'group:12',
        senderId: '88',
        senderName: '小明',
        at: 1000,
        mine: false,
        segments: [{ type: 'text', data: { text: '你好' } }],
        status: 'unknown',
        ...overrides,
    };
}

describe('search message merge', () => {
    it('sorts merged messages by time and keeps one entry per session key', () => {
        const merged = mergeSearchMessages(
            [message({ key: 'b', at: 20 })],
            [message({ key: 'a', at: 10 })],
        );
        expect(merged.map((item) => item.key)).toEqual(['a', 'b']);
    });
    it('deduplicates archived and loaded copies via message id', () => {
        const merged = mergeSearchMessages(
            [message({ key: 'archived', id: '7', at: 10 })],
            [message({ key: 'loaded', id: '7', at: 10 })],
        );
        expect(merged).toHaveLength(1);
        expect(merged[0].key).toBe('loaded');
    });
    it('keeps recalled tombstones when late history revives the same id', () => {
        const merged = mergeSearchMessages(
            [
                message({
                    key: 'archived',
                    id: '7',
                    at: 10,
                    recalled: true,
                    segments: [{ type: 'text', data: { text: '占位' } }],
                }),
            ],
            [message({ key: 'late', id: '7', at: 10 })],
        );
        expect(merged).toHaveLength(1);
        expect(merged[0].recalled).toBe(true);
        expect(merged[0].segments).toEqual([{ type: 'text', data: { text: '占位' } }]);
    });
});

describe('search category filter', () => {
    it('splits images into media and emoji by sub type', () => {
        const photo = message({ key: 'p', segments: [{ type: 'image', data: {} }] });
        const sticker = message({
            key: 's',
            segments: [{ type: 'image', data: { sub_type: 1 } }],
        });
        expect(inCategory(photo, 'media')).toBe(true);
        expect(inCategory(sticker, 'media')).toBe(false);
        expect(inCategory(sticker, 'emoji')).toBe(true);
    });
    it('detects links in text and markdown segments', () => {
        const text = message({
            key: 't',
            segments: [{ type: 'text', data: { text: '见 https://example.com' } }],
        });
        const md = message({
            key: 'm',
            segments: [{ type: 'markdown', data: { content: '[点我](http://example.com/a)' } }],
        });
        expect(inCategory(text, 'link')).toBe(true);
        expect(inCategory(md, 'link')).toBe(true);
        expect(inCategory(text, 'file')).toBe(false);
    });
    it('matches everything under all', () => {
        expect(inCategory(message({ key: 'x' }), 'all')).toBe(true);
    });
});

describe('search date group', () => {
    it('formats local calendar day with zero padding', () => {
        const at = new Date(2026, 9, 3, 9, 5).getTime();
        expect(dateGroup(at)).toBe('2026/10/03');
    });
});

describe('search pattern', () => {
    it('escapes regex metacharacters and matches case-insensitively', () => {
        const pattern = buildSearchPattern('A.*[B]');
        expect(pattern?.test('a.*[b]')).toBe(true);
        expect(pattern?.test('axxxx[B]')).toBe(false);
        expect(buildSearchPattern('')).toBeNull();
    });
});

describe('searchable index', () => {
    it('falls back to a derived conversation label when the contact is unknown', () => {
        const [item] = buildSearchableMessages([message({ key: 'a', session: 'group:12' })]);
        expect(item.preview).toBe('你好');
        const [priv] = buildSearchableMessages([message({ key: 'b', session: 'private:9' })]);
        expect(priv.conversation).toBe('私聊 9');
    });
    it('uses the contact name, counts sessions and dedupes senders with mine label', () => {
        const searchable = buildSearchableMessages(
            [
                message({ key: 'c', session: 'private:9', senderId: '88' }),
                message({ key: 'b', senderId: '99', senderName: '小红' }),
                message({ key: 'a', mine: true }),
            ],
            { 'group:12': { key: 'group:12', type: 'group', id: '12', name: '群' } },
        );
        expect(searchable[1].conversation).toBe('群');
        expect(countConversations(searchable)).toBe(2);
        expect(collectSenders(searchable)).toEqual([
            ['88', '我'],
            ['99', '小红'],
        ]);
    });
});

describe('search matching', () => {
    function items() {
        return buildSearchableMessages([
            message({
                key: 'a',
                at: new Date(2026, 9, 1, 12).getTime(),
                segments: [{ type: 'text', data: { text: 'Hello' } }],
            }),
            message({
                key: 'b',
                at: new Date(2026, 9, 2, 12).getTime(),
                segments: [{ type: 'text', data: { text: 'hello world' } }],
            }),
            message({
                key: 'c',
                at: new Date(2026, 9, 3, 12).getTime(),
                senderId: '99',
                segments: [{ type: 'text', data: { text: 'hello' } }],
            }),
        ]);
    }
    it('returns newest first and honours pattern, sender and date window', () => {
        expect(
            filterMatches(items(), {
                pattern: buildSearchPattern('hello'),
                category: 'all',
                sender: '',
                from: '',
                to: '',
            }).map(({ message: m }) => m.key),
        ).toEqual(['c', 'b', 'a']);
        expect(
            filterMatches(items(), {
                pattern: null,
                category: 'all',
                sender: '99',
                from: '',
                to: '',
            }).map(({ message: m }) => m.key),
        ).toEqual(['c']);
        expect(
            filterMatches(items(), {
                pattern: null,
                category: 'all',
                sender: '',
                from: '2026-10-02',
                to: '2026-10-02',
            }).map(({ message: m }) => m.key),
        ).toEqual(['b']);
    });
    it('filters media by category', () => {
        const media = buildSearchableMessages([
            message({ key: 'v', segments: [{ type: 'video', data: {} }] }),
            message({ key: 'f', segments: [{ type: 'file', data: { name: 'a.zip' } }] }),
        ]);
        expect(
            filterMatches(media, {
                pattern: null,
                category: 'media',
                sender: '',
                from: '',
                to: '',
            }).map(({ message: m }) => m.key),
        ).toEqual(['v']);
    });
});

describe('highlight split', () => {
    it('splits every case-insensitive match and keeps the tail', () => {
        const parts = splitHighlight('aHello b HELLO c', buildSearchPattern('hello')!);
        expect(
            parts
                .filter((p) => p.hit)
                .map((p) => p.text)
                .filter(Boolean),
        ).toEqual(['Hello', 'HELLO']);
        expect(parts.map((p) => p.text).join('')).toBe('aHello b HELLO c');
        expect(parts[parts.length - 1].hit).toBe(false);
    });
    it('returns the whole text as one plain part without matches', () => {
        const parts = splitHighlight('none', buildSearchPattern('zz')!);
        expect(parts).toEqual([{ text: 'none', start: 0, hit: false }]);
    });
});
