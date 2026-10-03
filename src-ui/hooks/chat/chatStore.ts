// 按 Bot 和登录身份分区；页面离开后保留内存消息与订阅。
import { useSyncExternalStore } from 'react';
import { chatService } from '../../core/services/chat.service';
import { chatArchiveService } from '../../core/services/chat-archive.service';
import { archiveOf, restoreArchive, mergeRecentConversations } from '../../core/domain/chat/archive';
import { accountKey, emptyAccount, ingestMessage, openConversation, setDraft, addPending, settleSend, parseContact, record, id, text, EMPTY_DRAFT, type Account, type Contact, type Draft, type SessionKey } from '../../core/domain/chat/model';
import { buildMessageSegments } from '../../core/domain/debug/composerModel';
import { localFileTokenFor } from '../../core/domain/debug/streamActions';
import { debugErrorCopy } from '../../core/domain/debug/errorCopy';
import { errorText } from '../../core/domain/errors';
import type { Segment } from '../../core/domain/debug/segments';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import type { DebugReceiverState } from '../../core/ipc/generated/debug/DebugReceiverState';
import type { DebugCallResponse } from '../../core/ipc/generated/debug/DebugCallResponse';

export interface ChatSnapshot { account: Account; contacts: Contact[]; connection: DebugReceiverState; error: string; contactsLoading: boolean; hydrated: boolean; archiveError: string; recentLoading: boolean; recentError: string; history: Record<string, { loading: boolean; loaded: boolean; done: boolean; error: string }> }
type Transport = Pick<typeof chatService, 'call' | 'subscribe' | 'unsubscribe'>;
type ArchivePort = Pick<typeof chatArchiveService, 'load' | 'save'>;
function dataOf(response: DebugCallResponse): unknown {
    if (response.result.kind === 'err') { const copy = debugErrorCopy(response.result.error); throw new Error([copy.title, copy.detail].filter(Boolean).join('：')); }
    const result = response.result.outcome;
    if (!result.ok) throw new Error(result.wording || result.message || `请求失败（${result.retcode}）`);
    if (result.truncated) throw new Error('返回内容过大，请缩小范围后重试');
    return result.data;
}

export class ChatAccountStore {
    private snapshot: ChatSnapshot;
    private listeners = new Set<() => void>();
    private epoch = 0;
    private subscription: string | null = null;
    private connecting = false;
    private reading: SessionKey | null = null;
    private sends = new Map<SessionKey, number>();
    private historyCursor = new Map<SessionKey, string>();
    private contactsRequest: Promise<void> | null = null;
    private restoreRequest: Promise<void> | null = null;
    private saveRequest: Promise<void> | null = null;
    private archiveDirty = false;
    private recentRequest: Promise<void> | null = null;
    private recentEpoch = -1;
    target: DebugTarget;

