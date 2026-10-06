// 加好友 / 加群请求卡片的「同意 / 拒绝」：拼出对应的一次 OneBot 调用。
// flag 是空的请求上游不收，也就不给入口；remark / reason 这类可选字段不带，上游按默认处理。

import type { ChatItem } from './chat';

export type RequestItem = Extract<ChatItem, { kind: 'request' }>;

const isRecord = (v: unknown): v is Record<string, unknown> =>
    typeof v === 'object' && v !== null && !Array.isArray(v);

/** 加群请求是「申请加入」还是「邀请 Bot」；事件里没写 sub_type 时按申请处理 */
function isInvite(item: RequestItem): boolean {
    return item.requestType === 'group' && isRecord(item.raw) && item.raw.sub_type === 'invite';
}

export interface RequestHandleCall {
    action: 'set_friend_add_request' | 'set_group_add_request';
    params: Record<string, unknown>;
}

/**
 * 处理这条请求要发的那次调用。没有 flag 返回 null（页面上也就别给按钮）。
 * `approve` true 是同意；set_group_add_request 的 sub_type 必填，从事件原文里拿。
 */
export function handleRequestCall(item: RequestItem, approve: boolean): RequestHandleCall | null {
    if (!item.flag) return null;
    if (item.requestType === 'friend') {
        return { action: 'set_friend_add_request', params: { flag: item.flag, approve } };
    }
    return {
        action: 'set_group_add_request',
        params: { flag: item.flag, sub_type: isInvite(item) ? 'invite' : 'add', approve },
    };
}

/** 卡片标题：「小明 请求加好友」/「小明 申请加群 100001」/「小明 邀请 Bot 加入群 甲群」 */
export function requestLine(item: RequestItem, user: string, group: string): string {
    if (item.requestType === 'friend') return `${user} 请求加好友`;
    return isInvite(item) ? `${user} 邀请 Bot 加入群 ${group}` : `${user} 申请加群 ${group}`;
}

/** 「拒绝」二次确认的后果句，和危险确认框里别的接口一个口吻 */
export function rejectLine(item: RequestItem, user: string, group: string): string {
    if (item.requestType === 'friend') return `会拒绝 ${user} 的加好友请求`;
    return isInvite(item)
        ? `会拒绝 ${user} 拉 Bot 进群 ${group} 的邀请`
        : `会拒绝 ${user} 加群 ${group} 的申请`;
}
