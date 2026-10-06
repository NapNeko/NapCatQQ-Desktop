import { describe, expect, it } from 'vitest';
import { handleRequestCall, rejectLine, requestLine, type RequestItem } from './requestHandling';

function item(patch: Partial<RequestItem> = {}): RequestItem {
    return {
        kind: 'request',
        key: 'e1',
        seq: 1,
        at: 1,
        requestType: 'friend',
        userId: 10001,
        comment: '我是小明',
        flag: 'FLAG-1',
        raw: { post_type: 'request', request_type: 'friend', user_id: 10001, flag: 'FLAG-1' },
        ...patch,
    };
}

const groupItem = (raw: Record<string, unknown> = {}): RequestItem =>
    item({
        requestType: 'group',
        groupId: 100001,
        raw: {
            post_type: 'request',
            request_type: 'group',
            sub_type: 'add',
            user_id: 10001,
            group_id: 100001,
            flag: 'FLAG-1',
            ...raw,
        },
    });

describe('handleRequestCall', () => {
    it('好友请求：set_friend_add_request 带 flag 和 approve', () => {
        expect(handleRequestCall(item(), true)).toEqual({
            action: 'set_friend_add_request',
            params: { flag: 'FLAG-1', approve: true },
        });
        expect(handleRequestCall(item(), false)).toEqual({
            action: 'set_friend_add_request',
            params: { flag: 'FLAG-1', approve: false },
        });
    });

    it('加群申请：sub_type 是 add；邀请 Bot 的是 invite（从事件原文拿）', () => {
        expect(handleRequestCall(groupItem(), true)?.params).toMatchObject({
            sub_type: 'add',
            approve: true,
        });
        expect(handleRequestCall(groupItem({ sub_type: 'invite' }), false)?.params).toMatchObject({
            sub_type: 'invite',
            approve: false,
        });
    });

    it('没有 flag 的请求返回 null', () => {
        expect(handleRequestCall(item({ flag: '' }), true)).toBeNull();
    });
});

describe('requestLine / rejectLine', () => {
    it('卡片标题按请求类型和是否邀请拼', () => {
        expect(requestLine(item(), '小明', '')).toBe('小明 请求加好友');
        expect(requestLine(groupItem(), '小明', '甲群')).toBe('小明 申请加群 甲群');
        expect(requestLine(groupItem({ sub_type: 'invite' }), '小明', '甲群')).toBe(
            '小明 邀请 Bot 加入群 甲群',
        );
    });

    it('拒绝的后果句', () => {
        expect(rejectLine(item(), '小明', '')).toBe('会拒绝 小明 的加好友请求');
        expect(rejectLine(groupItem(), '小明', '甲群')).toBe('会拒绝 小明 加群 甲群 的申请');
        expect(rejectLine(groupItem({ sub_type: 'invite' }), '小明', '甲群')).toBe(
            '会拒绝 小明 拉 Bot 进群 甲群 的邀请',
        );
    });
});
