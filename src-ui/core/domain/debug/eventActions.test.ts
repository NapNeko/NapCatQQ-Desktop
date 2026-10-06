import { describe, expect, it } from 'vitest';
import type { MessageItem } from './chatFormat';
import { messageLinkages } from './eventActions';

function item(patch: Partial<MessageItem> = {}): MessageItem {
    return {
        kind: 'message',
        key: 'e1',
        seq: 1,
        at: 1_700_000_000_000,
        session: 'group:100001',
        direction: 'in',
        senderId: 10001,
        senderName: '小明',
        messageId: 4242,
        segments: [{ type: 'text', data: { text: '在吗' } }],
        raw: {},
        ...patch,
    };
}

const byId = (list: ReturnType<typeof messageLinkages>, id: string) =>
    list.find((l) => l.id === id);

describe('messageLinkages', () => {
    it('群消息：回复发 send_group_msg（带 reply 段和群号）、撤复发 delete_msg、查发送者发 get_group_member_info', () => {
        const links = messageLinkages(item());
        expect(links.map((l) => l.id)).toEqual(['reply', 'recall', 'sender']);

        expect(byId(links, 'reply')).toMatchObject({
            action: 'send_group_msg',
            params: { group_id: 100001, message: [{ type: 'reply', data: { id: 4242 } }] },
        });
        expect(byId(links, 'recall')).toMatchObject({
            action: 'delete_msg',
            params: { message_id: 4242 },
        });
        expect(byId(links, 'sender')).toMatchObject({
            action: 'get_group_member_info',
            params: { group_id: 100001, user_id: 10001 },
        });
    });

    it('私聊消息：回复发给会话对象（send_private_msg），查发送者用 get_stranger_info,不带群号', () => {
        const links = messageLinkages(item({ session: 'private:10001' }));
        expect(byId(links, 'reply')).toMatchObject({
            action: 'send_private_msg',
            params: { user_id: 10001, message: [{ type: 'reply', data: { id: 4242 } }] },
        });
        expect(byId(links, 'sender')).toMatchObject({
            action: 'get_stranger_info',
            params: { user_id: 10001 },
        });
        expect(byId(links, 'sender')?.params.group_id).toBeUndefined();
    });

    it('自己发的私聊气泡：回复发给会话对象（不是自己），没有「查发送者」', () => {
        const links = messageLinkages(
            item({
                session: 'private:20002',
                direction: 'out',
                senderId: 1919810,
                senderName: '我',
            }),
        );
        expect(byId(links, 'reply')?.params.user_id).toBe(20002);
        expect(byId(links, 'sender')).toBeUndefined();
        // 自己的消息照样能撤
        expect(byId(links, 'recall')).toBeDefined();
    });

    it('没有 message_id（发送失败的气泡）：回复和撤回都不出现', () => {
        const noId = item();
        delete noId.messageId;
        const links = messageLinkages(noId);
        expect(links.map((l) => l.id)).toEqual(['sender']);
    });

    it('发送者号拿不到（senderId=0）时没有「查发送者」', () => {
        const links = messageLinkages(item({ senderId: 0 }));
        expect(byId(links, 'sender')).toBeUndefined();
    });

    it('会话键坏了：回复和撤回都不开（不知道往哪发）', () => {
        const links = messageLinkages(item({ session: 'weird' }));
        expect(links.map((l) => l.id)).toEqual(['sender']);
    });
});
