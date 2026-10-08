// 聊天会话的内存投影；标识在协议边界统一为字符串。
import { normalizeMessage, messagePreview, type Segment } from '../debug/segments';
import { isLocalFileToken } from '../debug/streamActions';
import type { Mention } from '../debug/composerModel';
import { findDuplicateMessage, mergeMessageIdentity, sortMessages } from './messageIdentity';
import { isInlineImageReference } from './imageSource';
import {
    retainWorkingMessages,
    retainArchiveMessages,
    type ReadingAnchor,
} from './messageWorkingSet';

export type SessionKey = `group:${string}` | `private:${string}`;
export interface Contact {
    key: SessionKey;
    type: 'group' | 'private';
    id: string;
    name: string;
    members?: number;
    categoryId?: string;
    categoryName?: string;
}
export interface Conversation extends Contact {
    unread: number;
    pinned: boolean;
    lastAt: number;
    preview: string;
    boxed?: boolean;
}
export type SendStatus = 'sending' | 'sent' | 'failed' | 'unknown';
export interface Message {
    key: string;
    session: SessionKey;
    id?: string;
    sequence?: string;
    fileId?: string;
    requestId?: string;
    senderId: string;
    senderName: string;
    at: number;
    mine: boolean;
    segments: Segment[];
    status: SendStatus;
    error?: string;
    recalled?: boolean;
    notice?: string;
    gapBefore?: boolean;
}
export type Attachment = { key: string; name: string } & (
    | { type: 'image' | 'file'; path: string; subType?: 1; previewPath?: string }
    | { type: 'face'; id: string }
);
export interface Reply {
    id: string;
    name: string;
    preview: string;
}
export interface Draft {
    text: string;
    attachments: Attachment[];
    reply: Reply | null;
    mentions?: Mention[];
}
export interface Account {
    selfId: string;
    active: SessionKey | null;
    conversations: Record<string, Conversation>;
    messages: Message[];
    archiveMessages?: Message[];
    drafts: Record<string, Draft>;
    lastSeq: number;
    gap: boolean;
}
export const EMPTY_DRAFT: Draft = { text: '', attachments: [], reply: null };
export const MESSAGE_LIMIT = 5000;
export function retainMessages(
    messages: Message[],
    reading?: SessionKey | null,
    loading?: SessionKey | null,
    anchor?: ReadingAnchor,
    replies?: ReadonlySet<string>,
): Message[] {
    return retainWorkingMessages(messages, reading, loading, anchor, replies);
}
export function mergeMessageRows(
    saved: readonly Message[],
    incoming: readonly Message[],
): Message[] {
    const byKey = new Map(saved.map((message) => [message.key, message] as const));
    const byId = new Map(
        saved
            .filter((message) => message.id)
            .map((message) => [`${message.session}/${message.id}`, message.key] as const),
    );
    for (const message of incoming) {
        const previous = message.id && byId.get(`${message.session}/${message.id}`);
        const savedMessage = byKey.get(previous || message.key);
        if (previous && previous !== message.key) byKey.delete(previous);
        byKey.set(
            message.key,
            savedMessage?.recalled
                ? {
                      ...message,
                      recalled: true,
                      segments: savedMessage.segments.length
                          ? savedMessage.segments
                          : message.segments,
                  }
                : message,
        );
        if (message.id) byId.set(`${message.session}/${message.id}`, message.key);
    }
    return sortMessages([...byKey.values()]);
}
export function mergeArchiveMessages(
    saved: readonly Message[],
    incoming: readonly Message[],
): Message[] {
    return retainArchiveMessages(mergeMessageRows(saved, incoming), MESSAGE_LIMIT);
}
export function trimAccountMessages(
    state: Account,
    anchor?: ReadingAnchor,
    loading: SessionKey | null = null,
): Account {
    const replies = new Set(
        Object.entries(state.drafts).flatMap(([session, draft]) =>
            draft.reply ? [`${session}/${draft.reply.id}`] : [],
        ),
    );
    const messages = retainMessages(state.messages, state.active, loading, anchor, replies);
    return messages === state.messages ? state : { ...state, messages };
}
export const accountKey = (botId: string, selfId: string) => JSON.stringify([botId, selfId]);
export const record = (v: unknown): Record<string, unknown> =>
    typeof v === 'object' && v !== null && !Array.isArray(v) ? (v as Record<string, unknown>) : {};
