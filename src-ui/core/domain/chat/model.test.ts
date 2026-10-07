import { describe, expect, it } from 'vitest';
import {
    accountKey,
    emptyAccount,
    ingestMessage,
    ingestMessages,
    retainMessages,
    openConversation,
    settleSend,
    addPending,
    parseContact,
    setDraft,
    mergeArchiveMessages,
} from './model';
import { WORKING_BYTE_LIMIT, messageBytes } from './messageWorkingSet';

const payload = (extra = {}) => ({
    post_type: 'message',
    message_type: 'group',
    group_id: 12,
    user_id: 22,
    message_id: 7,
    time: 100,
    sender: { nickname: '小林' },
    message: [{ type: 'text', data: { text: '你好' } }],
    ...extra,
});
describe('native chat projection', () => {
    it('does not replay evicted idless messages or notices into unread counts', () => {
        const rows = [
            payload({ message_id: undefined, message_seq: '81' }),
            { notice_type: 'group_increase', group_id: 12, user_id: 44, time: 101 },
            {
                notice_type: 'notify',
                sub_type: 'poke',
                group_id: 12,
                sender_id: 22,
                target_id: 44,
                time: 102,
            },
        ];
        let state = ingestMessages(emptyAccount('99'), rows);
        const unread = state.conversations['group:12'].unread;
        const count = state.archiveMessages!.length;
        state = { ...state, messages: [] };
        state = ingestMessages(state, rows);
        expect(state.messages).toHaveLength(0);
        expect(state.archiveMessages).toHaveLength(count);
        expect(state.conversations['group:12'].unread).toBe(unread);
    });
    it('retains received content when recalled messages are replayed from history or another view', () => {
        let state = ingestMessage(emptyAccount('99'), payload());
        const original = state.messages[0].segments;
        state = ingestMessage(state, {
            notice_type: 'group_recall',
            group_id: 12,
            message_id: 7,
        });
        state.messages = [];
        state = ingestMessage(state, payload({ message: '消息已撤回' }), true);
        expect(state.messages[0]).toMatchObject({ recalled: true, segments: original });
        state = ingestMessage(state, payload({ message_seq: '999', message: '消息已撤回' }), true);
        expect(state.messages[0]).toMatchObject({ recalled: true, segments: original });
        expect(
            mergeArchiveMessages(state.archiveMessages!, [
                { ...state.messages[0], segments: [], recalled: false },
            ])[0],
        ).toMatchObject({ recalled: true, segments: original });
    });
    it('keeps a bounded older page and the recent page while loading history', () => {
        let state = ingestMessage(emptyAccount('99'), payload());
        const message = state.messages[0];
        state.messages = Array.from({ length: 5000 }, (_, i) => ({
            ...message,
            key: `group:12/${i + 100}`,
            id: String(i + 100),
            at: i + 1000,
        }));
        state = ingestMessage(state, payload({ message_id: 1, message_seq: '1', time: 0.5 }), true);
        expect(state.messages[0].id).toBe('1');
        expect(state.messages.length).toBeLessThanOrEqual(200);
        expect(state.archiveMessages).toHaveLength(5000);
        expect(state.messages.at(-1)?.id).toBe('5099');
    });
    it('scopes accounts by bot and signed-in identity', () => {
        expect(accountKey('a', '1')).not.toBe(accountKey('b', '1'));
        expect(accountKey('a', '1')).not.toBe(accountKey('a', '2'));
    });
    it('keeps recent messages from other conversations alongside the active older pages', () => {
        let state = ingestMessage(emptyAccount('99'), payload());
        const message = state.messages[0];
        state.active = 'group:12';
        state.messages = Array.from({ length: 5000 }, (_, i) => ({
            ...message,
            key: `group:12/${i + 100}`,
            id: String(i + 100),
            at: i + 1000,
        }));
        state = ingestMessage(state, payload({ group_id: 13, message_id: 9, time: 10 }));
        expect(state.messages.length).toBeLessThanOrEqual(200);
        expect(state.messages.some((row) => row.session === 'group:13')).toBe(true);
        state = ingestMessage(state, payload({ group_id: 13, message_id: 1, time: 0.5 }), true);
        expect(
            state.messages.filter((row) => row.session === 'group:12').length,
        ).toBeLessThanOrEqual(150);
        expect(state.messages.some((row) => row.session === 'group:13' && row.id === '1')).toBe(
            true,
        );
    });
    it('retains the reading anchor, a gap and live tail while bounding a busy active conversation', () => {
        const message = ingestMessage(emptyAccount('99'), payload()).messages[0];
        const messages = Array.from({ length: 20000 }, (_, index) => ({
            ...message,
            key: `group:12/${index}`,
            id: String(index),
            at: index,
        }));
        const retained = retainMessages(messages, 'group:12', null, {
            session: 'group:12',
            messageKey: 'group:12/500',
            messageId: '500',
            atBottom: false,
        });
        expect(retained).toHaveLength(200);
        expect(retained.some((row) => row.id === '500')).toBe(true);
        expect(retained.at(-1)?.id).toBe('19999');
        expect(retained.filter((row) => row.gapBefore)).toHaveLength(1);
        expect(retainMessages(retained, 'private:88').length).toBeLessThanOrEqual(50);
    });
    it('bounds variable message content and keeps batches sorted and unread only once', () => {
        let state = emptyAccount('99');
        state.active = 'group:12';
        state = ingestMessages(
            state,
            Array.from({ length: 300 }, (_, index) =>
                payload({
                    message_id: index + 1,
                    time: 300 - index,
                    message: '长消息'.repeat(20000),
                }),
            ),
        );
        expect(
            state.messages.reduce((bytes, message) => bytes + messageBytes(message), 0),
        ).toBeLessThanOrEqual(WORKING_BYTE_LIMIT);
        expect(state.messages.map((row) => row.at)).toEqual(
            [...state.messages.map((row) => row.at)].sort((a, b) => a - b),
        );
        expect(state.conversations['group:12'].unread).toBe(300);
        const replay = ingestMessages(state, [
            payload({ message_id: 1, time: 300 }),
            payload({ message_id: 1, time: 300 }),
        ]);
        expect(replay.conversations['group:12'].unread).toBe(300);
    });
    it('deduplicates replay without increasing unread', () => {
        const first = ingestMessage(emptyAccount('99'), payload());
        const replay = ingestMessage(first, payload());
        expect(replay.messages).toHaveLength(1);
        expect(replay.conversations['group:12'].unread).toBe(1);
    });
    it('does not deduplicate message ids across conversations', () => {
        const first = ingestMessage(emptyAccount('99'), payload());
        expect(ingestMessage(first, payload({ group_id: 13 })).messages).toHaveLength(2);
    });
    it('keeps historical messages from marking a conversation unread', () => {
        const state = ingestMessage(emptyAccount('99'), payload(), true);
        expect(state.conversations['group:12'].unread).toBe(0);
    });
    it('uses the recipient for outgoing private events', () => {
        const state = ingestMessage(
            emptyAccount('99'),
            payload({
                post_type: 'message_sent',
                message_type: 'private',
                user_id: 99,
                target_id: 88,
            }),
        );
        expect(state.messages[0].session).toBe('private:88');
        expect(state.messages[0].mine).toBe(true);
    });
    it('reconciles an echo arriving before the send result', () => {
        let state = addPending(
            emptyAccount('99'),
            'group:12',
            'req',
            [{ type: 'text', data: { text: '你好' } }],
            1,
        );
        state = ingestMessage(state, payload({ user_id: 99 }));
        state = settleSend(state, 'req', { state: 'sent', id: '7' });
        expect(state.messages).toHaveLength(1);
        expect(state.messages[0].id).toBe('7');
        expect(state.messages[0].status).toBe('sent');
    });
    it('keeps unconfirmed sends and never manufactures success', () => {
        const state = addPending(emptyAccount('99'), 'group:12', 'req', [], 1);
        expect(
            settleSend(state, 'req', { state: 'unknown', error: '超时' }).messages[0].status,
        ).toBe('unknown');
    });
    it('replaces local attachment placeholders with the authoritative echo', () => {
        let state = addPending(
            emptyAccount('99'),
            'group:12',
            'req',
            [{ type: 'image', data: { file: 'ncd-local-file://image.png' } }],
            1,
        );
        state = settleSend(state, 'req', { state: 'sent', id: '7' });
        state = ingestMessage(
            state,
            payload({
                user_id: 99,
                message: [{ type: 'image', data: { url: 'https://example.com/image.png' } }],
            }),
        );
        expect(state.messages).toHaveLength(1);
        expect(state.messages[0].segments[0].data.url).toBe('https://example.com/image.png');
    });
    it('preserves string ids without a lossy number conversion', () => {
        expect(
            parseContact({ group_id: '9007199254740993', group_name: '开发群' }, 'group')?.id,
        ).toBe('9007199254740993');
    });
    it.each([
        { user_id: 22, group_id: 12, fileId: 'uploaded-file' },
        { user_id: 99, group_id: 13, fileId: 'uploaded-file' },
        { user_id: 99, group_id: 12, fileId: 'another-file' },
    ])('does not merge an unrelated file event: %j', (event) => {
        let state = addPending(
            emptyAccount('99'),
            'group:12',
            'req',
            [{ type: 'file', data: { name: 'same.txt' } }],
            1,
        );
        state = settleSend(state, 'req', { state: 'sent', fileId: 'uploaded-file' });
        state = ingestMessage(
            state,
            payload({
                user_id: event.user_id,
                group_id: event.group_id,
                message: [{ type: 'file', data: { file: 'same.txt', file_id: event.fileId } }],
            }),
        );
        expect(state.messages).toHaveLength(2);
        expect(state.messages.find((message) => message.requestId === 'req')?.id).toBeUndefined();
    });
    it('keeps drafts per conversation and marks only opened messages read', () => {
        let state = ingestMessage(emptyAccount('99'), payload());
        state = setDraft(state, 'group:12', { text: '草稿', attachments: [], reply: null });
        state = openConversation(state, {
            key: 'private:22',
            type: 'private',
            id: '22',
            name: '小林',
        });
        expect(state.drafts['group:12'].text).toBe('草稿');
        expect(state.conversations['group:12'].unread).toBe(1);
    });
});

