// 按 Bot 和登录身份分区，界面离开后释放接收租约。
import { useMemo, useSyncExternalStore } from 'react';
import { cancelChatScreenshot } from './useChatScreenshot';
import { chatService } from '../../core/services/chat.service';
import { chatArchiveService } from '../../core/services/chat-archive.service';
import { chatDesktopService } from '../../core/services/chat-desktop.service';
import {
    inlineImageService,
    isInlineImageReference,
} from '../../core/services/inline-image.service';
import type { ChatAccountView } from '../../core/ipc/generated/chat/ChatAccountView';
import type { ChatViewState } from '../../core/ipc/generated/chat/ChatViewState';
import type { ChatReadingPosition } from '../../core/ipc/generated/chat/ChatReadingPosition';
import {
    archiveOf,
    restoreArchive,
    mergeRecentConversations,
} from '../../core/domain/chat/archive';
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
    parseContact,
    parseFriendCategories,
    record,
    id,
    text,
    type Account,
    type Contact,
    type Draft,
    type Message,
    type SessionKey,
} from '../../core/domain/chat/model';
import { HISTORY_PAGE_SIZE, type ReadingAnchor } from '../../core/domain/chat/messageWorkingSet';
import { errorText } from '../../core/domain/errors';
import { normalizeMessage } from '../../core/domain/debug/segments';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import type { ForwardMode } from '../../core/domain/chat/messageTransfer';
import type { ChatSnapshot, ChatTransport, ChatArchivePort } from './chatStore.types';
import { ChatMediaCodec } from './chatMediaCodec';
import { ChatMessageActions } from './chatMessageActions';
import { dataOf } from './chatProtocol';
export type { ChatSnapshot } from './chatStore.types';

