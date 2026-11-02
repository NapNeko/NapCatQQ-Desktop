import {
    mergeArchiveMessages,
    mergeMessageRows,
    trimAccountMessages,
    type Account,
    type SessionKey,
} from '../../../core/domain/chat/model';
import { HISTORY_PAGE_SIZE, type ReadingAnchor } from '../../../core/domain/chat/messageWorkingSet';
import type { ChatAccountView } from '../../../core/ipc/generated/chat/ChatAccountView';
import type { ChatReadingPosition } from '../../../core/ipc/generated/chat/ChatReadingPosition';
import type { ChatSnapshot } from './chatStore.types';

/** 阅读锚点、历史游标与消息工作集必须一起更新，避免翻页后丢失阅读位置。 */
export class ChatTimeline {
    private historyCursor = new Map<SessionKey, string>();
    private pagingAnchors = new Map<SessionKey, ReadingAnchor>();
    private positions: Record<string, number> = {};
    private readingPositions: Record<string, ChatReadingPosition> = {};
    private initialReadingPositions: Record<string, ChatReadingPosition> = {};
    private timelineReaders = new Map<string, () => ChatReadingPosition | undefined>();
    reading: SessionKey | null = null;
    cursor(key: SessionKey) {
        return this.historyCursor.get(key);
    }
    setCursor(key: SessionKey, id?: string) {
        if (id) this.historyCursor.set(key, id);
        else this.historyCursor.delete(key);
    }
    pagingAnchor(key: SessionKey) {
        return this.pagingAnchors.get(key);
    }
    setPagingAnchor(key: SessionKey, anchor?: ReadingAnchor) {
        if (anchor) this.pagingAnchors.set(key, anchor);
        else this.pagingAnchors.delete(key);
    }
    resetReading(key: SessionKey) {
        delete this.initialReadingPositions[key];
        delete this.readingPositions[key];
    }
    clearInitialReading(key: SessionKey) {
        delete this.initialReadingPositions[key];
    }
    restoreReading(key: SessionKey, position: ChatReadingPosition) {
        this.readingPositions[key] = position;
        this.initialReadingPositions[key] = position;
    }
    currentAnchor(key: SessionKey): ReadingAnchor | undefined {
        const value = this.timelineReaders.get(key)?.() ?? this.readingPositions[key];
        return value ? { session: key, ...value } : undefined;
    }

    reconcile(
        previous: Account,
        patch: Partial<ChatSnapshot>,
        currentHistory: ChatSnapshot['history'],
    ): Partial<ChatSnapshot> {
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
                const history = { ...(patch.history ?? currentHistory) };
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
        return patch;
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
    view(): Pick<ChatAccountView, 'scroll' | 'reading'> {
        for (const [key, read] of this.timelineReaders) {
            const value = read();
            if (value) this.readingPosition(key, value);
        }
        return { scroll: { ...this.positions }, reading: { ...this.readingPositions } };
    }
    restoreView(view: Pick<ChatAccountView, 'scroll' | 'reading' | 'active'>) {
        this.positions = { ...view.scroll };
        this.readingPositions = { ...view.reading };
        const initial = view.active && view.reading?.[view.active];
        this.initialReadingPositions = initial && view.active ? { [view.active]: initial } : {};
    }
}