const poke = (extra = {}) => ({
    post_type: 'notice',
    notice_type: 'notify',
    sub_type: 'poke',
    group_id: 12,
    user_id: 22,
    target_id: 99,
    time: 200,
    ...extra,
});
describe('poke notices', () => {
    it('renders an incoming poke as a system line with the sender name and bumps unread', () => {
        let state = ingestMessage(emptyAccount('99'), payload());
        state = ingestMessage(state, poke());
        const notice = state.messages[1];
        expect(notice.notice).toBe('小林拍了拍你');
        expect(notice.mine).toBe(false);
        expect(state.conversations['group:12'].unread).toBe(2);
        expect(state.conversations['group:12'].preview).toBe('小林拍了拍你');
    });
    it('describes an outgoing poke as 你拍了拍… without adding unread', () => {
        let state = ingestMessage(emptyAccount('99'), payload());
        state = ingestMessage(state, poke({ user_id: 99, target_id: 22 }));
        const notice = state.messages[1];
        expect(notice.notice).toBe('你拍了拍小林');
        expect(notice.mine).toBe(true);
        expect(state.conversations['group:12'].unread).toBe(1);
    });
    it('skips the backend echo of a locally sent poke inside the time window', () => {
        let state = ingestMessage(emptyAccount('99'), payload());
        state = ingestMessage(state, poke({ user_id: 99, target_id: 22, time: 200 }));
        state = ingestMessage(state, poke({ user_id: 99, target_id: 22, time: 202 }));
        expect(state.messages.filter((m) => m.notice)).toHaveLength(1);
        state = ingestMessage(state, poke({ user_id: 99, target_id: 22, time: 300 }));
        expect(state.messages.filter((m) => m.notice)).toHaveLength(2);
    });
    it('routes private pokes to the peer session and falls back to raw ids', () => {
        const state = ingestMessage(emptyAccount('99'), {
            post_type: 'notice',
            notice_type: 'notify',
            sub_type: 'poke',
            user_id: 88,
            target_id: 99,
            time: 200,
        });
        expect(state.messages[0].session).toBe('private:88');
        expect(state.messages[0].notice).toBe('88拍了拍你');
    });
    it('ignores poke notices without both ends', () => {
        const state = ingestMessage(emptyAccount('99'), {
            post_type: 'notice',
            notice_type: 'notify',
            sub_type: 'poke',
            group_id: 12,
            user_id: 22,
        });
        expect(state.messages).toHaveLength(0);
    });
});

