// 档案只包含会话与消息，运行期游标和草稿不跨启动恢复。
import type { ChatArchive } from '../../ipc/generated/chat/ChatArchive';
import { messagePreview, normalizeMessage } from '../debug/segments';
import {
    id,
    record,
    text,
    ingestMessage,
    MESSAGE_LIMIT,
    mergeArchiveMessages,
    mergeMessageRows,
    trimAccountMessages,
    type Account,
    type Conversation,
    type Message,
    type SessionKey,
} from './model';
import { deduplicateMessages } from './messageIdentity';
import { isInlineImageReference } from './imageSource';

const sessionKey = (value: string): value is SessionKey => /^(group|private):[0-9]+$/.test(value);

export function archiveOf(account: Account): ChatArchive {
    const conversations = Object.values(account.conversations)
        .sort(
            (a, b) =>
                Number(b.pinned || b.boxed) - Number(a.pinned || a.boxed) || b.lastAt - a.lastAt,
        )
        .slice(0, 1000);
    const keys = new Set(conversations.map((c) => c.key));
    return {
        v: 1,
        selfId: account.selfId,
        conversations: conversations.map((c) => ({ ...c, boxed: c.type === 'group' && !!c.boxed })),
        messages: mergeArchiveMessages(account.archiveMessages ?? [], account.messages)
            .filter((m) => keys.has(m.session))
            .slice(-MESSAGE_LIMIT)
            .map(({ gapBefore: _gapBefore, ...m }) => ({
                ...m,
                segments: m.segments.map((segment) => {
                    const data = { ...segment.data };
                    delete data.inline_ref;
                    for (const field of ['file', 'local_file', 'url', 'base64']) {
                        if (
                            field === 'base64' ||
                            isInlineImageReference(data[field]) ||
                            /^(base64:\/\/|data:image\/)/i.test(text(data[field]))
                        )
                            delete data[field];
                    }
                    return { ...segment, data };
                }),
                status: m.status === 'sending' ? 'unknown' : m.status,
            })),
    };
}

export function restoreArchive(state: Account, archive: ChatArchive): Account {
    if (archive.v !== 1 || archive.selfId !== state.selfId)
        throw new Error('聊天档案版本或账号不匹配');
    const conversations: Record<string, Conversation> = {};
    for (const c of archive.conversations) {
        if (!sessionKey(c.key) || c.key !== `${c.type}:${c.id}`)
            throw new Error('聊天档案会话标识无效');
        conversations[c.key] = { ...c, key: c.key };
    }
    const byKey = new Map<string, Message>();
    const identity = (m: { session: string; id?: string; key: string }) =>
        m.id ? `${m.session}/${m.id}` : m.key;
    for (const m of archive.messages) {
        if (!sessionKey(m.session) || !conversations[m.session]) continue;
        byKey.set(identity(m), {
            ...m,
            session: m.session,
            segments: normalizeMessage(m.segments),
            status: m.status === 'sending' ? 'unknown' : m.status,
        });
    }
    for (const m of state.archiveMessages ?? state.messages) byKey.set(identity(m), m);
    for (const m of state.messages) byKey.set(identity(m), m);
    for (const [key, live] of Object.entries(state.conversations)) {
        const saved = conversations[key];
        conversations[key] = saved
            ? {
                  ...saved,
                  ...live,
                  pinned: saved.pinned || live.pinned,
                  boxed: live.boxed ?? saved.boxed,
                  unread: state.active === key ? 0 : Math.max(saved.unread, live.unread),
                  lastAt: Math.max(saved.lastAt, live.lastAt),
                  preview: live.lastAt >= saved.lastAt ? live.preview : saved.preview,
              }
            : live;
    }
    const messages = mergeMessageRows([], deduplicateMessages([...byKey.values()]));
    const latest = new Map<SessionKey, Message>();
    for (const message of messages) latest.set(message.session, message);
    for (const [key, message] of latest) {
        const conversation = conversations[key];
        if (
            conversation &&
            !message.recalled &&
            message.at >= conversation.lastAt &&
            message.segments.some((segment) => segment.type === 'markdown')
        )
            conversations[key] = { ...conversation, preview: messagePreview(message.segments) };
    }
    return trimAccountMessages({
        ...state,
        conversations,
        messages,
        archiveMessages: mergeArchiveMessages([], messages),
    });
}

export function mergeRecentConversations(state: Account, rows: unknown[]): Account {
    let next = state;
    for (const item of rows) {
        const row = record(item);
        const type = row.chatType === 2 ? 'group' : row.chatType === 1 ? 'private' : undefined;
        const peer = id(row.peerUin);
        if (!type || !/^[0-9]+$/.test(peer)) continue;
        const key: SessionKey = `${type}:${peer}`;
        const current = next.conversations[key];
        const last = record(row.lastestMsg);
        const rawTime = Number(row.msgTime);
        const at = Number.isFinite(rawTime) && rawTime >= 0 ? rawTime * 1000 : 0;
        const preview = messagePreview(normalizeMessage(last.message ?? last.raw_message));
        const conversation: Conversation = {
            ...(current ?? { unread: 0, pinned: false, lastAt: at, preview }),
            key,
            type,
            id: peer,
            name:
                current?.name && current.name !== peer
                    ? current.name
                    : text(row.remark) || text(row.peerName) || peer,
        };
        if (at > conversation.lastAt) {
            conversation.lastAt = at;
            conversation.preview = preview || conversation.preview;
        }
        next = { ...next, conversations: { ...next.conversations, [key]: conversation } };
        if (id(last.message_id) && (last.message !== undefined || last.raw_message !== undefined))
            next = ingestMessage(
                next,
                {
                    ...last,
                    message_type: type,
                    ...(type === 'group'
                        ? { group_id: peer }
                        : { target_id: peer, user_id: id(last.user_id) || peer }),
                },
                true,
            );
    }
    return next;
}
