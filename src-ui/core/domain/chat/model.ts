// 聊天会话的内存投影；标识在协议边界统一为字符串。
import { normalizeMessage, messagePreview, type Segment } from '../debug/segments';
import type { Mention } from '../debug/composerModel';

export type SessionKey = `group:${string}` | `private:${string}`;
export interface Contact { key: SessionKey; type: 'group' | 'private'; id: string; name: string; members?: number }
export interface Conversation extends Contact { unread: number; pinned: boolean; lastAt: number; preview: string; boxed?: boolean }
export type SendStatus = 'sending' | 'sent' | 'failed' | 'unknown';
export interface Message { key: string; session: SessionKey; id?: string; sequence?: string; fileId?: string; requestId?: string; senderId: string; senderName: string; at: number; mine: boolean; segments: Segment[]; status: SendStatus; error?: string; recalled?: boolean }
export type Attachment = { key: string; name: string } & ({ type: 'image' | 'file'; path: string; subType?: 1 } | { type: 'face'; id: string });
export interface Reply { id: string; name: string; preview: string }
export interface Draft { text: string; attachments: Attachment[]; reply: Reply | null; mentions?: Mention[] }
export interface Account { selfId: string; active: SessionKey | null; conversations: Record<string, Conversation>; messages: Message[]; drafts: Record<string, Draft>; lastSeq: number; gap: boolean }
export const EMPTY_DRAFT: Draft = { text: '', attachments: [], reply: null };
export const MESSAGE_LIMIT = 5000;
// 上翻中的会话不裁掉刚读到的旧消息，其余会话仍只留近期缓冲。
export function retainMessages(messages: Message[], reading?: SessionKey | null, loading?: SessionKey | null): Message[] {
    const recentStart = Math.max(0, messages.length - MESSAGE_LIMIT);
    return messages.filter((message, index) => index >= recentStart || message.session === reading || message.session === loading);
}
export const accountKey = (botId: string, selfId: string) => JSON.stringify([botId, selfId]);
export const record = (v: unknown): Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v) ? v as Record<string, unknown> : {};
export const id = (v: unknown): string => typeof v === 'string' ? v : typeof v === 'number' && Number.isSafeInteger(v) ? String(v) : '';
export const text = (v: unknown): string => typeof v === 'string' ? v : '';
export function emptyAccount(selfId: string): Account { return { selfId, active: null, conversations: {}, messages: [], drafts: {}, lastSeq: 0, gap: false }; }
export function contactFromKey(key: SessionKey): Contact { const [type, ...parts] = key.split(':'); const value = parts.join(':'); return { key, type: type === 'group' ? 'group' : 'private', id: value, name: value }; }
export function parseContact(raw: unknown, type: Contact['type']): Contact | null {
    const row = record(raw); const value = id(type === 'group' ? row.group_id : row.user_id);
    if (!value) return null;
    return { key: `${type}:${value}`, type, id: value, name: text(row.group_name) || text(row.remark) || text(row.nickname) || value, members: typeof row.member_count === 'number' ? row.member_count : undefined };
}
function conversation(state: Account, contact: Contact): Conversation {
    const existing: Conversation | undefined = state.conversations[contact.key];
    return existing ? { ...existing, ...contact } : { unread: 0, pinned: false, lastAt: 0, preview: '', ...contact };
}
export function openConversation(state: Account, contact: Contact): Account {
    return { ...state, active: contact.key, conversations: { ...state.conversations, [contact.key]: { ...conversation(state, contact), unread: 0 } } };
}
export function setDraft(state: Account, key: SessionKey, draft: Draft): Account { return { ...state, drafts: { ...state.drafts, [key]: draft } }; }
export function ingestMessage(state: Account, raw: unknown, historical = false, reading: SessionKey | null = null): Account {
    const row = record(raw);
    if (row.notice_type === 'group_recall' || row.notice_type === 'friend_recall') {
        const messageId = id(row.message_id);
        const session = row.notice_type === 'group_recall' ? `group:${id(row.group_id)}` : `private:${id(row.user_id)}`;
        return { ...state, messages: state.messages.map(m => m.session === session && m.id === messageId ? { ...m, recalled: true } : m) };
    }
    if (row.message_type !== 'group' && row.message_type !== 'private') return state;
    const sender = record(row.sender); const senderId = id(sender.user_id) || id(row.user_id);
    const mine = row.post_type === 'message_sent' || (!!state.selfId && senderId === state.selfId);
    const peer = row.message_type === 'group' ? id(row.group_id) : mine ? id(row.target_id) || id(row.peer_id) || id(row.user_id) : id(row.user_id) || senderId;
    if (!peer) return state;
    const session: SessionKey = `${row.message_type}:${peer}`; const messageId = id(row.message_id);
    const at = typeof row.time === 'number' ? row.time * 1000 : Date.now();
    const segments = normalizeMessage(row.message ?? row.raw_message);
    const fileId = segments.length === 1 && segments[0].type === 'file' ? id(segments[0].data.file_id) : '';
    const key = messageId ? `${session}/${messageId}` : `${session}/event/${id(row.message_seq) || at}/${senderId}/${messagePreview(segments)}`;
    const existing = state.messages.find(m => m.session === session && ((messageId ? m.id === messageId : m.key === key) || (mine && fileId && m.requestId && !m.id && m.fileId === fileId)));
    if (existing) {
        if (!existing.requestId) {
            const sequence = id(row.message_seq);
            return sequence && existing.sequence !== sequence ? { ...state, messages: state.messages.map(m => m === existing ? { ...m, sequence } : m) } : state;
        }
        return { ...state, messages: state.messages.map(message => message === existing ? { ...message, id: messageId || message.id, fileId: fileId || message.fileId, segments, status: 'sent', error: undefined } : message) };
    }
    const name = text(sender.card) || text(sender.nickname) || senderId;
    const contact = state.conversations[session] ?? { ...contactFromKey(session), name: text(row.group_name) || (row.message_type === 'private' && !mine ? name : peer) };
    const current = conversation(state, contact);
    const message: Message = { key, session, id: messageId || undefined, sequence: id(row.message_seq) || undefined, fileId: fileId || undefined, senderId, senderName: name, mine, segments, at, status: 'sent' };
    const messages = [...state.messages, message].sort((a, b) => a.at - b.at);
    return { ...state, gap: state.gap || messages.length > MESSAGE_LIMIT, messages: retainMessages(messages, state.active, historical ? session : null), conversations: { ...state.conversations, [session]: { ...current, unread: current.unread + (!historical && !mine && reading !== session ? 1 : 0), lastAt: Math.max(at, current.lastAt), preview: at >= current.lastAt ? messagePreview(segments) : current.preview } } };
}
export function addPending(state: Account, session: SessionKey, requestId: string, segments: Segment[], at: number): Account {
    const current = conversation(state, state.conversations[session] ?? contactFromKey(session));
    const message: Message = { key: `pending/${requestId}`, requestId, session, senderId: state.selfId, senderName: '我', mine: true, segments, at, status: 'sending' };
    return { ...state, messages: retainMessages([...state.messages, message], state.active), conversations: { ...state.conversations, [session]: { ...current, lastAt: at, preview: messagePreview(segments) } } };
}
export function settleSend(state: Account, requestId: string, result: { state: SendStatus; id?: string; fileId?: string; error?: string }): Account {
    const pending = state.messages.find(m => m.requestId === requestId);
    if (!pending) return state;
    const echo = state.messages.find(m => m !== pending && m.mine && m.session === pending.session && ((result.id && m.id === result.id) || (result.fileId && !m.requestId && m.fileId === result.fileId)));
    return { ...state, messages: state.messages.filter(m => m !== echo).map(m => m === pending ? { ...m, ...(echo ?? {}), key: pending.key, requestId, id: result.id || echo?.id || m.id, fileId: result.fileId || echo?.fileId || m.fileId, status: result.state, error: result.error } : m).sort((a, b) => a.at - b.at) };
}
