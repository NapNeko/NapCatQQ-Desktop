// 对着一条消息能一把开好的请求标签：回复 / 撤回 / 查发送者。
// 这里只算接口名和参数，发不发由中栏决定——这一步只是把标签摆好。
//
// 打开的标签参数和手动填的一样，危险接口（撤回）走到中栏发送时照样过二次确认。

import { parseSessionKey, type MessageItem } from './chatFormat';

export type EventLinkageId = 'reply' | 'recall' | 'sender';

export interface EventLinkage {
    id: EventLinkageId;
    label: string;
    action: string;
    params: Record<string, unknown>;
}

/** 消息行的会话 id：群号或私聊对象号；认不出（坏的会话键）是 undefined */
function sessionPeer(item: MessageItem): { type: 'group' | 'private'; id: number } | null {
    const s = parseSessionKey(item.session);
    return s && s.id > 0 ? s : null;
}

/**
 * 一条消息能用上的快捷操作。能不能用看手里有什么号：
 * 没有 message_id 的消息（发送失败的气泡）回复和撤回都不出现；「查发送者」只对着收到的消息，
 * 自己发的气泡 sender 是自己，查了没意义。
 */
export function messageLinkages(item: MessageItem): EventLinkage[] {
    const out: EventLinkage[] = [];
    const peer = sessionPeer(item);

    if (item.messageId !== undefined && peer) {
        // 私聊会话的 id 就是对方号（reduceMessage 建好气泡时定下的），回复和 @ 人一样发给会话
        const replySegment = { type: 'reply', data: { id: item.messageId } };
        out.push(
            peer.type === 'group'
                ? { id: 'reply', label: '回复', action: 'send_group_msg', params: { group_id: peer.id, message: [replySegment] } }
                : { id: 'reply', label: '回复', action: 'send_private_msg', params: { user_id: peer.id, message: [replySegment] } },
            { id: 'recall', label: '撤回', action: 'delete_msg', params: { message_id: item.messageId } },
        );
    }

    if (item.direction === 'in' && item.senderId > 0) {
        out.push(
            peer?.type === 'group'
                ? {
                      id: 'sender',
                      label: '查发送者',
                      action: 'get_group_member_info',
                      params: { group_id: peer.id, user_id: item.senderId },
                  }
                : { id: 'sender', label: '查发送者', action: 'get_stranger_info', params: { user_id: item.senderId } },
        );
    }

    return out;
}