export class ChatAccountStore {
    private snapshot: ChatSnapshot;
    private media: ChatMediaCodec;
    private actions: ChatMessageActions;
    private listeners = new Set<() => void>();
    private epoch = 0;
    private subscription: string | null = null;
    private connecting = false;
    private reconnectTimer: ReturnType<typeof setTimeout> | undefined;
    private reconnectAttempt = 0;
    private reading: SessionKey | null = null;
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
        private transport: ChatTransport = chatService,
        private archive: ChatArchivePort | undefined = transport === chatService
            ? chatArchiveService
            : undefined,
    ) {
        this.target = target;
        this.snapshot = {
            account: emptyAccount(String(target.qq_id)),
            contacts: [],
            sendProgress: {},
            connection: { state: 'stopped', reason: '尚未连接' },
            error: '',
            contactsLoading: false,
            hydrated: !archive,
            archiveError: '',
            recentLoading: false,
            recentError: '',
            history: {},
        };
        this.media = new ChatMediaCodec(
            () => this.target,
            (error) => this.update({ error }),
        );
        this.actions = new ChatMessageActions({
            getSnapshot: this.getSnapshot,
            getTarget: () => this.target,
            getEpoch: () => this.epoch,
            transport,
            media: this.media,
            update: (patch) => this.update(patch),
            account: (account) => this.account(account),
            peer: (key) => this.peer(key),
            currentAnchor: (key) => this.currentAnchor(key),
            draft: (key, draft) => this.draft(key, draft),
            releaseIfIdle: () => {
                if (this.releaseWhenIdle && !this.isSending() && !this.hasViewers())
                    void releaseChatAccount(this).catch(() => {});
            },
        });
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
                              segments: this.media.externalizeSegments(
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
        const previous = this.snapshot.account;
        // 草稿和会话元数据不改变图片引用；输入、已读和置顶更新不必扫描整份档案。
        if (
            account.messages !== previous.messages ||
            account.archiveMessages !== previous.archiveMessages
        )
            account = this.media.syncAccount(account);
        this.update({ account });
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
            sendProgress: {},
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
    private scheduleReconnect() {
        if (
            this.reconnectTimer ||
            this.connecting ||
            !this.hasViewers() ||
            this.releaseWhenIdle ||
            !this.target.running ||
            this.target.online === false ||
            this.snapshot.connection.state !== 'stopped'
        )
            return;
        const delay = Math.min(30_000, 1000 * 2 ** Math.min(this.reconnectAttempt++, 5));
        this.reconnectTimer = setTimeout(() => {
            this.reconnectTimer = undefined;
            if (this.hasViewers()) void this.initialize();
        }, delay);
    }
    async disconnect() {
        clearTimeout(this.reconnectTimer);
        this.reconnectTimer = undefined;
        this.reconnectAttempt = 0;
        this.setReading(null);
        this.epoch++;
        this.connecting = false;
        this.reading = null;
        const subscription = this.subscription;
        this.subscription = null;
        this.interruptPending();
        this.update({ connection: { state: 'stopped', reason: '已断开' }, sendProgress: {} });
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
        clearTimeout(this.reconnectTimer);
        this.reconnectTimer = undefined;
        const old = this.subscription;
        this.subscription = null;
        const epoch = ++this.epoch;
        this.connecting = true;
        this.interruptPending();
        this.update({ connection: { state: 'connecting' }, error: '' });
        try {
            if (old) await this.transport.unsubscribe(old);
            if (epoch !== this.epoch) return;
            let receivedState = false;
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
                    } else if (body.kind === 'receiver') {
                        receivedState = true;
                        connection = body.state;
                    } else if (body.kind === 'gap' || body.kind === 'dropped')
                        account = { ...account, gap: true };
                    account = { ...account, lastSeq: event.seq };
                }
                account = ingestMessages(
                    account,
                    this.media.externalizeRows(payloads),
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
                if (connection.state === 'connected') {
                    this.reconnectAttempt = 0;
                    clearTimeout(this.reconnectTimer);
                    this.reconnectTimer = undefined;
                }
                if (connection.state === 'stopped') this.scheduleReconnect();
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
            if (!receivedState) this.update({ connection: result.receiver.state });
            if (this.snapshot.connection.state === 'connected') this.reconnectAttempt = 0;
        } catch (error) {
            if (epoch === this.epoch)
                this.update({
                    error: errorText(error),
                    connection: { state: 'stopped', reason: errorText(error) },
                });
        } finally {
            if (epoch === this.epoch) {
                this.connecting = false;
                this.scheduleReconnect();
            }
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
                this.media.externalizeRows(incoming),
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
    send(key: SessionKey) {
        return this.actions.send(key);
    }
    command(key: SessionKey, value: string, replyId?: string) {
        return this.actions.command(key, value, replyId);
    }
    cancelSend(messageKey: string) {
        return this.actions.cancelSend(messageKey);
    }
    poke(key: SessionKey, userId: string) {
        return this.actions.poke(key, userId);
    }
    retry(messageKey: string) {
        return this.actions.retry(messageKey);
    }
    recall(messageKey: string) {
        return this.actions.recall(messageKey);
    }
    repeat(messageKey: string) {
        return this.actions.repeat(messageKey);
    }
    forward(
        messageKeys: readonly string[],
        contact: Contact,
        mode: ForwardMode = 'merged',
        onProgress?: (sent: number, total: number) => void,
    ) {
        return this.actions.forward(messageKeys, contact, mode, onProgress);
    }
    isSending() {
        return this.actions.isSending();
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

export function useChatSelector<T>(
    store: ChatAccountStore,
    selector: (snapshot: ChatSnapshot) => T,
    isEqual: (previous: T, next: T) => boolean = Object.is,
): T {
    const getSelection = useMemo(() => {
        let previousSnapshot: ChatSnapshot | undefined;
        let selection: T;
        return () => {
            const snapshot = store.getSnapshot();
            if (snapshot !== previousSnapshot) {
                const next = selector(snapshot);
                if (!previousSnapshot || !isEqual(selection, next)) selection = next;
                previousSnapshot = snapshot;
            }
            return selection;
        };
    }, [store, selector, isEqual]);
    return useSyncExternalStore(store.subscribe, getSelection, getSelection);
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