describe('poke echo attribution', () => {
    it('trusts sender_id over user_id and dedupes the echo of a local poke', () => {
        let state = ingestMessage(emptyAccount('99'), payload());
        // poke() 成功后的乐观上墙
        state = ingestMessage(state, {
            post_type: 'notice',
            notice_type: 'notify',
            sub_type: 'poke',
            group_id: 12,
            user_id: 99,
            target_id: 22,
            time: 200,
        });
        // 后端回显：user_id 是被拍的人，sender_id 才是发起者
        state = ingestMessage(state, {
            post_type: 'notice',
            notice_type: 'notify',
            sub_type: 'poke',
            group_id: 12,
            user_id: 22,
            sender_id: 99,
            target_id: 22,
            time: 201,
        });
        const notices = state.messages.filter((m) => m.notice);
        expect(notices).toHaveLength(1);
        expect(notices[0].notice).toBe('你拍了拍小林');
    });
});

const join = (extra = {}) => ({
    post_type: 'notice',
    notice_type: 'group_increase',
    sub_type: 'approve',
    group_id: 12,
    user_id: 33,
    operator_id: 0,
    time: 200,
    ...extra,
});
describe('group join notices', () => {
    it('renders a join as a system line and bumps unread', () => {
        let state = ingestMessage(emptyAccount('99'), payload());
        state = ingestMessage(state, join());
        const notice = state.messages[1];
        expect(notice.notice).toBe('33加入了本群');
        expect(notice.session).toBe('group:12');
        expect(state.conversations['group:12'].unread).toBe(2);
        expect(state.conversations['group:12'].preview).toBe('33加入了本群');
    });
    it('uses a known sender name and credits the inviter on invite joins', () => {
        let state = ingestMessage(emptyAccount('99'), payload());
        state = ingestMessage(state, join({ sub_type: 'invite', user_id: 22, operator_id: 44 }));
        expect(state.messages[1].notice).toBe('44邀请小林加入了本群');
    });
    it('does not count the bot itself joining as unread', () => {
        let state = ingestMessage(emptyAccount('99'), payload());
        state = ingestMessage(state, join({ user_id: 99 }));
        const notice = state.messages[1];
        expect(notice.notice).toBe('你加入了本群');
        expect(notice.mine).toBe(true);
        expect(state.conversations['group:12'].unread).toBe(1);
    });
    it('deduplicates the same join replayed from history', () => {
        let state = ingestMessage(emptyAccount('99'), join());
        state = ingestMessage(state, join(), true);
        expect(state.messages).toHaveLength(1);
        expect(state.conversations['group:12'].unread).toBe(1);
    });
    it('ignores join notices without a group or user', () => {
        expect(
            ingestMessage(emptyAccount('99'), {
                post_type: 'notice',
                notice_type: 'group_increase',
                user_id: 33,
                time: 200,
            }).messages,
        ).toHaveLength(0);
    });
});

