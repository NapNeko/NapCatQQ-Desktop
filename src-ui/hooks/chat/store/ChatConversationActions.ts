import { chatService } from '../../../core/services/chat.service';
import { chatDesktopService } from '../../../core/services/chat-desktop.service';
import {
    inlineImageService,
    isInlineImageReference,
} from '../../../core/services/inline-image.service';
import {
    mergeArchiveMessages,
    mergeMessageRows,
    openConversation,
    contactFromKey,
    setDraft,
    type Contact,
    type Draft,
    type Message,
    type SessionKey,
} from '../../../core/domain/chat/model';
import { HISTORY_PAGE_SIZE } from '../../../core/domain/chat/messageWorkingSet';
import { errorText } from '../../../core/domain/errors';
import type { ChatReadingPosition } from '../../../core/ipc/generated/chat/ChatReadingPosition';
import type { ChatAccountState } from './ChatAccountState';

/** 会话命令同时协调页面状态与桌面已读状态；快照本身不触发桌面 RPC。 */
export class ChatConversationActions {
    constructor(private state: ChatAccountState) {}
    private notify(request: () => Promise<void>) {
        if (this.state.session.transport === chatService)
            void request().catch((error) => this.state.update({ error: errorText(error) }));
    }
    private publishRead(key: SessionKey) {
        this.notify(() =>
            chatDesktopService.markRead(
                this.state.session.target.bot_id,
                this.state.snapshot.account.selfId,
                key,
            ),
        );
    }
    setReading(key: SessionKey | null) {
        this.state.timeline.reading = key;
        this.notify(() =>
            chatDesktopService.setReading(
                this.state.session.target.bot_id,
                this.state.snapshot.account.selfId,
                key,
            ),
        );
        if (key && this.state.snapshot.account.conversations[key]?.unread) this.markRead(key);
    }
    open(contact: Contact) {
        this.state.timeline.resetReading(contact.key);
        const state = this.state.snapshot.account;
        const rows =
            state.archiveMessages?.filter((message) => message.session === contact.key) ?? [];
        const recent = rows
            .slice(-HISTORY_PAGE_SIZE)
            .map((message) => (message.gapBefore ? { ...message, gapBefore: false } : message));
        this.state.account(
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
        this.publishRead(contact.key);
    }
    revealArchivedMessage(value: Message | string): Message | null {
        const state = this.state.snapshot.account;
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
        this.state.timeline.restoreReading(message.session, position);
        this.state.timeline.setPagingAnchor(message.session, anchor);
        const contact =
            this.state.snapshot.contacts.find((row) => row.key === message.session) ??
            state.conversations[message.session] ??
            contactFromKey(message.session);
        const current = state.conversations[message.session];
        this.state.account({
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
        this.state.timeline.setPagingAnchor(message.session);
        this.state.timeline.readingPosition(message.session, position);
        const oldest = this.state.snapshot.account.messages.find(
            (row) => row.session === message.session && row.id,
        );
        this.state.timeline.setCursor(message.session, oldest?.id);
        this.publishRead(message.session);
        return message;
    }
    close(key: SessionKey) {
        if (this.state.snapshot.account.active === key) {
            this.state.timeline.reading = null;
            this.state.account({ ...this.state.snapshot.account, active: null });
        }
    }
    draft(key: SessionKey, draft: Draft) {
        const attachments = draft.attachments.map((attachment) => {
            if (attachment.type === 'face' || !isInlineImageReference(attachment.path))
                return attachment;
            const path = inlineImageService.source(
                this.state.session.target.bot_id,
                String(this.state.session.target.qq_id),
                attachment.path,
            );
            if (!path) throw new Error('原图片内容已释放，请重新添加');
            return { ...attachment, path };
        });
        this.state.account(setDraft(this.state.snapshot.account, key, { ...draft, attachments }));
    }
    pin(key: SessionKey) {
        const state = this.state.snapshot.account;
        const c = state.conversations[key];
        if (c)
            this.state.account({
                ...state,
                conversations: { ...state.conversations, [key]: { ...c, pinned: !c.pinned } },
            });
    }
    markRead(key: SessionKey) {
        const state = this.state.snapshot.account;
        const conversation = state.conversations[key];
        if (!conversation?.unread) return;
        this.state.account({
            ...state,
            conversations: { ...state.conversations, [key]: { ...conversation, unread: 0 } },
        });
        this.publishRead(key);
    }
    box(key: SessionKey) {
        const state = this.state.snapshot.account;
        const c = state.conversations[key];
        if (c?.type === 'group')
            this.state.account({
                ...state,
                conversations: { ...state.conversations, [key]: { ...c, boxed: !c.boxed } },
            });
    }
}
