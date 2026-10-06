import { describe, expect, it } from 'vitest';
import {
    chatFrames,
    chatTimeLabel,
    parseChatFrame,
    splitDataUrl,
    timelineMarks,
} from './maibotChat';

const chat = (data: object) =>
    JSON.stringify({ op: 'event', domain: 'chat', event: 'x', session: 'desktop', data });

describe('parseChatFrame', () => {
    it('reads the envelope ops and ignores the rest', () => {
        expect(
            parseChatFrame(
                JSON.stringify({ op: 'event', domain: 'system', event: 'ready', data: {} }),
            ),
        ).toEqual({ op: 'ready' });
        expect(parseChatFrame(JSON.stringify({ op: 'pong', timestamp: 1 }))).toEqual({
            op: 'pong',
        });
        expect(
            parseChatFrame(JSON.stringify({ op: 'response', id: 'c1', ok: true, data: {} })),
        ).toEqual({
            op: 'response',
            id: 'c1',
            ok: true,
            error: undefined,
        });
        expect(
            parseChatFrame(
                JSON.stringify({
                    op: 'response',
                    id: 'c2',
                    ok: false,
                    error: { code: 'x', message: '找不到聊天会话' },
                }),
            ),
        ).toMatchObject({ ok: false, error: '找不到聊天会话' });
        expect(
            parseChatFrame(
                JSON.stringify({ op: 'event', domain: 'logs', event: 'snapshot', data: {} }),
            ),
        ).toBeNull();
        expect(parseChatFrame(chat({ type: 'virtual_identity_set' }))).toBeNull();
        expect(parseChatFrame('not json')).toBeNull();
        expect(parseChatFrame('[1,2]')).toBeNull();
    });

    it('turns history into messages, rich ones keep their segments', () => {
        const f = parseChatFrame(
            chat({
                type: 'history',
                messages: [
                    {
                        id: 'm1',
                        type: 'user',
                        content: '在吗',
                        timestamp: 100,
                        sender_name: '人类',
                        is_bot: false,
                        segments: null,
                    },
                    {
                        id: 'm2',
                        type: 'bot',
                        content: '[表情]',
                        timestamp: 101,
                        sender_name: '麦麦',
                        is_bot: true,
                        message_type: 'rich',
                        segments: [
                            { type: 'text', data: '在的' },
                            { type: 'emoji', data: 'data:image/gif;base64,R0lG' },
                            { type: 'image', data: '' },
                        ],
                    },
                ],
            }),
        );
        expect(f).toMatchObject({ op: 'chat', session: 'desktop', event: { kind: 'history' } });
        const messages = f?.op === 'chat' && f.event.kind === 'history' ? f.event.messages : [];
        expect(messages[0]).toEqual({
            id: 'm1',
            fromBot: false,
            senderName: '人类',
            segments: [{ type: 'text', text: '在吗' }],
            at: 100,
        });
        // 读不到的图上游给空，不画一张坏图
        expect(messages[1].segments).toEqual([
            { type: 'text', text: '在的' },
            { type: 'emoji', src: 'data:image/gif;base64,R0lG' },
        ]);
    });

    it('echoed user messages use the raw text and rebuild image data URLs', () => {
        const f = parseChatFrame(
            chat({
                type: 'user_message',
                content: '看这个\n[图片]',
                raw_content: '看这个',
                images: [{ name: 'a.png', mime_type: 'image/png', base64: 'iVBO' }],
                emojis: [],
                message_id: 'u1',
                timestamp: 200,
                sender: { name: '人类', user_id: 'webui_user_ncd_desktop', is_bot: false },
            }),
        );
        expect(f?.op === 'chat' && f.event).toEqual({
            kind: 'message',
            message: {
                id: 'u1',
                fromBot: false,
                senderName: '人类',
                segments: [
                    { type: 'text', text: '看这个' },
                    { type: 'image', src: 'data:image/png;base64,iVBO' },
                ],
                at: 200,
            },
        });
    });

    it('bot replies: plain text, or rich with at / reply / forward', () => {
        const plain = parseChatFrame(
            chat({
                type: 'bot_message',
                content: '好耶',
                message_type: 'text',
                segments: null,
                timestamp: 5,
                sender: { name: '麦麦' },
            }),
        );
        expect(plain?.op === 'chat' && plain.event).toMatchObject({
            kind: 'message',
            message: {
                id: '',
                fromBot: true,
                senderName: '麦麦',
                segments: [{ type: 'text', text: '好耶' }],
            },
        });
        const rich = parseChatFrame(
            chat({
                type: 'bot_message',
                content: '@人类 嗯',
                message_type: 'rich',
                timestamp: 6,
                segments: [
                    {
                        type: 'reply',
                        data: {
                            target_message_content: '在吗',
                            target_message_sender_nickname: '人类',
                        },
                    },
                    {
                        type: 'at',
                        data: {
                            target_user_id: 'u',
                            target_user_nickname: '人类',
                            target_user_cardname: '',
                        },
                    },
                    { type: 'text', data: '嗯' },
                    { type: 'forward', data: [{}, {}] },
                    { type: 'unknown', data: 'Dict(...)' },
                ],
            }),
        );
        const segs =
            rich?.op === 'chat' && rich.event.kind === 'message' ? rich.event.message.segments : [];
        expect(segs).toEqual([
            { type: 'reply', sender: '人类', text: '在吗' },
            { type: 'at', name: '人类' },
            { type: 'text', text: '嗯' },
            { type: 'forward', count: 2 },
            { type: 'other', text: 'Dict(...)' },
        ]);
    });

    it('typing, notices, nickname and session info', () => {
        const ev = (data: object) => {
            const f = parseChatFrame(chat(data));
            return f?.op === 'chat' ? f.event : null;
        };
        expect(ev({ type: 'typing', is_typing: true })).toEqual({ kind: 'typing', typing: true });
        expect(ev({ type: 'system', content: '已连接到本地聊天室', timestamp: 9 })).toEqual({
            kind: 'notice',
            text: '已连接到本地聊天室',
            at: 9,
            error: false,
        });
        expect(ev({ type: 'error', content: '处理消息时出错', timestamp: 9 })).toMatchObject({
            error: true,
        });
        expect(ev({ type: 'system', content: '' })).toBeNull();
        expect(ev({ type: 'nickname_updated', user_name: '小明' })).toEqual({
            kind: 'nickname',
            userName: '小明',
        });
        expect(
            ev({ type: 'session_info', bot_name: '', user_name: '人类', bot_qq: '123' }),
        ).toEqual({
            kind: 'session',
            botName: '麦麦',
            botQq: '123',
            userName: '人类',
        });
    });
});

