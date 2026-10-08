// 按 Bot 和登录身份分区，界面离开后释放接收租约。
import { useSyncExternalStore } from 'react';
import { cancelChatScreenshot } from './useChatScreenshot';
import { chatService } from '../../core/services/chat.service';
import { chatArchiveService } from '../../core/services/chat-archive.service';
import { chatDesktopService } from '../../core/services/chat-desktop.service';
import { qqFaceService } from '../../core/services/qq-face.service';
import {
    inlineImageService,
    isInlineImageReference,
} from '../../core/services/inline-image.service';
import { qqFaceLarge } from '../../core/domain/chat/qqFaces';
import { createChatMediaService } from '../../core/services/chat-media.service';
import type { ChatAccountView } from '../../core/ipc/generated/chat/ChatAccountView';
import type { ChatViewState } from '../../core/ipc/generated/chat/ChatViewState';
import type { ChatReadingPosition } from '../../core/ipc/generated/chat/ChatReadingPosition';
import {
    archiveOf,
    restoreArchive,
    mergeRecentConversations,
} from '../../core/domain/chat/archive';
import { recoverDraft } from '../../core/domain/chat/recoverDraft';
import { markdownContent } from '../../core/domain/debug/markdown';
import {
    accountKey,
    emptyAccount,
    ingestMessages,
    mergeArchiveMessages,
    mergeMessageRows,
    trimAccountMessages,
    openConversation,
    contactFromKey,
    setDraft,
    addPending,
    settleSend,
    parseContact,
    parseFriendCategories,
    record,
    id,
    text,
    EMPTY_DRAFT,
    type Account,
    type Contact,
    type Draft,
    type Message,
    type SessionKey,
} from '../../core/domain/chat/model';
import { HISTORY_PAGE_SIZE, type ReadingAnchor } from '../../core/domain/chat/messageWorkingSet';
import { buildMessageSegments } from '../../core/domain/debug/composerModel';
import { localFileTokenFor } from '../../core/domain/debug/streamActions';
import { debugErrorCopy } from '../../core/domain/debug/errorCopy';
import { errorText } from '../../core/domain/errors';
import { normalizeMessage, type Segment } from '../../core/domain/debug/segments';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import type { DebugReceiverState } from '../../core/ipc/generated/debug/DebugReceiverState';
import type { DebugCallResponse } from '../../core/ipc/generated/debug/DebugCallResponse';
import {
    canForwardMessage,
    canRecallMessage,
    canRepeatMessage,
    ChatForwardError,
    FORWARD_MESSAGE_LIMIT,
} from '../../core/domain/chat/messageTransfer';

