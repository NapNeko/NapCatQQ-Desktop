// 聊天视图的筛选：按会话、按条目种类、按文字。纯函数，每次筛选条件变了才对整个时间线跑一遍。

import type { ChatItem, SessionKey } from './chat';
import { messagePreview } from './segments';

export interface ChatFilter {
    session: SessionKey | 'all';
    kinds: { message: boolean; notice: boolean; request: boolean; call: boolean; meta: boolean };
    showHeartbeat: boolean;
    text: string;
}

export const DEFAULT_CHAT_FILTER: ChatFilter = {
    session: 'all',
    kinds: { message: true, notice: true, request: true, call: true, meta: true },
    // 心跳一直在来，默认藏起来，要看的人自己打开
    showHeartbeat: false,
    text: '',
};

/** 条目属于哪个会话；调用和元事件不属于任何会话，返回 undefined */
function sessionOf(item: ChatItem): SessionKey | undefined {
    switch (item.kind) {
        case 'message':
        case 'notice':
            return item.session;
        case 'request':
            return item.groupId !== undefined ? `group:${item.groupId}` : `private:${item.userId}`;
        default:
            return undefined;
    }
}

/** 拿去做文字搜索的内容（已转小写前的原文），一条条目里可能被搜到的都拼进去 */
function searchable(item: ChatItem): string {
    switch (item.kind) {
        case 'message':
            return `${messagePreview(item.segments)}\n${item.senderName}\n${item.senderId}\n${item.messageId ?? ''}`;
        case 'notice':
            return item.text;
        case 'request':
            return `${item.comment}\n${item.userId}\n${item.groupId ?? ''}`;
        case 'call':
            return `${item.action}\n${item.summary}`;
        case 'meta':
            return item.text;
        default:
            return '';
    }
}

/**
 * 筛出要显示的条目。缺口 / 丢弃 / 接收器状态是「这段时间数据不完整」的提示，
 * 不管怎么筛都保留，免得用户在筛过的画面里误以为没有遗漏。
 */
export function filterItems(items: ChatItem[], f: ChatFilter): ChatItem[] {
    const query = f.text.trim().toLowerCase();
    const inSession = f.session !== 'all';
    const out: ChatItem[] = [];
    for (const item of items) {
        if (item.kind === 'gap' || item.kind === 'dropped' || item.kind === 'receiver') {
            out.push(item);
            continue;
        }
        if (!f.kinds[item.kind]) continue;
        if (item.kind === 'meta' && item.heartbeat && !f.showHeartbeat) continue;
        // 选中某个会话就只看这个会话的东西；没有归属的条目（调用、元事件）不属于任何会话
        if (inSession && sessionOf(item) !== f.session) continue;
        if (query && !searchable(item).toLowerCase().includes(query)) continue;
        out.push(item);
    }
    // 没有任何条目被筛掉时沿用原数组，下游的 memo 不会白白失效
    return out.length === items.length ? items : out;
}