    constructor(target: DebugTarget, private transport: Transport = chatService, private archive: ArchivePort | undefined = transport === chatService ? chatArchiveService : undefined) {
        this.target = target;
        this.snapshot = { account: emptyAccount(String(target.qq_id)), contacts: [], connection: { state: 'stopped', reason: '尚未连接' }, error: '', contactsLoading: false, hydrated: !archive, archiveError: '', recentLoading: false, recentError: '', history: {} };
    }
    getSnapshot = () => this.snapshot;
    subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
    private update(patch: Partial<ChatSnapshot>) {
        const previous = this.snapshot.account;
        if (patch.account && patch.account.messages !== previous.messages) {
            const retained = new Set(patch.account.messages.map(message => message.key));
            const retainedIds = new Set(patch.account.messages.filter(message => message.id).map(message => `${message.session}/${message.id}`));
            const evicted = new Set(previous.messages.filter(message => !retained.has(message.key) && (!message.id || !retainedIds.has(`${message.session}/${message.id}`))).map(message => message.session));
            if (evicted.size) {
                const history = { ...(patch.history ?? this.snapshot.history) };
                for (const key of evicted) {
                    this.historyCursor.delete(key);
                    if (history[key]) history[key] = { loading: history[key].loading, loaded: false, done: false, error: '' };
                }
                patch = { ...patch, history };
            }
        }
        this.snapshot = { ...this.snapshot, ...patch };
        for (const listener of this.listeners) listener();
        if (patch.account && (patch.account.messages !== previous.messages || patch.account.conversations !== previous.conversations)) {
            this.archiveDirty = true;
            if (this.snapshot.hydrated && !this.snapshot.archiveError) void this.flushArchive();
        }
    }
    restore(): Promise<void> {
        if (this.restoreRequest) return this.restoreRequest;
        if (this.snapshot.hydrated || !this.archive) return Promise.resolve();
        this.update({ archiveError: '' });
        this.restoreRequest = this.archive.load(this.target.bot_id, this.snapshot.account.selfId).then(saved => {
            const account = saved ? restoreArchive(this.snapshot.account, saved) : this.snapshot.account;
            this.update({ account, hydrated: true });
            void this.flushArchive();
        }).catch(error => this.update({ archiveError: `聊天记录读取失败：${errorText(error)}` })).finally(() => { this.restoreRequest = null; });
        return this.restoreRequest;
    }
    flushArchive(): Promise<void> {
        if (this.saveRequest) return this.saveRequest;
        if (!this.archive || !this.snapshot.hydrated || !this.archiveDirty) return Promise.resolve();
        const archive = this.archive;
        this.saveRequest = Promise.resolve().then(async () => {
            while (this.archiveDirty) {
                this.archiveDirty = false;
                try {
                    await archive.save(this.target.bot_id, this.snapshot.account.selfId, archiveOf(this.snapshot.account));
                    this.update({ archiveError: '' });
                } catch (error) {
                    this.archiveDirty = true;
                    this.update({ archiveError: `聊天记录保存失败：${errorText(error)}` });
                    break;
                }
            }
        }).finally(() => {
            this.saveRequest = null;
            if (this.archiveDirty && !this.snapshot.archiveError) return this.flushArchive();
            return undefined;
        });
        return this.saveRequest;
    }
    async initialize() {
        await this.restore();
        if (!this.target.running || this.target.online === false) { await this.disconnect(); return; }
        await this.connect();
        if (this.snapshot.connection.state === 'connected') await Promise.all([this.loadContacts(), this.loadRecent()]);
    }
    loadRecent(retry = false): Promise<void> {
        if (this.recentRequest) return this.recentRequest;
        if (this.snapshot.connection.state !== 'connected' || (!retry && this.recentEpoch === this.epoch)) return Promise.resolve();
        const epoch = this.epoch;
        this.update({ recentLoading: true, recentError: '' });
        const request = this.transport.call(this.target.bot_id, 'get_recent_contact', { count: 100 }).then(dataOf).then(data => {
            if (epoch !== this.epoch) return;
            if (!Array.isArray(data)) throw new Error('此协议未返回最近会话列表');
            const merged = mergeRecentConversations(this.snapshot.account, data);
            const account = { ...merged, conversations: { ...merged.conversations } };
            for (const contact of this.snapshot.contacts) if (account.conversations[contact.key]) account.conversations[contact.key] = { ...account.conversations[contact.key], ...contact };
            this.account(account); this.recentEpoch = epoch;
        }).catch(error => { if (epoch === this.epoch) this.update({ recentError: `最近会话同步失败：${errorText(error)}` }); }).finally(() => {
            if (this.recentRequest === request) { this.recentRequest = null; this.update({ recentLoading: false }); }
        });
        this.recentRequest = request; return request;
    }
    private account(account: Account) { this.update({ account }); }
    setReading(key: SessionKey | null) { this.reading = key; if (key && this.snapshot.account.conversations[key]?.unread) this.open(this.snapshot.account.conversations[key]); }
    open(contact: Contact) { this.account(openConversation(this.snapshot.account, contact)); }
    close(key: SessionKey) { if (this.snapshot.account.active === key) { this.reading = null; this.account({ ...this.snapshot.account, active: null }); } }
    draft(key: SessionKey, draft: Draft) { this.account(setDraft(this.snapshot.account, key, draft)); }
    pin(key: SessionKey) { const state = this.snapshot.account; const c = state.conversations[key]; if (c) this.account({ ...state, conversations: { ...state.conversations, [key]: { ...c, pinned: !c.pinned } } }); }
    markRead(key: SessionKey) {
        const state = this.snapshot.account; const conversation = state.conversations[key];
        if (!conversation?.unread) return;
        this.account({ ...state, conversations: { ...state.conversations, [key]: { ...conversation, unread: 0 } } });
    }
    box(key: SessionKey) { const state = this.snapshot.account; const c = state.conversations[key]; if (c?.type === 'group') this.account({ ...state, conversations: { ...state.conversations, [key]: { ...c, boxed: !c.boxed } } }); }
    private interruptPending() {
        this.contactsRequest = null;
        this.recentRequest = null;
        const account = this.snapshot.account;
        this.update({
            contactsLoading: false, recentLoading: false,
            history: Object.fromEntries(Object.entries(this.snapshot.history).map(([key, value]) => [key, { ...value, loading: false }])),
            account: { ...account, messages: account.messages.map(message => message.status === 'sending' ? { ...message, status: 'unknown', error: '连接已断开，发送结果待确认' } : message) },
        });
    }
    async disconnect() {
        this.epoch++; this.connecting = false; this.reading = null;
        const subscription = this.subscription; this.subscription = null;
        this.interruptPending();
        this.update({ connection: { state: 'stopped', reason: '已断开' } });
        if (subscription) {
            try { await this.transport.unsubscribe(subscription); }
            catch (error) { this.update({ error: errorText(error) }); }
        }
    }
    async connect() {
        if (this.connecting || !this.target.running || this.target.online === false) return;
        if (this.subscription && this.snapshot.connection.state !== 'stopped') return;
        const old = this.subscription; this.subscription = null;
        const epoch = ++this.epoch; this.connecting = true;
        this.interruptPending();
        this.update({ connection: { state: 'connecting' }, error: '' });
        try {
            if (old) await this.transport.unsubscribe(old);
            if (epoch !== this.epoch) return;
            const result = await this.transport.subscribe(this.target.bot_id, batch => {
                if (epoch !== this.epoch || batch.bot_id !== this.target.bot_id || batch.v !== 1) return;
                let account = this.snapshot.account; let connection = this.snapshot.connection;
                for (const event of batch.events) {
                    if (event.seq <= account.lastSeq) continue;
                    const body = event.body;
                    if (body.kind === 'ob11') {
                        const self = id(body.payload.self_id);
                        if (self && self !== account.selfId) continue;
                        account = ingestMessage(account, body.payload, false, typeof document !== 'undefined' && document.visibilityState === 'visible' ? this.reading : null);
                    } else if (body.kind === 'receiver') connection = body.state;
                    else if (body.kind === 'gap' || body.kind === 'dropped') account = { ...account, gap: true };
                    account = { ...account, lastSeq: event.seq };
                }
                const becameConnected = connection.state === 'connected' && this.snapshot.connection.state !== 'connected';
                this.update({ account, connection });
                if (becameConnected) { void this.loadContacts(); void this.loadRecent(); }
            });
            if (epoch !== this.epoch) { await this.transport.unsubscribe(result.subscription_id); return; }
            this.subscription = result.subscription_id;
            this.update({ connection: result.receiver.state });
        } catch (error) { if (epoch === this.epoch) this.update({ error: errorText(error), connection: { state: 'stopped', reason: errorText(error) } }); }
        finally { if (epoch === this.epoch) this.connecting = false; }
    }
    loadContacts(): Promise<void> {
        if (this.contactsRequest) return this.contactsRequest;
        const epoch = this.epoch;
        this.update({ contactsLoading: true, error: '' });
        const request = (async () => {
            const results = await Promise.allSettled([
                this.transport.call(this.target.bot_id, 'get_group_list', {}).then(dataOf),
                this.transport.call(this.target.bot_id, 'get_friend_list', {}).then(dataOf),
            ]);
            if (epoch !== this.epoch) return;
            const contacts: Contact[] = []; const errors: string[] = [];
            results.forEach((result, index) => {
                const type = index === 0 ? 'group' : 'private';
                if (result.status === 'rejected') { errors.push(errorText(result.reason)); contacts.push(...this.snapshot.contacts.filter(c => c.type === type)); }
                else if (Array.isArray(result.value)) for (const row of result.value) { const contact = parseContact(row, type); if (contact) contacts.push(contact); }
                else errors.push('联系人返回格式无法识别');
            });
            const account = this.snapshot.account; const conversations = { ...account.conversations };
            for (const contact of contacts) if (conversations[contact.key]) conversations[contact.key] = { ...conversations[contact.key], ...contact };
            this.update({ contacts, account: { ...account, conversations }, error: errors.join('；') });
        })().finally(() => {
            if (this.contactsRequest === request) {
                this.contactsRequest = null;
                this.update({ contactsLoading: false });
            }
        });
        this.contactsRequest = request;
        return request;
    }
    async members(key: SessionKey): Promise<{ id: string; name: string }[]> {
        const value = dataOf(await this.transport.call(this.target.bot_id, 'get_group_member_list', { group_id: this.peer(key) }));
        return Array.isArray(value) ? value.map(record).map(row => ({ id: id(row.user_id), name: text(row.card) || text(row.nickname) || id(row.user_id) })).filter(row => row.id) : [];
    }
    private peer(key: SessionKey): string | number {
        const value = key.slice(key.indexOf(':') + 1);
        if (this.target.backend === 'snowluma') { const numeric = Number(value); if (!Number.isSafeInteger(numeric)) throw new Error('账号标识超出可支持范围'); return numeric; }
        return value;
    }
    ensureHistory(key: SessionKey): Promise<void> | undefined {
        const history = this.snapshot.history[key];
        if (history?.loaded || history?.loading || history?.error) return;
        return this.history(key);
    }
    async history(key: SessionKey): Promise<void> {
        if (this.snapshot.connection.state !== 'connected' || this.snapshot.history[key]?.loading || this.snapshot.history[key]?.done) return;
        const epoch = this.epoch;
        const set = (loading: boolean, done = false, error = '', loaded = this.snapshot.history[key]?.loaded ?? false) => this.update({ history: { ...this.snapshot.history, [key]: { loading, loaded, done, error } } });
        set(true);
        try {
            const group = key.startsWith('group:'); const cursor = this.historyCursor.get(key);
            const params: Record<string, unknown> = { [group ? 'group_id' : 'user_id']: this.peer(key), count: 50, reverse_order: this.target.backend === 'snowluma', disable_get_url: false, parse_mult_msg: true, quick_reply: false, reverseOrder: false };
            if (cursor) {
                if (this.target.backend === 'snowluma' && !Number.isSafeInteger(Number(cursor))) throw new Error('此协议的历史游标超出可支持范围');
                params[this.target.backend === 'snowluma' ? 'message_id' : 'message_seq'] = this.target.backend === 'snowluma' ? Number(cursor) : cursor;
            }
            const data = record(dataOf(await this.transport.call(this.target.bot_id, group ? 'get_group_msg_history' : 'get_friend_msg_history', params)));
            if (epoch !== this.epoch) return;
            // 请求期间缓存若被裁剪，旧游标已不连续，须重新取得最近一页。
            if (cursor && cursor !== this.historyCursor.get(key)) { set(false, false, '', false); return this.history(key); }
            if (!Array.isArray(data.messages)) throw new Error('此通道未返回可识别的消息历史');
            const rows = data.messages.map(record).sort((a, b) => Number(a.time || 0) - Number(b.time || 0));
            let account = this.snapshot.account;
            for (const row of rows) account = ingestMessage(account, { ...row, message_type: group ? 'group' : 'private', ...(group ? { group_id: this.peer(key) } : { target_id: this.peer(key) }) }, true);
            // NapCat 的参数虽名为 message_seq，实际按短 message_id 查内部 MsgId。
            const next = id(rows[0]?.message_id);
            if (next) this.historyCursor.set(key, next);
            this.account(account); set(false, !next || next === cursor || rows.length < 50, '', true);
        } catch (error) { if (epoch === this.epoch) set(false, false, errorText(error)); }
    }
    async send(key: SessionKey) {
        if (this.sends.get(key) === this.epoch || !this.target.running || this.target.online === false || this.snapshot.connection.state !== 'connected') return;
        const draft = this.snapshot.account.drafts[key] ?? EMPTY_DRAFT;
        if (!draft.text.trim() && !draft.attachments.length) return;
        const epoch = this.epoch;
        const group = key.startsWith('group:'); const peer = { [group ? 'group_id' : 'user_id']: this.peer(key) };
        const segments = buildMessageSegments(draft.text, draft.mentions ?? []);
        if (draft.reply) segments.unshift({ type: 'reply', data: { id: draft.reply.id } });
        for (const attachment of draft.attachments) {
            if (attachment.type === 'face') segments.push({ type: 'face', data: { id: attachment.id } });
            if (attachment.type === 'image') segments.push({ type: 'image', data: { file: /^(base64:\/\/|https?:\/\/)/i.test(attachment.path) ? attachment.path : localFileTokenFor(attachment.path), ...(attachment.subType ? { sub_type: attachment.subType } : {}) } });
        }
        const operations: { action: string; params: unknown; segments: Segment[] }[] = [];
        if (segments.some(s => s.type !== 'reply')) operations.push({ action: group ? 'send_group_msg' : 'send_private_msg', params: { ...peer, message: segments }, segments });
        for (const file of draft.attachments) if (file.type === 'file') operations.push({ action: group ? 'upload_group_file' : 'upload_private_file', params: { ...peer, file: localFileTokenFor(file.path), name: file.name, upload_file: true }, segments: [{ type: 'file', data: { name: file.name, file: localFileTokenFor(file.path) } }] });
        this.sends.set(key, epoch);
        this.draft(key, EMPTY_DRAFT);
        try {
            for (const operation of operations) {
                const requestId = crypto.randomUUID();
                this.account(addPending(this.snapshot.account, key, requestId, operation.segments, Date.now()));
                if (epoch !== this.epoch) {
                    this.account(settleSend(this.snapshot.account, requestId, { state: 'failed', error: '发送已中止，此条尚未提交' }));
                    continue;
                }
                try {
                    const response = await this.transport.call(this.target.bot_id, operation.action, operation.params, requestId);
                    if (epoch !== this.epoch) continue;
                    if (response.result.kind === 'err') {
                        const error = response.result.error; const unknown = ['timeout', 'cancelled', 'transport', 'internal'].includes(error.kind);
                        const copy = debugErrorCopy(error);
                        this.account(settleSend(this.snapshot.account, requestId, { state: unknown ? 'unknown' : 'failed', error: [copy.title, copy.detail].filter(Boolean).join('：') }));
                    } else {
                        const result = response.result.outcome;
                        this.account(settleSend(this.snapshot.account, requestId, { state: result.ok ? 'sent' : 'failed', id: id(record(result.data).message_id) || undefined, fileId: operation.action.startsWith('upload_') ? id(record(result.data).file_id) || undefined : undefined, error: result.ok ? undefined : result.wording || result.message || `发送失败（${result.retcode}）` }));
                    }
                } catch (error) { if (epoch === this.epoch) this.account(settleSend(this.snapshot.account, requestId, { state: 'unknown', error: errorText(error) })); }
            }
        } finally { if (this.sends.get(key) === epoch) this.sends.delete(key); }
    }
}

const accounts = new Map<string, ChatAccountStore>();
export function chatAccount(target: DebugTarget): ChatAccountStore {
    const key = accountKey(target.bot_id, String(target.qq_id));
    let store = accounts.get(key);
    if (!store) { store = new ChatAccountStore(target); accounts.set(key, store); }
    store.target = target; return store;
}
export function reconcileChatAccounts(targets: DebugTarget[]) {
    const live = new Set(targets.map(t => accountKey(t.bot_id, String(t.qq_id))));
    for (const [key, store] of accounts) {
        if (!live.has(key)) { void store.disconnect(); accounts.delete(key); }
        else { const target = targets.find(t => t.bot_id === store.target.bot_id); if (target) store.target = target; }
    }
}
export function useChatSnapshot(store: ChatAccountStore) { return useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot); }