export interface ChatSnapshot {
    account: Account;
    contacts: Contact[];
    connection: DebugReceiverState;
    error: string;
    contactsLoading: boolean;
    hydrated: boolean;
    archiveError: string;
    recentLoading: boolean;
    recentError: string;
    history: Record<string, { loading: boolean; loaded: boolean; done: boolean; error: string }>;
}
type Transport = Pick<typeof chatService, 'call' | 'subscribe' | 'unsubscribe'>;
type ArchivePort = Pick<typeof chatArchiveService, 'load' | 'save'>;
type SendOperation = {
    action: string;
    params: unknown;
    segments: Segment[];
    resolveImages?: boolean;
    sourceMessageId?: string;
};
function dataOf(response: DebugCallResponse): unknown {
    if (response.result.kind === 'err') {
        const copy = debugErrorCopy(response.result.error);
        throw new Error([copy.title, copy.detail].filter(Boolean).join('：'));
    }
    const result = response.result.outcome;
    if (!result.ok)
        throw new Error(result.wording || result.message || `请求失败（${result.retcode}）`);
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
    private recalls = new Map<string, number>();
    private historyCursor = new Map<SessionKey, string>();
    private pagingAnchors = new Map<SessionKey, ReadingAnchor>();
    private contactsRequest: Promise<void> | null = null;
    private restoreRequest: Promise<void> | null = null;
    private saveRequest: Promise<void> | null = null;
    private archiveDirty = false;
    private recentRequest: Promise<void> | null = null;
    private recentEpoch = -1;
    private saveTimer: ReturnType<typeof setTimeout> | undefined;
    private positions: Record<string, number> = {};
    private readingPositions: Record<string, ChatReadingPosition> = {};
    private initialReadingPositions: Record<string, ChatReadingPosition> = {};
    private timelineReaders = new Map<string, () => ChatReadingPosition | undefined>();
    releaseRequest: Promise<void> | null = null;
    releaseWhenIdle = false;
    viewRevision = 0;
    target: DebugTarget;

    constructor(
        target: DebugTarget,
        private transport: Transport = chatService,
        private archive: ArchivePort | undefined = transport === chatService
            ? chatArchiveService
            : undefined,
    ) {
        this.target = target;
        this.snapshot = {
            account: emptyAccount(String(target.qq_id)),
            contacts: [],
            connection: { state: 'stopped', reason: '尚未连接' },
            error: '',
            contactsLoading: false,
            hydrated: !archive,
            archiveError: '',
            recentLoading: false,
            recentError: '',
            history: {},
        };
    }
    getSnapshot = () => this.snapshot;
    subscribe = (listener: () => void) => {
        this.listeners.add(listener);
        return () => {
            this.listeners.delete(listener);
        };
    };
    private update(patch: Partial<ChatSnapshot>) {
        const previous = this.snapshot.account;
        if (
            patch.account &&
            (patch.account.messages !== previous.messages ||
                patch.account.active !== previous.active)
        ) {
            for (const [key, read] of this.timelineReaders) {
                const value = read();
                if (value) this.readingPositions[key] = value;
            }
            const active = patch.account.active;
            const saved = active && this.readingPositions[active];
            const anchor = active
                ? (this.pagingAnchors.get(active) ??
                  (saved ? { session: active, ...saved } : undefined))
                : undefined;
            let account =
                patch.account.archiveMessages !== previous.archiveMessages
                    ? patch.account
                    : {
                          ...patch.account,
                          archiveMessages: mergeArchiveMessages(
                              previous.archiveMessages ?? previous.messages,
                              patch.account.messages,
                          ),
                      };
            if (
                anchor &&
                !anchor.atBottom &&
                !account.messages.some(
                    (message) =>
                        message.session === anchor.session &&
                        (message.key === anchor.messageKey ||
                            (!!anchor.messageId && message.id === anchor.messageId)),
                )
            ) {
                const savedMessages =
                    account.archiveMessages?.filter(
                        (message) => message.session === anchor.session,
                    ) ?? [];
                const index = savedMessages.findIndex(
                    (message) =>
                        message.key === anchor.messageKey ||
                        (!!anchor.messageId && message.id === anchor.messageId),
                );
                if (index >= 0)
                    account = {
                        ...account,
                        messages: mergeMessageRows(
                            savedMessages.slice(
                                Math.max(0, index - HISTORY_PAGE_SIZE),
                                index + HISTORY_PAGE_SIZE * 2,
                            ),
                            account.messages,
                        ),
                    };
            }
            patch = { ...patch, account: trimAccountMessages(account, anchor) };
        }
        if (patch.account && patch.account.messages !== previous.messages) {
            const retained = new Set(patch.account.messages.map((message) => message.key));
            const retainedIds = new Set(
                patch.account.messages
                    .filter((message) => message.id)
                    .map((message) => `${message.session}/${message.id}`),
            );
            const evicted = new Set(
                previous.messages
                    .filter(
                        (message) =>
                            !retained.has(message.key) &&
                            (!message.id || !retainedIds.has(`${message.session}/${message.id}`)),
                    )
                    .map((message) => message.session),
            );
            if (evicted.size) {
                const history = { ...(patch.history ?? this.snapshot.history) };
                for (const key of evicted) {
                    const oldest = patch.account.messages.find(
                        (message) => message.session === key && message.id,
                    );
                    const before = previous.messages.find(
                        (message) => message.session === key && message.id,
                    );
                    if (oldest?.id) this.historyCursor.set(key as SessionKey, oldest.id);
                    else this.historyCursor.delete(key as SessionKey);
                    if (history[key] && (!oldest || (before && oldest.at > before.at)))
                        history[key] = {
                            loading: history[key].loading,
                            loaded: !!oldest,
                            done: false,
                            error: '',
                        };
                }
                patch = { ...patch, history };
            }
        }
        this.snapshot = { ...this.snapshot, ...patch };
        for (const listener of this.listeners) listener();
        if (
            patch.account &&
            (patch.account.messages !== previous.messages ||
                patch.account.conversations !== previous.conversations)
        ) {
            this.archiveDirty = true;
            if (this.snapshot.hydrated && !this.snapshot.archiveError && !this.saveTimer) {
                this.saveTimer = setTimeout(() => {
                    this.saveTimer = undefined;
                    void this.flushArchive();
                }, 300);
            }
        }
    }
    restore(): Promise<void> {
        if (this.restoreRequest) return this.restoreRequest;
        if (this.snapshot.hydrated || !this.archive) return Promise.resolve();
        this.update({ archiveError: '' });
        this.restoreRequest = this.archive
            .load(this.target.bot_id, this.snapshot.account.selfId)
            .then((saved) => {
                const account = saved
                    ? restoreArchive(this.snapshot.account, {
                          ...saved,
                          messages: saved.messages.map((message) => ({
                              ...message,
                              segments: this.externalizeSegments(
                                  normalizeMessage(message.segments),
                              ),
                          })),
                      })
                    : this.snapshot.account;
                this.account(account);
                this.update({ hydrated: true });
                void this.flushArchive();
            })
            .catch((error) =>
                this.update({ archiveError: `聊天记录读取失败：${errorText(error)}` }),
            )
            .finally(() => {
                this.restoreRequest = null;
            });
        return this.restoreRequest;
    }
    flushArchive(): Promise<void> {
        clearTimeout(this.saveTimer);
        this.saveTimer = undefined;
        if (this.saveRequest) return this.saveRequest;
        if (!this.archive || !this.snapshot.hydrated || !this.archiveDirty)
            return Promise.resolve();
        const archive = this.archive;
        this.saveRequest = Promise.resolve()
            .then(async () => {
                while (this.archiveDirty) {
                    this.archiveDirty = false;
                    try {
                        await archive.save(
                            this.target.bot_id,
                            this.snapshot.account.selfId,
                            archiveOf(this.snapshot.account),
                        );
                        this.update({ archiveError: '' });
                    } catch (error) {
                        this.archiveDirty = true;
                        this.update({ archiveError: `聊天记录保存失败：${errorText(error)}` });
                        break;
                    }
                }
            })
            .finally(() => {
                this.saveRequest = null;
                if (this.archiveDirty && !this.snapshot.archiveError) return this.flushArchive();
                return undefined;
            });
        return this.saveRequest;
    }
    async initialize() {
        if (this.releaseRequest) await this.releaseRequest;
        this.releaseWhenIdle = false;
        await this.restore();
        if (this.releaseWhenIdle && !this.hasViewers()) return;
        if (!this.target.running || this.target.online === false) {
            await this.disconnect();
            return;
        }
        await this.connect();
        if (this.snapshot.connection.state === 'connected')
            await Promise.all([this.loadContacts(), this.loadRecent()]);
    }
    loadRecent(retry = false): Promise<void> {
        if (this.recentRequest) return this.recentRequest;
        if (
            this.snapshot.connection.state !== 'connected' ||
            (!retry && this.recentEpoch === this.epoch)
        )
            return Promise.resolve();
        const epoch = this.epoch;
        this.update({ recentLoading: true, recentError: '' });
        const request = this.transport
            .call(this.target.bot_id, 'get_recent_contact', { count: 100 })
            .then(dataOf)
            .then((data) => {
                if (epoch !== this.epoch) return;
                if (!Array.isArray(data)) throw new Error('此协议未返回最近会话列表');
                const merged = mergeRecentConversations(this.snapshot.account, data);
                const account = { ...merged, conversations: { ...merged.conversations } };
                for (const contact of this.snapshot.contacts)
                    if (account.conversations[contact.key])
                        account.conversations[contact.key] = {
                            ...account.conversations[contact.key],
                            ...contact,
                        };
                this.account(account);
                this.recentEpoch = epoch;
            })
            .catch((error) => {
                if (epoch === this.epoch)
                    this.update({ recentError: `最近会话同步失败：${errorText(error)}` });
            })
            .finally(() => {
                if (this.recentRequest === request) {
                    this.recentRequest = null;
                    this.update({ recentLoading: false });
                }
            });
        this.recentRequest = request;
        return request;
    }
    private account(account: Account) {
        const sources = new Map<string, boolean>();
        const mark = (message: Message): Message => {
            let changed = false;
            const segments = message.segments.map((segment) => {
                const reference = [
                    segment.data.inline_ref,
                    segment.data.local_file,
                    segment.data.file,
                ].find(isInlineImageReference);
                if (!reference) return segment;
                sources.set(reference, !!sources.get(reference) || message.status !== 'sent');
                if (message.id && segment.data.source_message_id !== message.id) {
                    changed = true;
                    return { ...segment, data: { ...segment.data, source_message_id: message.id } };
                }
                return segment;
            });
            return changed ? { ...message, segments } : message;
        };
        const sync = (rows: Message[]): Message[] => {
            let changed: Message[] | undefined;
            rows.forEach((message, index) => {
                const next = mark(message);
                if (next === message) return;
                changed ??= rows.slice();
                changed[index] = next;
            });
            return changed ?? rows;
        };
        const messages = sync(account.messages);
        const archiveMessages =
            account.archiveMessages === account.messages
                ? messages
                : account.archiveMessages && sync(account.archiveMessages);
        if (messages !== account.messages || archiveMessages !== account.archiveMessages)
            account = { ...account, messages, archiveMessages };
        for (const [reference, protectedSource] of sources)
            inlineImageService.protect(
                this.target.bot_id,
                String(this.target.qq_id),
                reference,
                protectedSource,
            );
        this.update({ account });
    }
    private externalizeSegments(segments: Segment[], created: string[] = []): Segment[] {
        return segments.map((segment) => {
            if (segment.type !== 'image' && segment.type !== 'mface') return segment;
            const data = { ...segment.data };
            const converted = new Map<string, string>();
            let changed = false;
            for (const field of ['file', 'local_file', 'base64', 'url']) {
                const source = text(data[field]);
                if (
                    source.length < 256 * 1024 ||
                    !(
                        field === 'base64' ||
                        /^(base64:\/\/|data:image\/[^;,]+;base64,)/i.test(source)
                    )
                )
                    continue;
                let reference = converted.get(source);
                if (!reference) {
                    reference = inlineImageService.capture(
                        this.target.bot_id,
                        String(this.target.qq_id),
                        source,
                        (reference) => created.push(reference),
                    );
                    converted.set(source, reference);
                }
                data[field] = reference;
                data.inline_ref = reference;
                changed = true;
            }
            return changed ? { ...segment, data } : segment;
        });
    }
    private externalizeRows(rows: readonly unknown[]): unknown[] {
        return rows.map((raw) => {
            const row = record(raw);
            if (!row.message && !row.raw_message) return raw;
            const created: string[] = [];
            try {
                const segments = this.externalizeSegments(
                    normalizeMessage(row.message ?? row.raw_message),
                    created,
                );
                return { ...row, message: segments };
            } catch (error) {
                for (const reference of created)
                    inlineImageService.discard(
                        this.target.bot_id,
                        String(this.target.qq_id),
                        reference,
                    );
                this.update({ error: errorText(error) });
                const segments = normalizeMessage(row.message ?? row.raw_message).map((segment) => {
                    if (segment.type !== 'image' && segment.type !== 'mface') return segment;
                    const data: Record<string, unknown> = {
                        ...segment.data,
                        source_message_id: id(row.message_id),
                    };
                    for (const field of ['file', 'local_file', 'url', 'base64'])
                        if (
                            /^(base64:\/\/|data:image\/)/i.test(text(data[field])) ||
                            field === 'base64'
                        )
                            delete data[field];
                    return { ...segment, data };
                });
                return { ...row, message: segments };
            }
        });
    }
    private materializeSegments(segments: Segment[]): Segment[] {
        return segments.map((segment) => {
            if (segment.type !== 'image' && segment.type !== 'mface') return segment;
            const data = { ...segment.data };
            for (const field of ['file', 'local_file', 'base64', 'url']) {
                const value = data[field];
                if (!isInlineImageReference(value)) continue;
                const source = inlineImageService.source(
                    this.target.bot_id,
                    String(this.target.qq_id),
                    value,
                );
                if (!source) throw new Error('原图片缓存已失效，请重新添加后发送');
                data[field] = field === 'base64' ? source.slice('base64://'.length) : source;
            }
            delete data.inline_ref;
            delete data.source_message_id;
            return { ...segment, data };
        });
    }
    private currentAnchor(key: SessionKey): ReadingAnchor | undefined {
        const value = this.timelineReaders.get(key)?.() ?? this.readingPositions[key];
        return value ? { session: key, ...value } : undefined;
    }
    setReading(key: SessionKey | null) {
        this.reading = key;
        if (this.transport === chatService)
            void chatDesktopService
                .setReading(this.target.bot_id, this.snapshot.account.selfId, key)
                .catch((error) => this.update({ error: errorText(error) }));
        if (key && this.snapshot.account.conversations[key]?.unread) this.markRead(key);
    }
    open(contact: Contact) {
        delete this.initialReadingPositions[contact.key];
        delete this.readingPositions[contact.key];
        const state = this.snapshot.account;
        const rows =
            state.archiveMessages?.filter((message) => message.session === contact.key) ?? [];
        const recent = rows
            .slice(-HISTORY_PAGE_SIZE)
            .map((message) => (message.gapBefore ? { ...message, gapBefore: false } : message));
        this.account(
            openConversation(
                {
                    ...state,
                    messages: mergeMessageRows(
                        [...rows.filter((message) => message.status !== 'sent'), ...recent],
                        state.messages,
                    ),
                },
                contact,
            ),
        );
        if (this.transport === chatService)
            void chatDesktopService
                .markRead(this.target.bot_id, this.snapshot.account.selfId, contact.key)
                .catch((error) => this.update({ error: errorText(error) }));
    }
    revealArchivedMessage(value: Message | string): Message | null {
        const state = this.snapshot.account;
        const messageKey = typeof value === 'string' ? value : value.key;
        const saved = mergeArchiveMessages(state.archiveMessages ?? [], state.messages);
        const message = saved.find((row) => row.key === messageKey);
        if (!message) return null;
        const rows = saved.filter((row) => row.session === message.session);
        const index = rows.findIndex((row) => row.key === message.key);
        const position: ChatReadingPosition = {
            messageKey: message.key,
            messageId: message.id ?? null,
            offset: 0,
            atBottom: false,
        };
        const anchor = { session: message.session, ...position };
        this.readingPositions[message.session] = position;
        this.initialReadingPositions[message.session] = position;
        this.pagingAnchors.set(message.session, anchor);
        const contact =
            this.snapshot.contacts.find((row) => row.key === message.session) ??
            state.conversations[message.session] ??
            contactFromKey(message.session);
        const current = state.conversations[message.session];
        this.account({
            ...state,
            active: message.session,
            conversations: {
                ...state.conversations,
                [message.session]: {
                    ...(current ?? { pinned: false, lastAt: message.at, preview: '' }),
                    ...contact,
                    unread: 0,
                },
            },
            messages: mergeArchiveMessages(
                state.messages,
                rows.slice(Math.max(0, index - HISTORY_PAGE_SIZE), index + HISTORY_PAGE_SIZE + 1),
            ),
        });
        this.pagingAnchors.delete(message.session);
        this.readingPositions[message.session] = position;
        const oldest = this.snapshot.account.messages.find(
            (row) => row.session === message.session && row.id,
        );
        if (oldest?.id) this.historyCursor.set(message.session, oldest.id);
        else this.historyCursor.delete(message.session);
        if (this.transport === chatService)
            void chatDesktopService
                .markRead(this.target.bot_id, state.selfId, message.session)
                .catch((error) => this.update({ error: errorText(error) }));
        return message;
    }
    close(key: SessionKey) {
        if (this.snapshot.account.active === key) {
            this.reading = null;
            this.account({ ...this.snapshot.account, active: null });
        }
    }
    draft(key: SessionKey, draft: Draft) {
        const attachments = draft.attachments.map((attachment) => {
            if (attachment.type === 'face' || !isInlineImageReference(attachment.path))
                return attachment;
            const path = inlineImageService.source(
                this.target.bot_id,
                String(this.target.qq_id),
                attachment.path,
            );
            if (!path) throw new Error('原图片内容已释放，请重新添加');
            return { ...attachment, path };
        });
        this.account(setDraft(this.snapshot.account, key, { ...draft, attachments }));
    }
    pin(key: SessionKey) {
        const state = this.snapshot.account;
        const c = state.conversations[key];
        if (c)
            this.account({
                ...state,
                conversations: { ...state.conversations, [key]: { ...c, pinned: !c.pinned } },
            });
    }
    markRead(key: SessionKey) {
        const state = this.snapshot.account;
        const conversation = state.conversations[key];
        if (!conversation?.unread) return;
        this.account({
            ...state,
            conversations: { ...state.conversations, [key]: { ...conversation, unread: 0 } },
        });
        if (this.transport === chatService)
            void chatDesktopService
                .markRead(this.target.bot_id, this.snapshot.account.selfId, key)
                .catch((error) => this.update({ error: errorText(error) }));
    }
    box(key: SessionKey) {
        const state = this.snapshot.account;
        const c = state.conversations[key];
        if (c?.type === 'group')
            this.account({
                ...state,
                conversations: { ...state.conversations, [key]: { ...c, boxed: !c.boxed } },
            });
    }
    private interruptPending() {
        this.contactsRequest = null;
        this.recentRequest = null;
        const account = this.snapshot.account;
        this.update({
            contactsLoading: false,
            recentLoading: false,
            history: Object.fromEntries(
                Object.entries(this.snapshot.history).map(([key, value]) => [
                    key,
                    { ...value, loading: false },
                ]),
            ),
            account: {
                ...account,
                messages: account.messages.map((message) =>
                    message.status === 'sending'
                        ? { ...message, status: 'unknown', error: '连接已断开，发送结果待确认' }
                        : message,
                ),
            },
        });
    }
    async disconnect() {
        this.setReading(null);
        this.epoch++;
        this.connecting = false;
        this.reading = null;
        const subscription = this.subscription;
        this.subscription = null;
        this.interruptPending();
        this.update({ connection: { state: 'stopped', reason: '已断开' } });
        if (subscription) {
            try {
                await this.transport.unsubscribe(subscription);
            } catch (error) {
                this.update({ error: errorText(error) });
            }
        }
    }
    async connect() {
        if (this.connecting || !this.target.running || this.target.online === false) return;
        if (this.subscription && this.snapshot.connection.state !== 'stopped') return;
        const old = this.subscription;
        this.subscription = null;
        const epoch = ++this.epoch;
        this.connecting = true;
        this.interruptPending();
        this.update({ connection: { state: 'connecting' }, error: '' });
        try {
            if (old) await this.transport.unsubscribe(old);
            if (epoch !== this.epoch) return;
            const result = await this.transport.subscribe(this.target.bot_id, (batch) => {
                if (epoch !== this.epoch || batch.bot_id !== this.target.bot_id || batch.v !== 1)
                    return;
                let account = this.snapshot.account;
                let connection = this.snapshot.connection;
                const payloads: unknown[] = [];
                for (const event of batch.events) {
                    if (event.seq <= account.lastSeq) continue;
                    const body = event.body;
                    if (body.kind === 'ob11') {
                        const self = id(body.payload.self_id);
                        if (self && self !== account.selfId) continue;
                        payloads.push(body.payload);
                    } else if (body.kind === 'receiver') connection = body.state;
                    else if (body.kind === 'gap' || body.kind === 'dropped')
                        account = { ...account, gap: true };
                    account = { ...account, lastSeq: event.seq };
                }
                account = ingestMessages(
                    account,
                    this.externalizeRows(payloads),
                    false,
                    typeof document !== 'undefined' &&
                        document.visibilityState === 'visible' &&
                        document.hasFocus()
                        ? this.reading
                        : null,
                    account.active ? this.currentAnchor(account.active) : undefined,
                );
                const becameConnected =
                    connection.state === 'connected' &&
                    this.snapshot.connection.state !== 'connected';
                this.update({ account, connection });
                if (becameConnected) {
                    void this.loadContacts();
                    void this.loadRecent();
                }
            });
            if (epoch !== this.epoch) {
                await this.transport.unsubscribe(result.subscription_id);
                return;
            }
            this.subscription = result.subscription_id;
            this.update({ connection: result.receiver.state });
        } catch (error) {
            if (epoch === this.epoch)
                this.update({
                    error: errorText(error),
                    connection: { state: 'stopped', reason: errorText(error) },
                });
        } finally {
            if (epoch === this.epoch) this.connecting = false;
        }
    }
    loadContacts(): Promise<void> {
        if (this.contactsRequest) return this.contactsRequest;
        const epoch = this.epoch;
        this.update({ contactsLoading: true, error: '' });
        const request = (async () => {
            const friends = async (): Promise<Contact[]> => {
                try {
                    const grouped = parseFriendCategories(
                        dataOf(
                            await this.transport.call(
                                this.target.bot_id,
                                'get_friends_with_category',
                                {},
                            ),
                        ),
                    );
                    if (grouped) return grouped;
                } catch {
                    // 旧协议端可能没有分组接口；普通好友列表仍可用。
                }
                if (epoch !== this.epoch) return [];
                const data = dataOf(
                    await this.transport.call(this.target.bot_id, 'get_friend_list', {}),
                );
                if (!Array.isArray(data)) throw new Error('好友列表返回格式无法识别');
                return data.flatMap((row) => {
                    const contact = parseContact(row, 'private');
                    return contact ? [contact] : [];
                });
            };
            const results = await Promise.allSettled([
                this.transport
                    .call(this.target.bot_id, 'get_group_list', {})
                    .then(dataOf)
                    .then((data) => {
                        if (!Array.isArray(data)) throw new Error('群聊列表返回格式无法识别');
                        return data.flatMap((row) => {
                            const contact = parseContact(row, 'group');
                            return contact ? [contact] : [];
                        });
                    }),
                friends(),
            ]);
            if (epoch !== this.epoch) return;
            const contacts: Contact[] = [];
            const errors: string[] = [];
            results.forEach((result, index) => {
                const type = index === 0 ? 'group' : 'private';
                if (result.status === 'rejected') {
                    errors.push(errorText(result.reason));
                    contacts.push(...this.snapshot.contacts.filter((c) => c.type === type));
                } else contacts.push(...result.value);
            });
            const account = this.snapshot.account;
            const conversations = { ...account.conversations };
            for (const contact of contacts)
                if (conversations[contact.key])
                    conversations[contact.key] = { ...conversations[contact.key], ...contact };
            this.update({
                contacts,
                account: { ...account, conversations },
                error: errors.join('；'),
            });
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
        const value = dataOf(
            await this.transport.call(this.target.bot_id, 'get_group_member_list', {
                group_id: this.peer(key),
            }),
        );
        return Array.isArray(value)
            ? value
                  .map(record)
                  .map((row) => ({
                      id: id(row.user_id),
                      name: text(row.card) || text(row.nickname) || id(row.user_id),
                  }))
                  .filter((row) => row.id)
            : [];
    }
    private peer(key: SessionKey): string | number {
        const value = key.slice(key.indexOf(':') + 1);
        if (this.target.backend === 'snowluma') {
            const numeric = Number(value);
            if (!Number.isSafeInteger(numeric)) throw new Error('账号标识超出可支持范围');
            return numeric;
        }
        return value;
    }
    ensureHistory(key: SessionKey): Promise<void> | undefined {
        const history = this.snapshot.history[key];
        if (history?.loaded || history?.loading || history?.error) return;
        const saved = this.initialReadingPositions[key];
        return this.history(
            key,
            saved && !saved.atBottom ? (saved.messageId ?? undefined) : undefined,
        );
    }
    async latest(key: SessionKey, signal?: AbortSignal): Promise<void> {
        const epoch = this.epoch;
        if (this.snapshot.history[key]?.loading && !signal?.aborted) {
            await new Promise<void>((resolve) => {
                const finish = () => {
                    unsubscribe();
                    signal?.removeEventListener('abort', finish);
                    resolve();
                };
                const unsubscribe = this.subscribe(() => {
                    if (!this.snapshot.history[key]?.loading || epoch !== this.epoch) finish();
                });
                signal?.addEventListener('abort', finish, { once: true });
            });
        }
        if (signal?.aborted || epoch !== this.epoch) return;
        await this.history(key, undefined, { signal, latest: true });
        if (signal?.aborted || epoch !== this.epoch || this.snapshot.history[key]?.error) return;
        delete this.initialReadingPositions[key];
        const message = this.snapshot.account.messages.filter((row) => row.session === key).at(-1);
        if (message)
            this.readingPositions[key] = {
                messageKey: message.key,
                messageId: message.id ?? null,
                offset: 0,
                atBottom: true,
            };
    }
    async history(
        key: SessionKey,
        before?: string,
        navigation?: { signal?: AbortSignal; latest: boolean },
    ): Promise<void> {
        if (
            navigation?.signal?.aborted ||
            this.snapshot.connection.state !== 'connected' ||
            this.snapshot.history[key]?.loading ||
            (!navigation?.latest && !before && this.snapshot.history[key]?.done)
        )
            return;
        const epoch = this.epoch;
        const anchor = this.pagingAnchors.get(key) ?? this.currentAnchor(key);
        const wasDone = this.snapshot.history[key]?.done ?? false;
        const set = (
            loading: boolean,
            done = false,
            error = '',
            loaded = this.snapshot.history[key]?.loaded ?? false,
        ) =>
            this.update({
                history: { ...this.snapshot.history, [key]: { loading, loaded, done, error } },
            });
        set(true);
        try {
            const group = key.startsWith('group:');
            const cursor = navigation?.latest ? undefined : (before ?? this.historyCursor.get(key));
            const params: Record<string, unknown> = {
                [group ? 'group_id' : 'user_id']: this.peer(key),
                count: HISTORY_PAGE_SIZE,
                reverse_order: this.target.backend === 'snowluma',
                disable_get_url: false,
                parse_mult_msg: true,
                quick_reply: false,
                reverseOrder: false,
            };
            if (cursor) {
                if (this.target.backend === 'snowluma' && !Number.isSafeInteger(Number(cursor)))
                    throw new Error('此协议的历史游标超出可支持范围');
                params[this.target.backend === 'snowluma' ? 'message_id' : 'message_seq'] =
                    this.target.backend === 'snowluma' ? Number(cursor) : cursor;
            }
            const data = record(
                dataOf(
                    await this.transport.call(
                        this.target.bot_id,
                        group ? 'get_group_msg_history' : 'get_friend_msg_history',
                        params,
                    ),
                ),
            );
            if (epoch !== this.epoch) return;
            if (navigation?.signal?.aborted) {
                set(false, wasDone);
                return;
            }
            if (!Array.isArray(data.messages)) throw new Error('此通道未返回可识别的消息历史');
            const rows = data.messages
                .map(record)
                .sort((a, b) => Number(a.time || 0) - Number(b.time || 0));
            const previous = this.snapshot.account.messages.filter(
                (message) => message.session === key,
            );
            const boundary = before && previous.findIndex((message) => message.id === before);
            const left =
                typeof boundary === 'number' && boundary > 0 ? previous[boundary - 1] : undefined;
            const incoming = rows.map((row) => ({
                ...row,
                message_type: group ? 'group' : 'private',
                ...(group ? { group_id: this.peer(key) } : { target_id: this.peer(key) }),
            }));
            const next = id(rows[0]?.message_id);
            const initial = this.initialReadingPositions[key];
            const current = initial ? { session: key, ...initial } : this.currentAnchor(key);
            const readingChanged =
                current &&
                (current.messageKey !== anchor?.messageKey ||
                    current.atBottom !== anchor?.atBottom);
            const anchorIndex =
                anchor &&
                previous.findIndex(
                    (message) =>
                        message.key === anchor.messageKey || message.id === anchor.messageId,
                );
            const pagingAnchor = navigation?.latest
                ? { session: key, messageKey: '', atBottom: true }
                : readingChanged && current
                  ? current
                  : before || !anchor || anchor.atBottom || (anchorIndex ?? -1) > HISTORY_PAGE_SIZE
                    ? {
                          session: key,
                          messageKey: `${key}/${before || next}`,
                          messageId: before || next,
                          atBottom: false,
                      }
                    : anchor;
            this.pagingAnchors.set(key, pagingAnchor);
            const state = this.snapshot.account;
            const recent = navigation?.latest
                ? (state.archiveMessages ?? state.messages)
                      .filter((message) => message.session === key)
                      .slice(-HISTORY_PAGE_SIZE)
                      .map((message) =>
                          message.gapBefore ? { ...message, gapBefore: false } : message,
                      )
                : [];
            const recentKeys = new Set(recent.map((message) => message.key));
            const recentIds = new Set(
                recent.flatMap((message) => (message.id ? [message.id] : [])),
            );
            const source = navigation?.latest
                ? {
                      ...state,
                      messages: mergeMessageRows(
                          recent,
                          state.messages.filter(
                              (message) =>
                                  message.session !== key ||
                                  message.status !== 'sent' ||
                                  recentKeys.has(message.key) ||
                                  (!!message.id && recentIds.has(message.id)),
                          ),
                      ),
                  }
                : state;
            let account = ingestMessages(
                source,
                this.externalizeRows(incoming),
                true,
                null,
                pagingAnchor,
                key,
            );
            if (before && left && previous.find((message) => message.id === before)?.gapBefore) {
                const filled = rows.some((row) => id(row.message_id) === left.id);
                account = {
                    ...account,
                    messages: account.messages.map((message) =>
                        message.session !== key
                            ? message
                            : message.id === before
                              ? { ...message, gapBefore: false }
                              : message.id === next
                                ? { ...message, gapBefore: !filled }
                                : message,
                    ),
                };
            }
            // NapCat 的参数虽名为 message_seq，实际按短 message_id 查内部 MsgId。
            this.account(account);
            this.pagingAnchors.delete(key);
            const oldest = this.snapshot.account.messages.find(
                (message) => message.session === key && message.id,
            );
            if (oldest?.id) this.historyCursor.set(key, oldest.id);
            const done = before
                ? wasDone && oldest?.id === previous[0]?.id
                : !next || next === cursor || rows.length < HISTORY_PAGE_SIZE;
            set(false, done, '', true);
        } catch (error) {
            if (epoch === this.epoch)
                set(
                    false,
                    navigation?.signal?.aborted ? wasDone : false,
                    navigation?.signal?.aborted ? '' : errorText(error),
                );
        } finally {
            this.pagingAnchors.delete(key);
        }
    }
    async send(key: SessionKey) {
        if (
            this.sends.get(key) === this.epoch ||
            !this.target.running ||
            this.target.online === false ||
            this.snapshot.connection.state !== 'connected'
        )
            return;
        const draft = this.snapshot.account.drafts[key] ?? EMPTY_DRAFT;
        if (!draft.text.trim() && !draft.attachments.length) return;
        const group = key.startsWith('group:');
        const peer = { [group ? 'group_id' : 'user_id']: this.peer(key) };
        const segments = buildMessageSegments(draft.text, draft.mentions ?? []);
        if (draft.reply) segments.unshift({ type: 'reply', data: { id: draft.reply.id } });
        for (const attachment of draft.attachments) {
            if (attachment.type === 'face')
                segments.push({ type: 'face', data: { id: attachment.id } });
            if (attachment.type === 'image')
                segments.push({
                    type: 'image',
                    data: {
                        file:
                            isInlineImageReference(attachment.path) ||
                            /^(base64:\/\/|https?:\/\/)/i.test(attachment.path)
                                ? attachment.path
                                : localFileTokenFor(attachment.path),
                        name: attachment.name,
                        sub_type: attachment.subType ?? 0,
                    },
                });
        }
        const operations: SendOperation[] = [];
        if (segments.some((s) => s.type !== 'reply'))
            operations.push({
                action: group ? 'send_group_msg' : 'send_private_msg',
                params: { ...peer, message: segments },
                segments,
            });
        for (const file of draft.attachments)
            if (file.type === 'file')
                operations.push({
                    action: group ? 'upload_group_file' : 'upload_private_file',
                    params: {
                        ...peer,
                        file: localFileTokenFor(file.path),
                        name: file.name,
                        upload_file: true,
                    },
                    segments: [
                        {
                            type: 'file',
                            data: { name: file.name, file: localFileTokenFor(file.path) },
                        },
                    ],
                });
        await this.runSends(key, operations);
    }
    async poke(key: SessionKey, userId: string) {
        if (
            !this.target.running ||
            this.target.online === false ||
            this.snapshot.connection.state !== 'connected'
        )
            return;
        const group = key.startsWith('group:');
        dataOf(
            await this.transport.call(
                this.target.bot_id,
                group ? 'group_poke' : 'friend_poke',
                group ? { group_id: this.peer(key), user_id: userId } : { user_id: userId },
            ),
        );
        // 成功后乐观上墙；后端回显的 poke 通知在 ingestMessage 里按时间窗去重
        const echo: Record<string, unknown> = {
            notice_type: 'notify',
            sub_type: 'poke',
            user_id: this.snapshot.account.selfId,
            target_id: userId,
            time: Math.floor(Date.now() / 1000),
        };
        if (group) echo.group_id = this.peer(key);
        this.account(
            ingestMessages(this.snapshot.account, [echo], false, null, this.currentAnchor(key)),
        );
    }
    async retry(messageKey: string) {
        const message =
            this.snapshot.account.messages.find((m) => m.key === messageKey) ??
            this.snapshot.account.archiveMessages?.find((m) => m.key === messageKey);
        if (!message?.mine || message.status !== 'failed' || message.recalled) return;
        if (message.segments.some((segment) => segment.type === 'forward')) return;
        const key = message.session;
        if (
            !this.target.running ||
            this.target.online === false ||
            this.snapshot.connection.state !== 'connected'
        )
            throw new Error('聊天未连接，请连接后重发');
        if (this.sends.get(key) === this.epoch) return;
        const group = key.startsWith('group:');
        const peer = { [group ? 'group_id' : 'user_id']: this.peer(key) };
        const segments = message.segments.map((segment) => {
            if (segment.type === 'image' || segment.type === 'mface') {
                if (
                    !text(segment.data.local_file) &&
                    !text(segment.data.file) &&
                    !text(segment.data.url) &&
                    !text(segment.data.file_id) &&
                    !id(segment.data.emoji_id)
                )
                    throw new Error('原附件已不可用，请重新添加后发送');
                return { ...segment, data: { ...segment.data } };
            }
            if (segment.type !== 'file') return segment;
            const file =
                text(segment.data.local_file) || text(segment.data.file) || text(segment.data.url);
            if (!/^(ncd-local-file:\/\/|base64:\/\/|https?:\/\/).+/i.test(file))
                throw new Error('原附件已不可用，请重新添加后发送');
            // local_file 只服务本地预览与选源，不下发给协议。
            const data: Record<string, unknown> = { ...segment.data, file };
            delete data.local_file;
            return { ...segment, data };
        });
        if (!segments.length || segments.every((s) => s.type === 'reply'))
            throw new Error('原消息内容已不可用');
        const file =
            segments.length === 1 && segments[0].type === 'file' ? segments[0].data : undefined;
        const operation: SendOperation = file
            ? {
                  action: group ? 'upload_group_file' : 'upload_private_file',
                  params: { ...peer, file: file.file, name: file.name, upload_file: true },
                  segments,
              }
            : {
                  action: group ? 'send_group_msg' : 'send_private_msg',
                  params: { ...peer, message: segments },
                  segments,
                  resolveImages: segments.some(
                      (segment) => segment.type === 'image' || segment.type === 'mface',
                  ),
                  sourceMessageId: message.id,
              };
        if (!this.snapshot.account.messages.some((row) => row.key === message.key))
            this.account({
                ...this.snapshot.account,
                messages: [...this.snapshot.account.messages, { ...message, status: 'sending' }],
            });
        await this.runSends(key, [operation], message.key);
    }
    async recall(messageKey: string): Promise<void> {
        const message = this.findMessage(messageKey);
        if (
            !message ||
            !canRecallMessage(message) ||
            message.senderId !== this.snapshot.account.selfId
        )
            return;
        this.requireConnected();
        const epoch = this.epoch;
        if (this.recalls.get(messageKey) === epoch) return;
        const scope = accountKey(this.target.bot_id, String(this.target.qq_id));
        this.recalls.set(messageKey, epoch);
        try {
            const response = await this.transport.call(this.target.bot_id, 'delete_msg', {
                message_id: this.protocolMessageId(message.id!),
            });
            if (!this.actionIsCurrent(epoch, scope)) return;
            dataOf(response);
            const state = this.snapshot.account;
            const conversation = state.conversations[message.session];
            const latest = (state.archiveMessages ?? state.messages)
                .filter((row) => row.session === message.session)
                .at(-1);
            const mark = (row: Message) =>
                row.session === message.session && row.id === message.id
                    ? { ...row, recalled: true }
                    : row;
            this.account({
                ...state,
                messages: state.messages.map(mark),
                archiveMessages: state.archiveMessages?.map(mark),
                conversations:
                    conversation && latest?.id === message.id && message.at >= conversation.lastAt
                        ? {
                              ...state.conversations,
                              [message.session]: { ...conversation, preview: '消息已撤回' },
                          }
                        : state.conversations,
            });
        } catch (error) {
            if (this.actionIsCurrent(epoch, scope)) throw error;
        } finally {
            if (this.recalls.get(messageKey) === epoch) this.recalls.delete(messageKey);
            if (this.releaseWhenIdle && !this.isSending() && !this.hasViewers())
                void releaseChatAccount(this).catch(() => {});
        }
    }
    async repeat(messageKey: string): Promise<void> {
        const message = this.findMessage(messageKey);
        if (!message || !canRepeatMessage(message)) return;
        this.requireConnected();
        if (this.sends.get(message.session) === this.epoch) return;
        const segments = message.segments.map((segment) => {
            if (segment.type === 'markdown' && segment.data.content === undefined) {
                const content = markdownContent(segment.data);
                if (!content) throw new Error('原 Markdown 内容已不可用');
                return { ...segment, data: { ...segment.data, content } };
            }
            return { ...segment, data: { ...segment.data } };
        });
        const group = message.session.startsWith('group:');
        await this.runSends(
            message.session,
            [
                {
                    action: group ? 'send_group_msg' : 'send_private_msg',
                    params: {
                        [group ? 'group_id' : 'user_id']: this.peer(message.session),
                        message: segments,
                    },
                    segments,
                    resolveImages: true,
                    sourceMessageId: message.id,
                },
            ],
            undefined,
            true,
        );
    }
    async forward(messageKeys: readonly string[], contact: Contact): Promise<void> {
        this.requireConnected();
        const keys = [...new Set(messageKeys)];
        if (!keys.length || keys.length > FORWARD_MESSAGE_LIMIT)
            throw new Error(`一次可转发 1–${FORWARD_MESSAGE_LIMIT} 条消息`);
        const messages = keys.map((key) => this.findMessage(key));
        if (messages.some((message) => !message || !canForwardMessage(message)))
            throw new Error('所选消息已不可转发，请重新选择');
        const source = (messages as Message[]).sort((a, b) => a.at - b.at);
        if (source.some((message) => message.session !== source[0].session))
            throw new Error('请选择同一会话中的消息');
        const known =
            this.snapshot.contacts.find((row) => row.key === contact.key) ??
            this.snapshot.account.conversations[contact.key];
        if (!known || known.id !== contact.id || known.type !== contact.type)
            throw new Error('转发目标不属于当前账号');
        const epoch = this.epoch;
        if (this.sends.get(contact.key) === epoch) throw new Error('该会话仍在发送，请稍后转发');
        const scope = accountKey(this.target.bot_id, String(this.target.qq_id));
        const requestId = crypto.randomUUID();
        const peer = this.peer(contact.key);
        const content = source.map((message) => ({
            type: 'node',
            data: {
                user_id: message.senderId,
                nickname: message.senderName,
                time: Math.floor(message.at / 1000),
                content: message.segments,
            },
        }));
        const nodes = source.map((message) => ({
            type: 'node',
            data: { id: this.protocolMessageId(message.id!) },
        }));
        this.sends.set(contact.key, epoch);
        const state = this.snapshot.account;
        this.account(
            addPending(
                {
                    ...state,
                    conversations: {
                        ...state.conversations,
                        [contact.key]: {
                            ...(state.conversations[contact.key] ?? {
                                unread: 0,
                                pinned: false,
                                lastAt: 0,
                                preview: '',
                            }),
                            ...known,
                        },
                    },
                },
                contact.key,
                requestId,
                [{ type: 'forward', data: { content } }],
                Date.now(),
            ),
        );
        try {
            const group = contact.type === 'group';
            let response: DebugCallResponse;
            try {
                response = await this.transport.call(
                    this.target.bot_id,
                    group ? 'send_group_forward_msg' : 'send_private_forward_msg',
                    { [group ? 'group_id' : 'user_id']: peer, messages: nodes },
                    requestId,
                );
            } catch (error) {
                throw new ChatForwardError(
                    `转发结果待确认：${errorText(error)}。请先查看目标会话`,
                    true,
                );
            }
            if (!this.actionIsCurrent(epoch, scope))
                throw new ChatForwardError('连接已改变，转发结果待确认，请先查看目标会话', true);
            if (response.result.kind === 'err') {
                const error = response.result.error;
                const uncertain = ['timeout', 'cancelled', 'transport', 'internal'].includes(
                    error.kind,
                );
                const copy = debugErrorCopy(error);
                throw new ChatForwardError(
                    uncertain
                        ? `转发结果待确认，请先查看目标会话：${copy.title}`
                        : [copy.title, copy.detail].filter(Boolean).join('：'),
                    uncertain,
                );
            }
            const result = response.result.outcome;
            if (!result.ok)
                throw new ChatForwardError(result.wording || result.message || '转发失败');
            this.account(
                settleSend(this.snapshot.account, requestId, {
                    state: 'sent',
                    id: id(record(result.data).message_id) || undefined,
                }),
            );
        } catch (error) {
            if (this.actionIsCurrent(epoch, scope))
                this.account(
                    settleSend(this.snapshot.account, requestId, {
                        state:
                            error instanceof ChatForwardError && error.uncertain
                                ? 'unknown'
                                : 'failed',
                        error: errorText(error),
                    }),
                );
            throw error;
        } finally {
            if (this.sends.get(contact.key) === epoch) this.sends.delete(contact.key);
            if (this.releaseWhenIdle && !this.isSending() && !this.hasViewers())
                void releaseChatAccount(this).catch(() => {});
        }
    }
    private findMessage(key: string): Message | undefined {
        return (
            this.snapshot.account.messages.find((message) => message.key === key) ??
            this.snapshot.account.archiveMessages?.find((message) => message.key === key)
        );
    }
    private requireConnected() {
        if (
            !this.target.running ||
            this.target.online === false ||
            this.snapshot.connection.state !== 'connected'
        )
            throw new Error('聊天未连接，请连接后操作');
    }
    private protocolMessageId(value: string): string | number {
        if (!/^-?\d+$/.test(value) || /^-?0+$/.test(value))
            throw new Error('此消息缺少有效协议标识');
        if (this.target.backend !== 'snowluma') return value;
        const numeric = Number(value);
        if (!Number.isSafeInteger(numeric)) throw new Error('此协议的消息标识超出可支持范围');
        return numeric;
    }
    private actionIsCurrent(epoch: number, scope: string): boolean {
        return (
            epoch === this.epoch &&
            scope === accountKey(this.target.bot_id, String(this.target.qq_id))
        );
    }
    private async runSends(
        key: SessionKey,
        operations: SendOperation[],
        retryKey?: string,
        preserveDraft = false,
    ) {
        const epoch = this.epoch;
        const scope = accountKey(this.target.bot_id, String(this.target.qq_id));
        const created: string[] = [];
        let prepared: SendOperation[];
        try {
            prepared = operations.map((operation) => ({
                ...operation,
                segments: this.externalizeSegments(operation.segments, created),
            }));
        } catch (error) {
            for (const reference of created)
                inlineImageService.discard(
                    this.target.bot_id,
                    String(this.target.qq_id),
                    reference,
                );
            throw error;
        }
        this.sends.set(key, epoch);
        if (!retryKey && !preserveDraft) this.draft(key, EMPTY_DRAFT);
        try {
            for (const operation of prepared) {
                const requestId = crypto.randomUUID();
                this.account(
                    retryKey
                        ? {
                              ...this.snapshot.account,
                              messages: this.snapshot.account.messages.map((message) =>
                                  message.key === retryKey
                                      ? {
                                            ...message,
                                            requestId,
                                            id: undefined,
                                            fileId: undefined,
                                            segments: operation.segments,
                                            status: 'sending' as const,
                                            error: undefined,
                                        }
                                      : message,
                              ),
                          }
                        : addPending(
                              this.snapshot.account,
                              key,
                              requestId,
                              operation.segments,
                              Date.now(),
                          ),
                );
                if (!this.actionIsCurrent(epoch, scope)) {
                    this.account(
                        settleSend(this.snapshot.account, requestId, {
                            state: 'failed',
                            error: '发送已中止，此条尚未提交',
                        }),
                    );
                    continue;
                }
                if (
                    this.target.backend === 'snowluma' &&
                    operation.segments.some((segment) => segment.type === 'face')
                ) {
                    try {
                        await qqFaceService.validate(
                            this.target,
                            operation.segments
                                .filter((segment) => segment.type === 'face')
                                .map((segment) => text(segment.data.id)),
                            (...args) => this.transport.call(...args),
                        );
                    } catch (error) {
                        if (this.actionIsCurrent(epoch, scope)) {
                            const failed = settleSend(this.snapshot.account, requestId, {
                                state: 'failed',
                                error: errorText(error),
                            });
                            const message = failed.messages.find(
                                (row) => row.requestId === requestId,
                            );
                            this.account(
                                !retryKey && !preserveDraft && message
                                    ? setDraft(
                                          failed,
                                          key,
                                          recoverDraft(message, failed.drafts[key] ?? EMPTY_DRAFT),
                                      )
                                    : failed,
                            );
                        }
                        continue;
                    }
                    if (!this.actionIsCurrent(epoch, scope)) continue;
                }
                let submitted = false;
                try {
                    let params = Array.isArray(record(operation.params).message)
                        ? { ...record(operation.params), message: operation.segments }
                        : operation.params;
                    if (operation.resolveImages) {
                        const media = createChatMediaService((...args) =>
                            this.transport.call(...args),
                        );
                        const target = { ...this.target };
                        const segments: Segment[] = [];
                        let imageIndex = 0;
                        for (const segment of operation.segments) {
                            if (!this.actionIsCurrent(epoch, scope)) break;
                            segments.push(
                                segment.type === 'image' || segment.type === 'mface'
                                    ? await media.imageForSend(target, segment.data, {
                                          messageId: operation.sourceMessageId,
                                          imageIndex: imageIndex++,
                                      })
                                    : segment,
                            );
                        }
                        if (!this.actionIsCurrent(epoch, scope)) continue;
                        params = { ...record(params), message: segments };
                    }
                    if (
                        (operation.resolveImages || retryKey) &&
                        this.target.backend === 'snowluma'
                    ) {
                        const segments = record(params).message;
                        if (Array.isArray(segments))
                            params = {
                                ...record(params),
                                message: segments.map((segment: Segment) => {
                                    const large =
                                        segment.type === 'face'
                                            ? qqFaceLarge(segment.data)
                                            : undefined;
                                    return large === undefined
                                        ? segment
                                        : { ...segment, data: { ...segment.data, large } };
                                }),
                            };
                    }
                    if (Array.isArray(record(params).message))
                        params = {
                            ...record(params),
                            message: this.materializeSegments(record(params).message as Segment[]),
                        };
                    submitted = true;
                    const response = await this.transport.call(
                        this.target.bot_id,
                        operation.action,
                        params,
                        requestId,
                    );
                    if (!this.actionIsCurrent(epoch, scope)) continue;
                    if (response.result.kind === 'err') {
                        const error = response.result.error;
                        const unknown = ['timeout', 'cancelled', 'transport', 'internal'].includes(
                            error.kind,
                        );
                        const copy = debugErrorCopy(error);
                        this.account(
                            settleSend(this.snapshot.account, requestId, {
                                state: unknown ? 'unknown' : 'failed',
                                error: [copy.title, copy.detail].filter(Boolean).join('：'),
                            }),
                        );
                    } else {
                        const result = response.result.outcome;
                        if (
                            !result.ok &&
                            /system face.*(?:absent|incomplete)/i.test(
                                result.wording || result.message || '',
                            )
                        )
                            qqFaceService.invalidate(this.target);
                        this.account(
                            settleSend(this.snapshot.account, requestId, {
                                state: result.ok ? 'sent' : 'failed',
                                id: id(record(result.data).message_id) || undefined,
                                fileId: operation.action.startsWith('upload_')
                                    ? id(record(result.data).file_id) || undefined
                                    : undefined,
                                error: result.ok
                                    ? undefined
                                    : `${result.wording || result.message || '发送失败'}（错误码 ${result.retcode}）`,
                            }),
                        );
                    }
                } catch (error) {
                    if (this.actionIsCurrent(epoch, scope))
                        this.account(
                            settleSend(this.snapshot.account, requestId, {
                                state: submitted ? 'unknown' : 'failed',
                                error: errorText(error),
                            }),
                        );
                }
            }
        } finally {
            if (this.sends.get(key) === epoch) this.sends.delete(key);
            if (this.releaseWhenIdle && !this.isSending() && !this.hasViewers())
                void releaseChatAccount(this).catch(() => {});
        }
    }
    isSending() {
        return this.sends.size > 0 || this.recalls.size > 0;
    }
    hasViewers() {
        return this.listeners.size > 0;
    }
    async flushForRelease() {
        if (this.restoreRequest) await this.restoreRequest;
        await this.flushArchive();
        if (this.snapshot.archiveError) throw new Error(this.snapshot.archiveError);
    }
    scroll(key: string, value?: number) {
        if (value !== undefined) this.positions[key] = Math.max(0, value);
        return this.positions[key];
    }
    readingPosition(key: string, value?: ChatReadingPosition): ChatReadingPosition | undefined {
        if (value) this.readingPositions[key] = value;
        return this.readingPositions[key];
    }
    initialReadingPosition(key: string): ChatReadingPosition | undefined {
        return this.initialReadingPositions[key];
    }
    finishInitialReading(key: string, position: ChatReadingPosition) {
        if (this.initialReadingPositions[key] === position)
            delete this.initialReadingPositions[key];
    }
    captureTimeline(key: string, read: () => ChatReadingPosition | undefined) {
        this.timelineReaders.set(key, read);
        return () => {
            if (this.timelineReaders.get(key) !== read) return;
            const value = read();
            if (value) this.readingPosition(key, value);
            this.timelineReaders.delete(key);
        };
    }
    view(): ChatAccountView {
        for (const [key, read] of this.timelineReaders) {
            const value = read();
            if (value) this.readingPosition(key, value);
        }
        const account = this.snapshot.account;
        return {
            botId: this.target.bot_id,
            selfId: account.selfId,
            active: account.active,
            scroll: { ...this.positions },
            reading: { ...this.readingPositions },
            drafts: Object.fromEntries(
                Object.entries(account.drafts).map(([key, draft]) => [
                    key,
                    {
                        ...draft,
                        mentions: draft.mentions ?? [],
                        attachments: draft.attachments.map((a) =>
                            a.type === 'face'
                                ? a
                                : a.type === 'image'
                                  ? {
                                        ...a,
                                        type: 'image' as const,
                                        subType: a.subType ?? null,
                                        previewPath: a.previewPath ?? null,
                                    }
                                  : { ...a, type: 'file' as const },
                        ),
                    },
                ]),
            ),
        };
    }
    restoreView(view: ChatAccountView) {
        if (view.botId !== this.target.bot_id || view.selfId !== this.snapshot.account.selfId)
            return;
        const drafts = Object.fromEntries(
            Object.entries(view.drafts).map(([key, draft]) => [
                key,
                {
                    ...draft,
                    attachments: draft.attachments.map((a) =>
                        a.type === 'image'
                            ? {
                                  ...a,
                                  subType: a.subType === 1 ? (1 as const) : undefined,
                                  previewPath: a.previewPath ?? undefined,
                              }
                            : a,
                    ),
                },
            ]),
        );
        this.positions = { ...view.scroll };
        this.readingPositions = { ...view.reading };
        const initial = view.active && view.reading?.[view.active];
        this.initialReadingPositions = initial && view.active ? { [view.active]: initial } : {};
        this.account({
            ...this.snapshot.account,
            active: view.active as SessionKey | null,
            drafts,
        });
    }
}

const accounts = new Map<string, ChatAccountStore>();
const views = new Map<string, ChatAccountView>();
let selectedChatBot: string | null = null;
let viewRevision = 0;
let handoffInProgress = false;
let viewSaveQueue: Promise<void> = Promise.resolve();
let viewLoadRequest: Promise<ChatViewState> | null = null;
export const selectChatBot = (bot: string) => {
    selectedChatBot = bot;
};
export const getChatSelectedBot = () => selectedChatBot;
export function chatAccount(target: DebugTarget): ChatAccountStore {
    const key = accountKey(target.bot_id, String(target.qq_id));
    let store = accounts.get(key);
    if (!store) {
        store = new ChatAccountStore(target);
        store.viewRevision = viewRevision;
        const view = views.get(key);
        if (view) store.restoreView(view);
        accounts.set(key, store);
    }
    store.target = target;
    return store;
}
export function reconcileChatAccounts(targets: DebugTarget[]) {
    const live = new Set(targets.map((t) => accountKey(t.bot_id, String(t.qq_id))));
    for (const key of views.keys()) if (!live.has(key)) views.delete(key);
    for (const [key, store] of accounts) {
        if (!live.has(key)) {
            void store
                .disconnect()
                .then(() =>
                    chatDesktopService.releaseAccount(
                        store.target.bot_id,
                        store.getSnapshot().account.selfId,
                    ),
                )
                .catch(() => {});
            accounts.delete(key);
        } else {
            const target = targets.find((t) => t.bot_id === store.target.bot_id);
            if (target) store.target = target;
        }
    }
}
export function useChatSnapshot(store: ChatAccountStore) {
    return useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
}

export function restoreChatView(view: ChatViewState) {
    viewRevision = view.revision;
    views.clear();
    for (const account of view.accounts)
        views.set(accountKey(account.botId, account.selfId), account);
    selectedChatBot = view.selectedBot;
    for (const [key, store] of accounts) {
        store.viewRevision = viewRevision;
        const saved = views.get(key);
        if (saved) store.restoreView(saved);
    }
}
export function loadChatView() {
    if (viewLoadRequest) return viewLoadRequest;
    const request = viewSaveQueue.catch(() => {}).then(() => chatDesktopService.loadView(true));
    viewLoadRequest = request;
    void request
        .finally(() => {
            if (viewLoadRequest === request) viewLoadRequest = null;
        })
        .catch(() => {});
    return request;
}
const withoutReading = (view: ChatAccountView): ChatAccountView => ({
    ...view,
    reading: {},
    scroll: {},
});
function savedView(selectedBot: string | null, keepReading = false): ChatViewState {
    const saved = [...views.values()].filter(
        (view) =>
            view.active || Object.values(view.drafts).some((d) => d.text || d.attachments.length),
    );
    if (saved.length > 8) throw new Error('草稿账号超过 8 个，请先清理不再使用的草稿');
    return structuredClone({
        v: 1,
        revision: viewRevision,
        selectedBot,
        accounts: saved.map((view) =>
            keepReading && view.botId === selectedBot ? view : withoutReading(view),
        ),
    });
}
function persistView(view: ChatViewState) {
    const saved = viewSaveQueue.catch(() => {}).then(() => chatDesktopService.saveView(view));
    viewSaveQueue = saved;
    return saved;
}
export function releaseChatAccount(store: ChatAccountStore): Promise<void> {
    void cancelChatScreenshot(store);
    const key = accountKey(store.target.bot_id, store.getSnapshot().account.selfId);
    store.releaseWhenIdle = true;
    if (store.viewRevision === viewRevision)
        views.set(key, handoffInProgress ? store.view() : withoutReading(store.view()));
    if (store.releaseRequest) return store.releaseRequest;
    let saving = Promise.resolve();
    if (!handoffInProgress && store.viewRevision === viewRevision) {
        try {
            saving = persistView(savedView(selectedChatBot));
        } catch (error) {
            return Promise.reject(error);
        }
    }
    const release = Promise.resolve()
        .then(async () => {
            await saving;
            if (store.isSending() || store.hasViewers()) return;
            await store.flushForRelease();
            if (store.hasViewers()) return;
            await store.disconnect();
            await store.flushForRelease();
            if (!store.hasViewers() && accounts.get(key) === store) accounts.delete(key);
            inlineImageService.release(store.target.bot_id, store.getSnapshot().account.selfId);
            await chatDesktopService.releaseAccount(
                store.target.bot_id,
                store.getSnapshot().account.selfId,
            );
        })
        .finally(() => {
            if (store.releaseRequest === release) store.releaseRequest = null;
        });
    store.releaseRequest = release;
    return release;
}
export async function prepareChatHandoff(
    selectedBot: string | null,
    unmount?: () => void,
    keepReading = true,
) {
    await cancelChatScreenshot();
    // 控制台未挂聊天页时，runtime 中可能是上次独立窗留下的新草稿。
    if (!accounts.size) {
        unmount?.();
        await chatDesktopService.flush();
        return;
    }
    if ([...accounts.values()].some((store) => store.isSending()))
        throw new Error('消息仍在发送，请稍后切换窗口');
    for (const [key, store] of accounts) views.set(key, store.view());
    const view = savedView(selectedBot, keepReading);
    handoffInProgress = true;
    try {
        unmount?.();
        await persistView(view);
        await Promise.all([...accounts.values()].map(releaseChatAccount));
        await chatDesktopService.flush();
    } finally {
        handoffInProgress = false;
    }
}
