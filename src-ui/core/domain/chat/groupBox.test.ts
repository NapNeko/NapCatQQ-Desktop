import { describe, expect, it } from 'vitest';
import { groupBoxSummary, conversationRows } from './groupBox';
import type { Conversation } from './model';
const c = (id: string, boxed: boolean, unread = 0): Conversation => ({
    key: `group:${id}`,
    id,
    type: 'group',
    name: `群${id}`,
    boxed,
    unread,
    pinned: false,
    lastAt: Number(id),
    preview: '消息',
});
describe('group message box', () => {
    const conversations = [c('1', true, 3), c('2', false, 2), c('3', true, 4)];
    it('groups all groups by default, including previously unboxed archived conversations', () => {
        expect(groupBoxSummary(conversations)).toMatchObject({
            count: 3,
            unread: 9,
            latest: { id: '3' },
        });
        expect(conversationRows(conversations, { box: false, unread: false, query: '' })).toEqual(
            [],
        );
        expect(
            conversationRows(conversations, { box: true, unread: false, query: '' }).map(
                (c) => c.id,
            ),
        ).toEqual(['3', '2', '1']);
    });
    it('leaves private conversations in the main list even if old metadata marks them boxed', () => {
        const friend: Conversation = {
            ...c('4', true),
            type: 'private',
            key: 'private:4',
            name: '朋友',
        };
        expect(
            conversationRows([...conversations, friend], { box: false, unread: false, query: '' }),
        ).toEqual([friend]);
    });
    it('searches across the box from the main list and respects unread filtering', () => {
        expect(
            conversationRows(conversations, { box: false, unread: true, query: '群1' }).map(
                (c) => c.id,
            ),
        ).toEqual(['1']);
        expect(conversationRows([c('1', true)], { box: true, unread: true, query: '' })).toEqual(
            [],
        );
    });
});
