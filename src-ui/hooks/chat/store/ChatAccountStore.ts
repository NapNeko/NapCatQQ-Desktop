import { chatService } from '../../../core/services/chat.service';
import { chatArchiveService } from '../../../core/services/chat-archive.service';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import type { ChatTransport, ChatArchivePort } from './chatStore.types';
import { ChatSession } from './ChatSession';
import { ChatAccountState } from './ChatAccountState';
import { ChatConversationActions } from './ChatConversationActions';
import { ChatArchive } from './ChatArchive';
import { ChatDirectory } from './ChatDirectory';
import { ChatConnection } from './ChatConnection';
import { ChatHistory } from './ChatHistory';
import { ChatOperations } from './ChatOperations';
import { ChatSendQueue } from './ChatSendQueue';
import { ChatMessageActions } from './ChatMessageActions';
import { ChatForwarding } from './ChatForwarding';
import type { ChatTimeline } from './ChatTimeline';

/** 单账号生命周期协调器；公开方法保持兼容，状态与异步任务由各自所有者管理。 */
export class ChatAccountStore {
    private state: ChatAccountState;
    private archive: ChatArchive;
    private directory: ChatDirectory;
    private connection: ChatConnection;
    private operations: ChatOperations;
    private releaseOnIdle?: () => void;
    private releaseRequest: Promise<void> | null = null;
    private releasePending = false;
    private releaseRevision = 0;
    get releaseWhenIdle() {
        return this.releasePending;
    }
    /** 释放请求永久使先前视图任务失效，重新挂载不会复用其所有权。 */
    get ownerRevision() {
        return this.releaseRevision;
    }
    viewRevision = 0;
    get target() {
        return this.state.session.target;
    }
    set target(target: DebugTarget) {
        this.state.session.target = target;
    }
    readonly getSnapshot: ChatAccountState['getSnapshot'];
    readonly subscribe: ChatAccountState['subscribe'];
    readonly hasViewers: ChatAccountState['hasViewers'];
    readonly setReading: ChatConversationActions['setReading'];
    readonly open: ChatConversationActions['open'];
    readonly revealArchivedMessage: ChatConversationActions['revealArchivedMessage'];
    readonly close: ChatConversationActions['close'];
    readonly draft: ChatConversationActions['draft'];
    readonly pin: ChatConversationActions['pin'];
    readonly markRead: ChatConversationActions['markRead'];
    readonly box: ChatConversationActions['box'];
    readonly view: ChatAccountState['view'];
    readonly restoreView: ChatAccountState['restoreView'];
    readonly scroll: ChatTimeline['scroll'];
    readonly readingPosition: ChatTimeline['readingPosition'];
    readonly initialReadingPosition: ChatTimeline['initialReadingPosition'];
    readonly finishInitialReading: ChatTimeline['finishInitialReading'];
    readonly captureTimeline: ChatTimeline['captureTimeline'];
    readonly restore: ChatArchive['restore'];
    readonly flushArchive: ChatArchive['flushArchive'];
    readonly flushForRelease: ChatArchive['flushForRelease'];
    readonly loadRecent: ChatDirectory['loadRecent'];
    readonly loadContacts: ChatDirectory['loadContacts'];
    readonly members: ChatDirectory['members'];
    readonly ensureHistory: ChatHistory['ensureHistory'];
    readonly latest: ChatHistory['latest'];
    readonly history: ChatHistory['history'];
    readonly connect: ChatConnection['connect'];
    readonly disconnect: ChatConnection['disconnect'];
    readonly send: ChatMessageActions['send'];
    readonly command: ChatMessageActions['command'];
    readonly cancelSend: ChatMessageActions['cancelSend'];
    readonly poke: ChatMessageActions['poke'];
    readonly retry: ChatMessageActions['retry'];
    readonly recall: ChatMessageActions['recall'];
    readonly repeat: ChatMessageActions['repeat'];
    readonly forward: ChatForwarding['forward'];
    constructor(
        target: DebugTarget,
        transport: ChatTransport = chatService,
        archive: ChatArchivePort | undefined = transport === chatService
            ? chatArchiveService
            : undefined,
    ) {
        this.state = new ChatAccountState(new ChatSession(target, transport), !!archive, () =>
            this.archive.markDirty(),
        );
        this.archive = new ChatArchive(this.state, archive);
        const conversations = new ChatConversationActions(this.state);
        this.directory = new ChatDirectory(this.state);
        this.connection = new ChatConnection(this.state, this.directory, conversations, {
            canReconnect: () => !this.releaseWhenIdle,
            reconnect: () => this.initialize(),
        });
        this.operations = new ChatOperations(() => {
            if (this.releaseWhenIdle && !this.hasViewers()) this.releaseOnIdle?.();
        });
        const history = new ChatHistory(this.state);
        const queue = new ChatSendQueue(this.state, this.operations, conversations);
        const actions = new ChatMessageActions(this.state, this.operations, queue);
        const forwarding = new ChatForwarding(this.state, this.operations, queue);
        const timeline = this.state.timeline;
        this.getSnapshot = this.state.getSnapshot.bind(this.state);
        this.subscribe = this.state.subscribe.bind(this.state);
        this.hasViewers = this.state.hasViewers.bind(this.state);
        this.setReading = conversations.setReading.bind(conversations);
        this.open = conversations.open.bind(conversations);
        this.revealArchivedMessage = conversations.revealArchivedMessage.bind(conversations);
        this.close = conversations.close.bind(conversations);
        this.draft = conversations.draft.bind(conversations);
        this.pin = conversations.pin.bind(conversations);
        this.markRead = conversations.markRead.bind(conversations);
        this.box = conversations.box.bind(conversations);
        this.view = this.state.view.bind(this.state);
        this.restoreView = this.state.restoreView.bind(this.state);
        this.scroll = timeline.scroll.bind(timeline);
        this.readingPosition = timeline.readingPosition.bind(timeline);
        this.initialReadingPosition = timeline.initialReadingPosition.bind(timeline);
        this.finishInitialReading = timeline.finishInitialReading.bind(timeline);
        this.captureTimeline = timeline.captureTimeline.bind(timeline);
        this.restore = this.archive.restore.bind(this.archive);
        this.flushArchive = this.archive.flushArchive.bind(this.archive);
        this.flushForRelease = this.archive.flushForRelease.bind(this.archive);
        this.loadRecent = this.directory.loadRecent.bind(this.directory);
        this.loadContacts = this.directory.loadContacts.bind(this.directory);
        this.members = this.directory.members.bind(this.directory);
        this.ensureHistory = history.ensureHistory.bind(history);
        this.latest = history.latest.bind(history);
        this.history = history.history.bind(history);
        this.connect = this.connection.connect.bind(this.connection);
        this.disconnect = this.connection.disconnect.bind(this.connection);
        this.send = actions.send.bind(actions);
        this.command = actions.command.bind(actions);
        this.cancelSend = actions.cancelSend.bind(actions);
        this.poke = actions.poke.bind(actions);
        this.retry = actions.retry.bind(actions);
        this.recall = actions.recall.bind(actions);
        this.repeat = actions.repeat.bind(actions);
        this.forward = forwarding.forward.bind(forwarding);
    }
    isSending() {
        return this.operations.busy();
    }
    requestRelease(onIdle: () => void) {
        this.releaseRevision++;
        this.releasePending = true;
        this.releaseOnIdle = onIdle;
    }
    release(saveView: () => Promise<void>, dispose: () => Promise<void>): Promise<void> {
        if (this.releaseRequest) return this.releaseRequest;
        let saving: Promise<void>;
        try {
            saving = saveView();
        } catch (error) {
            return Promise.reject(error);
        }
        const release = Promise.resolve()
            .then(async () => {
                await saving;
                if (this.isSending() || this.hasViewers()) return;
                await this.archive.flushForRelease();
                if (this.hasViewers()) return;
                await this.connection.disconnect();
                await this.archive.flushForRelease();
                // 页面可能在断开订阅或归档落盘期间重新挂载。初始化会等待此链，
                // 但已保留的快照仍持有图片引用，因此此时不能销毁账号资源。
                if (this.hasViewers()) return;
                await dispose();
            })
            .finally(() => {
                if (this.releaseRequest === release) this.releaseRequest = null;
            });
        this.releaseRequest = release;
        return release;
    }
    async initialize() {
        const revision = this.releaseRevision;
        if (this.releaseRequest) await this.releaseRequest;
        // 重挂可能在旧释放链结束前再次离开，不能让迟到的初始化重建无人使用的连接。
        if (revision !== this.releaseRevision && !this.hasViewers()) return;
        this.releasePending = false;
        await this.archive.restore();
        if (this.releaseWhenIdle && !this.hasViewers()) return;
        if (!this.target.running || this.target.online === false) {
            await this.connection.disconnect();
            return;
        }
        await this.connection.connect();
        if (this.state.snapshot.connection.state === 'connected')
            await Promise.all([this.directory.loadContacts(), this.directory.loadRecent()]);
    }
}