export const id = (v: unknown): string =>
    typeof v === 'string' ? v : typeof v === 'number' && Number.isSafeInteger(v) ? String(v) : '';
export const text = (v: unknown): string => (typeof v === 'string' ? v : '');
export function emptyAccount(selfId: string): Account {
    return {
        selfId,
        active: null,
        conversations: {},
        messages: [],
        drafts: {},
        lastSeq: 0,
        gap: false,
    };
}
export function contactFromKey(key: SessionKey): Contact {
    const [type, ...parts] = key.split(':');
    const value = parts.join(':');
    return { key, type: type === 'group' ? 'group' : 'private', id: value, name: value };
}
export function parseContact(raw: unknown, type: Contact['type']): Contact | null {
    const row = record(raw);
    const value = id(type === 'group' ? row.group_id : row.user_id);
    if (!value) return null;
    return {
        key: `${type}:${value}`,
        type,
        id: value,
        name: text(row.group_name) || text(row.remark) || text(row.nickname) || value,
        members: typeof row.member_count === 'number' ? row.member_count : undefined,
        categoryId:
            type === 'private' ? id(row.category_id ?? row.categoryId) || undefined : undefined,
        categoryName:
            type === 'private'
                ? text(row.categoryName) || text(row.category_name) || undefined
                : undefined,
    };
}
export function parseFriendCategories(raw: unknown): Contact[] | null {
    if (!Array.isArray(raw)) return null;
    const contacts = new Map<SessionKey, Contact>();
    for (const value of raw) {
        const category = record(value);
        const categoryId = id(category.categoryId);
        if (!categoryId || !Array.isArray(category.buddyList)) return null;
        const categoryName =
            text(category.categoryName).trim() ||
            (categoryId === '0' ? '未分组' : `分组 ${categoryId}`);
        for (const buddy of category.buddyList) {
            const contact = parseContact(buddy, 'private');
            if (contact) contacts.set(contact.key, { ...contact, categoryId, categoryName });
        }
    }
    return [...contacts.values()];
}
function conversation(state: Account, contact: Contact): Conversation {
    const existing: Conversation | undefined = state.conversations[contact.key];
    return existing
        ? { ...existing, ...contact }
        : { unread: 0, pinned: false, lastAt: 0, preview: '', ...contact };
}
export function openConversation(state: Account, contact: Contact): Account {
    return trimAccountMessages({
        ...state,
        active: contact.key,
        conversations: {
            ...state.conversations,
            [contact.key]: { ...conversation(state, contact), unread: 0 },
        },
    });
}
export function setDraft(state: Account, key: SessionKey, draft: Draft): Account {
    return { ...state, drafts: { ...state.drafts, [key]: draft } };
}
export function ingestMessage(
    state: Account,
    raw: unknown,
    historical = false,
    reading: SessionKey | null = null,
): Account {
    return ingestMessages(state, [raw], historical, reading);
}
export function ingestMessages(
    state: Account,
    rows: readonly unknown[],
    historical = false,
    reading: SessionKey | null = null,
    anchor?: ReadingAnchor,
    loading: SessionKey | null = null,
): Account {
    if (!rows.length) return state;
    const archived = new Map(
        (state.archiveMessages ?? []).map(
            (message) =>
                [message.id ? `${message.session}/${message.id}` : message.key, message] as const,
        ),
    );
    let next: Account = {
        ...state,
        messages: [...state.messages],
        conversations: { ...state.conversations },
    };
    for (const raw of rows) next = ingestOne(next, raw, historical, reading, archived);
    sortMessages(next.messages);
    next.archiveMessages = mergeArchiveMessages(
        next.archiveMessages ?? state.messages,
        next.messages,
    );
    return trimAccountMessages(
        next,
        anchor,
        loading ??
            (historical
                ? (next.messages.find((message) => !state.messages.includes(message))?.session ??
                  null)
                : null),
    );
}
function ingestOne(
    state: Account,
    raw: unknown,
    historical: boolean,
    reading: SessionKey | null,
    archived: ReadonlyMap<string, Message>,
): Account {
    const row = record(raw);
    if (row.notice_type === 'group_recall' || row.notice_type === 'friend_recall') {
        const messageId = id(row.message_id);
        const session =
            row.notice_type === 'group_recall'
                ? `group:${id(row.group_id)}`
                : `private:${id(row.user_id)}`;
        const latest = (state.archiveMessages ?? state.messages)
            .filter((message) => message.session === session)
            .at(-1);
        const current = state.conversations[session];
        if (current && latest?.id === messageId && latest.at >= current.lastAt)
            state.conversations[session] = { ...current, preview: '消息已撤回' };
        for (let index = 0; index < state.messages.length; index++) {
            const message = state.messages[index];
            if (message.session === session && message.id === messageId)
                state.messages[index] = { ...message, recalled: true };
        }
        state.archiveMessages = state.archiveMessages?.map((message) =>
            message.session === session && message.id === messageId
                ? { ...message, recalled: true }
                : message,
        );
        return state;
    }
    if (row.notice_type === 'notify' && text(row.sub_type) === 'poke')
        return ingestPoke(state, row, historical, reading, archived);
    if (row.notice_type === 'group_increase')
        return ingestGroupJoin(state, row, historical, reading, archived);
    if (row.message_type !== 'group' && row.message_type !== 'private') return state;
    const sender = record(row.sender);
    const senderId = id(sender.user_id) || id(row.user_id);
    const mine = row.post_type === 'message_sent' || (!!state.selfId && senderId === state.selfId);
    const peer =
        row.message_type === 'group'
            ? id(row.group_id)
            : mine
              ? id(row.target_id) || id(row.peer_id) || id(row.user_id)
              : id(row.user_id) || senderId;
    if (!peer) return state;
    const session: SessionKey = `${row.message_type}:${peer}`;
    const messageId = id(row.message_id);
    const at = typeof row.time === 'number' ? row.time * 1000 : Date.now();
    const segments = normalizeMessage(row.message ?? row.raw_message);
    const fileId =
        segments.length === 1 && segments[0].type === 'file' ? id(segments[0].data.file_id) : '';
    const key = messageId
        ? `${session}/${messageId}`
        : `${session}/event/${id(row.message_seq) || at}/${senderId}/${messagePreview(segments)}`;
    const name = text(sender.card) || text(sender.nickname) || senderId;
    const message: Message = {
        key,
        session,
        id: messageId || undefined,
        sequence: id(row.message_seq) || undefined,
        fileId: fileId || undefined,
        senderId,
        senderName: name,
        mine,
        segments,
        at,
        status: 'sent',
    };
    const existing =
        findDuplicateMessage(state.messages, message) ??
        state.messages.find(
            (m) =>
                m.session === session &&
                mine &&
                fileId &&
                m.requestId &&
                !m.id &&
                m.fileId === fileId,
        );
    const saved = archived.get(messageId ? `${session}/${messageId}` : key);
    if (saved && saved.recalled) {
        message.recalled = true;
        if (saved.segments.length) message.segments = saved.segments;
    }
    if (!existing && saved && !historical) {
        if (saved.requestId)
            state.messages.push({
                ...mergeMessageIdentity(saved, message),
                segments: withLocalImageSources(message.segments, saved.segments),
            });
        return state;
    }
    if (existing) {
        if (
            !existing.requestId &&
            existing.id === message.id &&
            (!message.sequence || existing.sequence === message.sequence)
        )
            return state;
        const merged = mergeMessageIdentity(existing, {
            ...message,
            id: message.id || existing.id,
            fileId: message.fileId || existing.fileId,
            sequence: message.sequence || existing.sequence,
            error: undefined,
        });
        state.messages[state.messages.indexOf(existing)] = {
            ...merged,
            segments: withLocalImageSources(merged.segments, existing.segments),
        };
        return state;
    }
    const contact = state.conversations[session] ?? {
        ...contactFromKey(session),
        name: text(row.group_name) || (row.message_type === 'private' && !mine ? name : peer),
    };
    const current = conversation(state, contact);
    state.messages.push(message);
    state.conversations[session] = {
        ...current,
        unread: current.unread + (!historical && !mine && reading !== session ? 1 : 0),
        lastAt: Math.max(at, current.lastAt),
        preview:
            at >= current.lastAt
                ? message.recalled
                    ? '消息已撤回'
                    : messagePreview(message.segments)
                : current.preview,
    };
    return state;
}
// 拍一拍通知落成时间线系统行。通知里只有 QQ 号，名字从该会话已见过的发送者里反查；
// 自己发出的在 poke() 里已乐观上墙，后端回显按时间窗去重。
function ingestPoke(
    state: Account,
    row: Record<string, unknown>,
    historical: boolean,
    reading: SessionKey | null,
    archived: ReadonlyMap<string, Message>,
): Account {
    // 发起者优先取 sender_id：部分后端的 poke 通知里 user_id 是被拍的人
    const from = id(row.sender_id) || id(row.user_id);
    const to = id(row.target_id);
    if (!from || !to) return state;
    const groupId = id(row.group_id);
    const session: SessionKey = groupId
        ? `group:${groupId}`
        : `private:${from === state.selfId ? to : from}`;
    const at = typeof row.time === 'number' && row.time > 0 ? row.time * 1000 : Date.now();
    const echoKey = `${session}/poke/${from}/${to}/`;
    for (const message of archived.values())
        if (message.notice && message.key.startsWith(echoKey) && Math.abs(at - message.at) < 8000)
            return state;
    if (
        [...state.messages, ...(state.archiveMessages ?? [])].some(
            (m) =>
                m.session === session &&
                m.notice &&
                m.key.startsWith(echoKey) &&
                Math.abs(at - m.at) < 8000,
        )
    )
        return state;
    const nameOf = (qq: string): string => {
        if (qq === state.selfId) return '你';
        for (let i = state.messages.length - 1; i >= 0; i--) {
            const m = state.messages[i];
            if (m.session === session && m.senderId === qq && m.senderName) return m.senderName;
        }
        return qq;
    };
    const mine = from === state.selfId;
    const line = `${nameOf(from)}拍了拍${nameOf(to)}`;
    const message: Message = {
        key: `${echoKey}${at}`,
        session,
        senderId: from,
        senderName: mine ? '我' : nameOf(from),
        at,
        mine,
        segments: [],
        status: 'sent',
        notice: line,
    };
    const current = conversation(state, state.conversations[session] ?? contactFromKey(session));
    state.messages.push(message);
    state.conversations[session] = {
        ...current,
        unread: current.unread + (!historical && !mine && reading !== session ? 1 : 0),
        lastAt: Math.max(at, current.lastAt),
        preview: at >= current.lastAt ? line : current.preview,
    };
    return state;
}
// 入群通知落成时间线系统行，名字同样从会话已见发送者反查；邀请入群带上操作者。
function ingestGroupJoin(
    state: Account,
    row: Record<string, unknown>,
    historical: boolean,
    reading: SessionKey | null,
    archived: ReadonlyMap<string, Message>,
): Account {
    const groupId = id(row.group_id);
    const userId = id(row.user_id);
    if (!groupId || !userId) return state;
    const session: SessionKey = `group:${groupId}`;
    const at = typeof row.time === 'number' && row.time > 0 ? row.time * 1000 : Date.now();
    const key = `${session}/join/${userId}/${at}`;
    // 历史回放和实时推送可能撞上同一条通知
    if (archived.has(key) || state.messages.some((m) => m.key === key)) return state;
    const nameOf = (qq: string): string => {
        if (qq === state.selfId) return '你';
        for (let i = state.messages.length - 1; i >= 0; i--) {
            const m = state.messages[i];
            if (m.session === session && m.senderId === qq && m.senderName) return m.senderName;
        }
        return qq;
    };
    const operator = id(row.operator_id);
    const invited = text(row.sub_type) === 'invite' && !!operator && operator !== userId;
    const line = invited
        ? `${nameOf(operator)}邀请${nameOf(userId)}加入了本群`
        : `${nameOf(userId)}加入了本群`;
    const mine = userId === state.selfId;
    const message: Message = {
        key,
        session,
        senderId: userId,
        senderName: mine ? '我' : nameOf(userId),
        at,
        mine,
        segments: [],
        status: 'sent',
        notice: line,
    };
    const current = conversation(state, state.conversations[session] ?? contactFromKey(session));
    state.messages.push(message);
    state.conversations[session] = {
        ...current,
        unread: current.unread + (!historical && !mine && reading !== session ? 1 : 0),
        lastAt: Math.max(at, current.lastAt),
        preview: at >= current.lastAt ? line : current.preview,
    };
    return state;
}
export function addPending(
    state: Account,
    session: SessionKey,
    requestId: string,
    segments: Segment[],
    at: number,
): Account {
    const current = conversation(state, state.conversations[session] ?? contactFromKey(session));
    const message: Message = {
        key: `pending/${requestId}`,
        requestId,
        session,
        senderId: state.selfId,
        senderName: '我',
        mine: true,
        segments,
        at,
        status: 'sending',
    };
    return {
        ...state,
        messages: [...state.messages, message],
        conversations: {
            ...state.conversations,
            [session]: { ...current, lastAt: at, preview: messagePreview(segments) },
        },
    };
}
export function settleSend(
    state: Account,
    requestId: string,
    result: { state: SendStatus; id?: string; fileId?: string; error?: string },
): Account {
    const pending = state.messages.find((m) => m.requestId === requestId);
    if (!pending) return state;
    const echo = state.messages.find(
        (m) =>
            m !== pending &&
            m.mine &&
            m.session === pending.session &&
            ((result.id && m.id === result.id) ||
                (result.fileId && !m.requestId && m.fileId === result.fileId)),
    );
    const messages = state.messages
        .filter((m) => m !== echo)
        .map((m) =>
            m === pending
                ? {
                      ...m,
                      ...(echo ?? {}),
                      key: pending.key,
                      at: pending.at,
                      requestId,
                      id: result.id || echo?.id || m.id,
                      fileId: result.fileId || echo?.fileId || m.fileId,
                      segments: withLocalImageSources(
                          echo ? echo.segments : m.segments,
                          m.segments,
                      ),
                      status: result.state,
                      error: result.error,
                  }
                : m,
        );
    sortMessages(messages);
    return {
        ...state,
        messages,
        archiveMessages: mergeArchiveMessages(
            (state.archiveMessages ?? state.messages).filter(
                (message) => !echo || message.key !== echo.key,
            ),
            messages,
        ),
    };
}
// 自己刚发的图以本机字节为准：回显/回包替换段时把本机来源挂到 local_file 上，展示与重发不再依赖协议回环。
function withLocalImageSources(segments: Segment[], previous: Segment[]): Segment[] {
    const locals = previous
        .filter((s) => s.type === 'image')
        .map((s) => text(s.data.inline_ref) || text(s.data.local_file) || text(s.data.file))
        .filter(
            (f) => f.startsWith('base64://') || isInlineImageReference(f) || isLocalFileToken(f),
        );
    if (!locals.length) return segments;
    let index = 0;
    return segments.map((segment) => {
        if (segment.type !== 'image') return segment;
        const local = locals[index++];
        if (!local || text(segment.data.local_file)) return segment;
        if (text(segment.data.file) === local) return segment;
        return { ...segment, data: { ...segment.data, local_file: local } };
    });
}
