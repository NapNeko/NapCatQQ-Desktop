// 阅读页和近期页共享消息对象；预算按 UTF-16 内容估算，不把它当作浏览器堆采样。
import type { Message, SessionKey } from './model';

export const HISTORY_PAGE_SIZE = 50;
export const WORKING_MESSAGE_LIMIT = 1000;
export const WORKING_BYTE_LIMIT = 8 * 1024 * 1024;
export const ARCHIVE_BYTE_LIMIT = 16 * 1024 * 1024;
export interface ReadingAnchor {
    session: SessionKey;
    messageKey: string;
    messageId?: string | null;
    atBottom?: boolean;
}
const sizes = new WeakMap<Message, number>();
const archiveCopies = new WeakMap<Message, Message>();
const inlineValue = (value: unknown) =>
    typeof value === 'string' && /^(base64:\/\/|data:)/i.test(value);

function archiveCopy(message: Message): Message {
    const existing = archiveCopies.get(message);
    if (existing) return existing;
    // 小媒体仍受归档总预算约束；只为工作集装不下的临时内容保留轻量副本。
    if (messageBytes(message) <= WORKING_BYTE_LIMIT) {
        archiveCopies.set(message, message);
        return message;
    }
    const segments = message.segments.map((segment) => {
        if (segment.type === 'text') return segment;
        const entries = Object.entries(segment.data).filter(
            ([key, value]) => key.toLowerCase() !== 'base64' && !inlineValue(value),
        );
        return entries.length === Object.keys(segment.data).length
            ? segment
            : { ...segment, data: Object.fromEntries(entries) };
    });
    const copy = segments.every((segment, index) => segment === message.segments[index])
        ? message
        : { ...message, segments };
    archiveCopies.set(message, copy);
    return copy;
}
export function messageBytes(message: Message): number {
    let bytes = sizes.get(message);
    if (bytes === undefined) {
        bytes = JSON.stringify(message).length * 2 + 128;
        sizes.set(message, bytes);
    }
    return bytes;
}

export function retainArchiveMessages(messages: readonly Message[], limit = 5000): Message[] {
    let bytes = 0;
    const retained: Message[] = [];
    for (let index = messages.length - 1; index >= 0 && retained.length < limit; index--) {
        // 大型临时媒体先剥离，再计算归档预算；正文和协议标识仍保留。
        const message = archiveCopy(messages[index]);
        const size = messageBytes(message);
        // 归档没有持久化分页缺口；保留连续后缀，避免再插入缺口前的旧消息。
        if (bytes + size > ARCHIVE_BYTE_LIMIT) break;
        bytes += size;
        retained.push(message);
    }
    return retained.reverse();
}

export function retainWorkingMessages(
    messages: Message[],
    active: SessionKey | null = null,
    loading: SessionKey | null = null,
    anchor?: ReadingAnchor,
    replies: ReadonlySet<string> = new Set(),
): Message[] {
    const sessions = new Map<SessionKey, Message[]>();
    for (const message of messages) {
        const list = sessions.get(message.session);
        if (list) list.push(message);
        else sessions.set(message.session, [message]);
    }
    const priorities = new Map<Message, number>();
    for (const [session, rows] of sessions) {
        for (const message of rows.slice(-HISTORY_PAGE_SIZE))
            priorities.set(message, session === active ? 3 : 1);
        if (session === active || session === loading) {
            const saved = anchor?.session === session && !anchor.atBottom ? anchor : undefined;
            let index = saved
                ? rows.findIndex(
                      (message) =>
                          message.key === saved.messageKey ||
                          (!!saved.messageId && message.id === saved.messageId),
                  )
                : session === loading
                  ? 0
                  : rows.length - 1;
            if (index < 0) index = rows.length - 1;
            const page = Math.floor(index / HISTORY_PAGE_SIZE);
            const start = Math.max(0, (page - 1) * HISTORY_PAGE_SIZE);
            const end = Math.min(rows.length, (page + 2) * HISTORY_PAGE_SIZE);
            for (const message of rows.slice(start, end)) priorities.set(message, 4);
            if (rows[index]) priorities.set(rows[index], 6);
        }
    }
    for (const message of messages)
        if (
            message.status !== 'sent' ||
            (message.id && replies.has(`${message.session}/${message.id}`))
        )
            priorities.set(message, 5);
    // 保留当前会话最新一条超预算的本机图片，避免发送成功后立即消失。
    // 只豁免一条；旧消息的临时内容不留在归档副本里。
    const protectedInline = [...messages]
        .reverse()
        .find(
            (message) =>
                message.session === active &&
                message.mine &&
                !!message.requestId &&
                message.status !== 'sending' &&
                message.segments.some(
                    (segment) =>
                        segment.type === 'image' && Object.values(segment.data).some(inlineValue),
                ) &&
                messageBytes(message) > WORKING_BYTE_LIMIT,
        );
    if (protectedInline) priorities.set(protectedInline, 7);
    const candidates = [...priorities.keys()].sort(
        (a, b) => priorities.get(b)! - priorities.get(a)! || b.at - a.at,
    );
    const retained = new Set<Message>();
    let bytes = 0;
    for (const message of candidates) {
        const size = messageBytes(message);
        // 已提交的发送不能因一次大附件被裁掉，否则回包找不到 requestId。
        if (
            message.status !== 'sending' &&
            message !== protectedInline &&
            (retained.size >= WORKING_MESSAGE_LIMIT || bytes + size > WORKING_BYTE_LIMIT)
        )
            continue;
        retained.add(message);
        if (message !== protectedInline) bytes += size;
    }
    const result: Message[] = [];
    for (const rows of sessions.values()) {
        let missing = false;
        let previous: Message | undefined;
        for (const message of rows) {
            if (!retained.has(message)) {
                if (previous) missing = true;
                continue;
            }
            const gapBefore = !!previous && (missing || !!message.gapBefore);
            result.push(gapBefore === !!message.gapBefore ? message : { ...message, gapBefore });
            previous = message;
            missing = false;
        }
    }
    result.sort((a, b) => a.at - b.at);
    return result.length === messages.length &&
        result.every((message, index) => message === messages[index])
        ? messages
        : result;
}
