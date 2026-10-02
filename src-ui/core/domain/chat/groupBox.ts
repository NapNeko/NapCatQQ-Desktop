// 群盒子只改变会话归类，不影响接收、未读或消息搜索。
import type { Conversation } from './model';

export function groupBoxSummary(rows: Conversation[]) {
    const groups = rows.filter(c => c.type === 'group' && c.boxed);
    return { count: groups.length, unread: groups.reduce((sum, c) => sum + c.unread, 0), latest: groups.reduce<Conversation | undefined>((latest, c) => !latest || c.lastAt > latest.lastAt ? c : latest, undefined) };
}
export function conversationRows(rows: Conversation[], options: { box: boolean; unread: boolean; query: string }) {
    const term = options.query.trim().toLocaleLowerCase();
    return rows.filter(c => (term && !options.box || !!c.boxed === options.box) && (!term || `${c.name} ${c.id}`.toLocaleLowerCase().includes(term)) && (!options.unread || c.unread > 0))
        .sort((a, b) => Number(b.pinned) - Number(a.pinned) || b.lastAt - a.lastAt);
}