describe('chatFrames', () => {
    it('send caps images at eight and uses upstream key names', () => {
        const images = Array.from({ length: 10 }, (_, i) => ({
            name: `${i}.png`,
            mimeType: 'image/png',
            base64: 'AA',
        }));
        const frame = JSON.parse(chatFrames.send('c3', 'desktop', '人类', '看图', images));
        expect(frame).toMatchObject({
            op: 'call',
            id: 'c3',
            domain: 'chat',
            method: 'message.send',
            session: 'desktop',
        });
        expect(frame.data.content).toBe('看图');
        expect(frame.data.user_name).toBe('人类');
        expect(frame.data.images).toHaveLength(8);
        expect(frame.data.images[0]).toEqual({
            name: '0.png',
            mime_type: 'image/png',
            base64: 'AA',
        });
    });

    it('open declares a launcher client and passes restore through', () => {
        const frame = JSON.parse(
            chatFrames.open('c1', 'desktop', 'ncd_desktop', '人类', true, '3.2.2'),
        );
        expect(frame.method).toBe('session.open');
        expect(frame.data).toEqual({
            user_id: 'ncd_desktop',
            user_name: '人类',
            client: { type: 'launcher', name: 'NapCatQQ Desktop', version: '3.2.2' },
            restore: true,
        });
    });
});

describe('helpers', () => {
    it('splitDataUrl', () => {
        expect(splitDataUrl('data:image/webp;base64,UklG')).toEqual({
            mimeType: 'image/webp',
            base64: 'UklG',
        });
        expect(splitDataUrl('https://x/y.png')).toBeNull();
    });

    it('timelineMarks: time rows after long gaps, sender once per run', () => {
        const marks = timelineMarks([
            { fromBot: false, at: 0 },
            { fromBot: false, at: 30 },
            { fromBot: true, at: 40 },
            { fromBot: true, at: 40 + 4 * 60 },
            { fromBot: true, at: 40 + 20 * 60 },
        ]);
        expect(marks).toEqual([
            { showTime: true, showSender: true },
            { showTime: false, showSender: false },
            { showTime: false, showSender: true },
            { showTime: false, showSender: true },
            { showTime: true, showSender: true },
        ]);
    });

    it('chatTimeLabel', () => {
        const now = new Date(2026, 8, 27, 15, 0);
        expect(chatTimeLabel(new Date(2026, 8, 27, 9, 5).getTime() / 1000, now)).toBe('09:05');
        expect(chatTimeLabel(new Date(2026, 8, 26, 22, 30).getTime() / 1000, now)).toBe(
            '昨天 22:30',
        );
        expect(chatTimeLabel(new Date(2026, 2, 3, 8, 0).getTime() / 1000, now)).toBe(
            '3月3日 08:00',
        );
        expect(chatTimeLabel(new Date(2025, 11, 31, 23, 59).getTime() / 1000, now)).toBe(
            '2025年12月31日 23:59',
        );
    });
});
