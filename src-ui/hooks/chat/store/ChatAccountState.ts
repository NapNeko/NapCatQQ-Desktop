import {
    emptyAccount,
    type Account,
    type Message,
    type SessionKey,
} from '../../../core/domain/chat/model';
import type { ChatAccountView } from '../../../core/ipc/generated/chat/ChatAccountView';
import type { ChatSnapshot } from './chatStore.types';
import { ChatMediaCodec } from './chatMediaCodec';
import type { ChatSession } from './ChatSession';
import { ChatTimeline } from './ChatTimeline';

/** 唯一的可观察快照所有者；负责提交工作集和序列化视图，不发起网络请求。 */
export class ChatAccountState {
    private value: ChatSnapshot;
    private listeners = new Set<() => void>();
    readonly timeline = new ChatTimeline();
    readonly media: ChatMediaCodec;
    constructor(
        readonly session: ChatSession,
        persistent: boolean,
        private accountChanged: () => void,
    ) {
        this.value = {
            account: emptyAccount(String(session.target.qq_id)),
            contacts: [],
            sendProgress: {},
            connection: { state: 'stopped', reason: '尚未连接' },
            error: '',
            contactsLoading: false,
            hydrated: !persistent,
            archiveError: '',
            recentLoading: false,
            recentError: '',
            history: {},
        };
        this.media = new ChatMediaCodec(
            () => session.target,
            (error) => this.update({ error }),
        );
    }
    get snapshot() {
        return this.value;
    }
    getSnapshot = () => this.value;
    subscribe = (listener: () => void) => {
        this.listeners.add(listener);
        return () => {
            this.listeners.delete(listener);
        };
    };
    hasViewers() {
        return this.listeners.size > 0;
    }
    update(patch: Partial<ChatSnapshot>) {
        const previous = this.value.account;
        patch = this.timeline.reconcile(previous, patch, this.value.history);
        this.value = { ...this.value, ...patch };
        for (const listener of this.listeners) listener();
        if (
            patch.account &&
            (patch.account.messages !== previous.messages ||
                patch.account.conversations !== previous.conversations)
        )
            this.accountChanged();
    }
    account(account: Account) {
        const previous = this.snapshot.account;
        // 草稿和会话元数据不改变图片引用；输入、已读和置顶更新不必扫描整份档案。
        if (
            account.messages !== previous.messages ||
            account.archiveMessages !== previous.archiveMessages
        )
            account = this.media.syncAccount(account);
        this.update({ account });
    }
    findMessage(key: string): Message | undefined {
        return (
            this.snapshot.account.messages.find((message) => message.key === key) ??
            this.snapshot.account.archiveMessages?.find((message) => message.key === key)
        );
    }
    requireConnected() {
        if (
            !this.session.target.running ||
            this.session.target.online === false ||
            this.snapshot.connection.state !== 'connected'
        )
            throw new Error('聊天未连接，请连接后操作');
    }
    view(): ChatAccountView {
        const account = this.value.account;
        return {
            botId: this.session.target.bot_id,
            selfId: account.selfId,
            active: account.active,
            ...this.timeline.view(),
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
        if (view.botId !== this.session.target.bot_id || view.selfId !== this.value.account.selfId)
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
        this.timeline.restoreView(view);
        this.account({
            ...this.value.account,
            active: view.active as SessionKey | null,
            drafts,
        });
    }
}