describe('local image source carry', () => {
    it('keeps the local file on image segments when the echo replaces a pending send', () => {
        let state = addPending(
            emptyAccount('99'),
            'group:12',
            'req',
            [
                {
                    type: 'image',
                    data: { file: 'ncd-local-file://C:/tmp/sent.png', name: 'sent.png' },
                },
            ],
            1,
        );
        state = settleSend(state, 'req', { state: 'sent', id: '7' });
        state = ingestMessage(
            state,
            payload({
                user_id: 99,
                message: [
                    {
                        type: 'image',
                        data: { file: 'nt-name.jpg', url: 'https://cdn.example/echo.png' },
                    },
                ],
            }),
        );
        expect(state.messages).toHaveLength(1);
        expect(state.messages[0].segments[0].data.local_file).toBe(
            'ncd-local-file://C:/tmp/sent.png',
        );
        expect(state.messages[0].segments[0].data.url).toBe('https://cdn.example/echo.png');
    });
    it('carries the local file when the send result merges an early echo', () => {
        let state = addPending(
            emptyAccount('99'),
            'group:12',
            'req',
            [
                {
                    type: 'image',
                    data: { file: 'ncd-local-file://C:/tmp/sent.png', name: 'sent.png' },
                },
            ],
            1,
        );
        state = ingestMessage(
            state,
            payload({
                user_id: 99,
                message: [
                    {
                        type: 'image',
                        data: { file: 'nt-name.jpg', url: 'https://cdn.example/echo.png' },
                    },
                ],
            }),
        );
        state = settleSend(state, 'req', { state: 'sent', id: '7' });
        expect(state.messages).toHaveLength(1);
        expect(state.messages[0].segments[0].data.local_file).toBe(
            'ncd-local-file://C:/tmp/sent.png',
        );
    });
    it('leaves received images untouched', () => {
        const state = ingestMessage(
            emptyAccount('99'),
            payload({
                message: [
                    {
                        type: 'image',
                        data: { file: 'nt-name.jpg', url: 'https://cdn.example/their.png' },
                    },
                ],
            }),
        );
        expect(state.messages[0].segments[0].data.local_file).toBeUndefined();
    });
});
