import { ingestMessages, id } from '../../../core/domain/chat/model';
import { errorText } from '../../../core/domain/errors';
import type { ChatAccountState } from './ChatAccountState';
import type { ChatConversationActions } from './ChatConversationActions';
import type { ChatDirectory } from './ChatDirectory';

interface ConnectionLifecycle {
    canReconnect(): boolean;
    reconnect(): Promise<void>;
}
export class ChatConnection {
    private subscription: string | null = null;
    private connecting = false;
    private reconnectTimer: ReturnType<typeof setTimeout> | undefined;
    private reconnectAttempt = 0;
    constructor(
        private state: ChatAccountState,
        private directory: ChatDirectory,
        private conversations: ChatConversationActions,
        private lifecycle: ConnectionLifecycle,
    ) {}
    private interruptPending() {
        this.directory.interrupt();
        const account = this.state.snapshot.account;
        this.state.update({
            sendProgress: {},
            contactsLoading: false,
            recentLoading: false,
            history: Object.fromEntries(
                Object.entries(this.state.snapshot.history).map(([key, value]) => [
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
            !this.state.hasViewers() ||
            !this.lifecycle.canReconnect() ||
            !this.state.session.target.running ||
            this.state.session.target.online === false ||
            this.state.snapshot.connection.state !== 'stopped'
        )
            return;
        const delay = Math.min(30_000, 1000 * 2 ** Math.min(this.reconnectAttempt++, 5));
        this.reconnectTimer = setTimeout(() => {
            this.reconnectTimer = undefined;
            if (this.state.hasViewers()) void this.lifecycle.reconnect();
        }, delay);
    }
    async disconnect() {
        clearTimeout(this.reconnectTimer);
        this.reconnectTimer = undefined;
        this.reconnectAttempt = 0;
        this.conversations.setReading(null);
        this.state.session.advance();
        this.connecting = false;
        this.state.timeline.reading = null;
        const subscription = this.subscription;
        this.subscription = null;
        this.interruptPending();
        this.state.update({ connection: { state: 'stopped', reason: '已断开' }, sendProgress: {} });
        if (subscription) {
            try {
                await this.state.session.transport.unsubscribe(subscription);
            } catch (error) {
                this.state.update({ error: errorText(error) });
            }
        }
    }
    async connect() {
        if (
            this.connecting ||
            !this.state.session.target.running ||
            this.state.session.target.online === false
        )
            return;
        if (this.subscription && this.state.snapshot.connection.state !== 'stopped') return;
        clearTimeout(this.reconnectTimer);
        this.reconnectTimer = undefined;
        const old = this.subscription;
        this.subscription = null;
        const epoch = this.state.session.advance();
        this.connecting = true;
        this.interruptPending();
        this.state.update({ connection: { state: 'connecting' }, error: '' });
        try {
            if (old) await this.state.session.transport.unsubscribe(old);
            if (epoch !== this.state.session.epoch) return;
            let receivedState = false;
            const result = await this.state.session.transport.subscribe(
                this.state.session.target.bot_id,
                (batch) => {
                    if (
                        epoch !== this.state.session.epoch ||
                        batch.bot_id !== this.state.session.target.bot_id ||
                        batch.v !== 1
                    )
                        return;
                    let account = this.state.snapshot.account;
                    let connection = this.state.snapshot.connection;
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
                        this.state.media.externalizeRows(payloads),
                        false,
                        typeof document !== 'undefined' &&
                            document.visibilityState === 'visible' &&
                            document.hasFocus()
                            ? this.state.timeline.reading
                            : null,
                        account.active
                            ? this.state.timeline.currentAnchor(account.active)
                            : undefined,
                    );
                    const becameConnected =
                        connection.state === 'connected' &&
                        this.state.snapshot.connection.state !== 'connected';
                    this.state.update({ account, connection });
                    if (connection.state === 'connected') {
                        this.reconnectAttempt = 0;
                        clearTimeout(this.reconnectTimer);
                        this.reconnectTimer = undefined;
                    }
                    if (connection.state === 'stopped') this.scheduleReconnect();
                    if (becameConnected) {
                        void this.directory.loadContacts();
                        void this.directory.loadRecent();
                    }
                },
            );
            if (epoch !== this.state.session.epoch) {
                await this.state.session.transport.unsubscribe(result.subscription_id);
                return;
            }
            this.subscription = result.subscription_id;
            if (!receivedState) this.state.update({ connection: result.receiver.state });
            if (this.state.snapshot.connection.state === 'connected') this.reconnectAttempt = 0;
        } catch (error) {
            if (epoch === this.state.session.epoch)
                this.state.update({
                    error: errorText(error),
                    connection: { state: 'stopped', reason: errorText(error) },
                });
        } finally {
            if (epoch === this.state.session.epoch) {
                this.connecting = false;
                this.scheduleReconnect();
            }
        }
    }
}
