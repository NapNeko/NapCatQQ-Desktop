// 旧私聊历史把 message_id 当作序号；只合并能唯一对应的旧、新表示。
import type { Message } from './model';

const hasSequence = (message: Message) => !!message.sequence && message.sequence !== '0' && message.sequence !== message.id;
const legacySequence = (message: Message) => !!message.id && message.sequence === message.id;

function contentKey(message: Message): string {
    return JSON.stringify(message.segments.map(({ type, data }) => {
        if (type === 'image') {
            const file = String(data.file_id || data.file || '');
            const digest = file.replace(/[{}-]/g, '').match(/^([a-f0-9]{32})(?:\.|$)/i)?.[1];
            return [type, digest?.toLowerCase() || data.url || file];
        }
        return [type, Object.keys(data).sort().map(key => [key, data[key]])];
    }));
}

function sameEnvelope(a: Message, b: Message): boolean {
    return a.session === b.session && a.senderId === b.senderId && a.mine === b.mine && a.at === b.at;
}

export function findDuplicateMessage(messages: readonly Message[], incoming: Message): Message | undefined {
    const exact = messages.find(message => message.session === incoming.session && (message.key === incoming.key || !!incoming.id && message.id === incoming.id));
    if (exact) return exact;
    const peers = messages.filter(message => sameEnvelope(message, incoming));
    const sequence = hasSequence(incoming) && peers.find(message => hasSequence(message) && message.sequence === incoming.sequence);
    if (sequence) return sequence;
    if (!incoming.session.startsWith('private:')) return;
    const content = contentKey(incoming);
    const matches = peers.filter(message => contentKey(message) === content);
    if (matches.length !== 1) return;
    const match = matches[0];
    return legacySequence(match) && hasSequence(incoming) || hasSequence(match) && legacySequence(incoming) ? match : undefined;
}

export function mergeMessageIdentity(existing: Message, incoming: Message): Message {
    const authoritative = hasSequence(existing) && legacySequence(incoming) ? existing : incoming;
    return { ...existing, ...authoritative, key: existing.key, requestId: existing.requestId || incoming.requestId, recalled: existing.recalled || incoming.recalled };
}

export function deduplicateMessages(messages: readonly Message[]): Message[] {
    const identities = new Map<string, Message>();
    for (const message of messages) {
        const key = message.id ? `${message.session}/${message.id}` : message.key;
        const previous = identities.get(key);
        identities.set(key, previous ? mergeMessageIdentity(previous, message) : message);
    }
    const groups = new Map<string, Message[]>();
    for (const message of identities.values()) {
        const key = JSON.stringify([message.session, message.senderId, message.mine, message.at, contentKey(message)]);
        const group = groups.get(key);
        if (group) group.push(message); else groups.set(key, [message]);
    }
    const result: Message[] = [];
    for (const group of groups.values()) {
        // 多条同秒同文无法证明一一对应，宁可保留，也不吞掉真正的连续发送。
        if (group.length === 2 && findDuplicateMessage([group[0]], group[1])) result.push(mergeMessageIdentity(group[0], group[1]));
        else result.push(...group);
    }
    return result.sort((a, b) => a.at - b.at);
}
