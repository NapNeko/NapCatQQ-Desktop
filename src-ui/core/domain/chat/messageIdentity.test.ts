import { describe, expect, it } from 'vitest';
import { emptyAccount, ingestMessage, type Message } from './model';
import { archiveOf, restoreArchive } from './archive';
import { deduplicateMessages } from './messageIdentity';

const event = (message_id: number, message_seq: number, extra = {}) => ({
    message_type: 'private',
    user_id: 22,
    message_id,
    message_seq,
    time: 100,
    message: '相同的正文',
    ...extra,
});
describe('message identity across historical API versions', () => {
    it.each([true, false])(
        'merges legacy id-as-sequence with the canonical event in either order: %s',
        (legacyFirst) => {
            const legacy = event(996932123, 996932123);
            const canonical = event(-1765286255, 63186);
            let state = ingestMessage(emptyAccount('99'), legacyFirst ? legacy : canonical);
            const key = state.messages[0].key;
            state = ingestMessage(state, legacyFirst ? canonical : legacy, true);
            expect(state.messages).toHaveLength(1);
            expect(state.messages[0]).toMatchObject({ key, id: '-1765286255', sequence: '63186' });
            expect(state.conversations['private:22'].unread).toBe(1);
            expect(ingestMessage(state, legacy).messages).toHaveLength(1);
            expect(ingestMessage(state, canonical).messages).toHaveLength(1);
        },
    );
    it('repairs previously persisted duplicates when restoring the archive', () => {
        const state = ingestMessage(emptyAccount('99'), event(1234, 1234));
        const saved = archiveOf(state);
        saved.messages.push(ingestMessage(emptyAccount('99'), event(-9876, 48)).messages[0]);
        const restored = restoreArchive(emptyAccount('99'), saved);
        expect(restored.messages).toHaveLength(1);
        expect(restored.messages[0]).toMatchObject({ id: '-9876', sequence: '48' });
    });
    it('retains genuine repeated sends in the same second with different valid sequences', () => {
        let state = ingestMessage(emptyAccount('99'), event(1234, 48));
        state = ingestMessage(state, event(5678, 49));
        expect(deduplicateMessages(state.messages)).toHaveLength(2);
        expect(state.conversations['private:22'].unread).toBe(2);
    });
    it.each([
        { time: 101 },
        { user_id: 23 },
        { message: '不同正文' },
        { message_type: 'group', group_id: 22 },
    ])('does not merge a different envelope or body: %j', (extra) => {
        let state = ingestMessage(emptyAccount('99'), event(1234, 1234));
        state = ingestMessage(state, event(-9876, 48, extra));
        expect(deduplicateMessages(state.messages)).toHaveLength(2);
    });
    it('keeps ambiguous same-second repetitions instead of guessing which one was replayed', () => {
        const rows = [event(1234, 1234), event(5678, 5678), event(-9876, 48)].map(
            (raw) => ingestMessage(emptyAccount('99'), raw).messages[0],
        );
        expect(deduplicateMessages(rows)).toHaveLength(3);
    });
    it('recognizes the same image digest despite URL refresh and extra history metadata', () => {
        const first = {
            type: 'image',
            data: {
                file: 'AAFFA78D0AC4088F7C30F5D1132B9CD8.jpg',
                url: 'https://cdn.example/old',
                summary: '',
            },
        };
        const second = {
            type: 'image',
            data: { ...first.data, url: 'https://cdn.example/new', file_size: 1234 },
        };
        let state = ingestMessage(emptyAccount('99'), event(1234, 1234, { message: [first] }));
        state = ingestMessage(state, event(-9876, 48, { message: [second] }));
        expect(state.messages).toHaveLength(1);
        expect(state.messages[0].segments).toEqual([second]);
    });
    it('preserves the optimistic row key and request id when canonical identity arrives', () => {
        const message: Message = {
            ...ingestMessage(emptyAccount('99'), event(1234, 1234)).messages[0],
            key: 'pending/request',
            requestId: 'request',
        };
        const canonical = {
            ...message,
            id: '-9876',
            sequence: '48',
            key: 'private:22/-9876',
            requestId: undefined,
        };
        expect(deduplicateMessages([message, canonical])[0]).toMatchObject({
            key: 'pending/request',
            requestId: 'request',
            id: '-9876',
            sequence: '48',
        });
    });
});
