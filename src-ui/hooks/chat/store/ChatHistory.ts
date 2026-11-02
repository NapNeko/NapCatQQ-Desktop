import {
    ingestMessages,
    mergeMessageRows,
    record,
    id,
    type SessionKey,
} from '../../../core/domain/chat/model';
import { HISTORY_PAGE_SIZE } from '../../../core/domain/chat/messageWorkingSet';
import { errorText } from '../../../core/domain/errors';
import { dataOf } from './chatProtocol';
import type { ChatAccountState } from './ChatAccountState';

export class ChatHistory {
    constructor(private state: ChatAccountState) {}
    ensureHistory(key: SessionKey): Promise<void> | undefined {
        const history = this.state.snapshot.history[key];
        if (history?.loaded || history?.loading || history?.error) return;
        const saved = this.state.timeline.initialReadingPosition(key);
        return this.history(
            key,
            saved && !saved.atBottom ? (saved.messageId ?? undefined) : undefined,
        );
    }
    async latest(key: SessionKey, signal?: AbortSignal): Promise<void> {
        const epoch = this.state.session.epoch;
        if (this.state.snapshot.history[key]?.loading && !signal?.aborted) {
            await new Promise<void>((resolve) => {
                const finish = () => {
                    unsubscribe();
                    signal?.removeEventListener('abort', finish);
                    resolve();
                };
                const unsubscribe = this.state.subscribe(() => {
                    if (
                        !this.state.snapshot.history[key]?.loading ||
                        epoch !== this.state.session.epoch
                    )
                        finish();
                });
                signal?.addEventListener('abort', finish, { once: true });
            });
        }
        if (signal?.aborted || epoch !== this.state.session.epoch) return;
        await this.history(key, undefined, { signal, latest: true });
        if (
            signal?.aborted ||
            epoch !== this.state.session.epoch ||
            this.state.snapshot.history[key]?.error
        )
            return;
        this.state.timeline.clearInitialReading(key);
        const message = this.state.snapshot.account.messages
            .filter((row) => row.session === key)
            .at(-1);
        if (message)
            this.state.timeline.readingPosition(key, {
                messageKey: message.key,
                messageId: message.id ?? null,
                offset: 0,
                atBottom: true,
            });
    }
    async history(
        key: SessionKey,
        before?: string,
        navigation?: { signal?: AbortSignal; latest: boolean },
    ): Promise<void> {
        if (
            navigation?.signal?.aborted ||
            this.state.snapshot.connection.state !== 'connected' ||
            this.state.snapshot.history[key]?.loading ||
            (!navigation?.latest && !before && this.state.snapshot.history[key]?.done)
        )
            return;
        const epoch = this.state.session.epoch;
        const anchor =
            this.state.timeline.pagingAnchor(key) ?? this.state.timeline.currentAnchor(key);
        const wasDone = this.state.snapshot.history[key]?.done ?? false;
        const set = (
            loading: boolean,
            done = false,
            error = '',
            loaded = this.state.snapshot.history[key]?.loaded ?? false,
        ) =>
            this.state.update({
                history: {
                    ...this.state.snapshot.history,
                    [key]: { loading, loaded, done, error },
                },
            });
        set(true);
        try {
            const group = key.startsWith('group:');
            const cursor = navigation?.latest
                ? undefined
                : (before ?? this.state.timeline.cursor(key));
            const params: Record<string, unknown> = {
                [group ? 'group_id' : 'user_id']: this.state.session.peer(key),
                count: HISTORY_PAGE_SIZE,
                reverse_order: this.state.session.target.backend === 'snowluma',
                disable_get_url: false,
                parse_mult_msg: true,
                quick_reply: false,
                reverseOrder: false,
            };
            if (cursor) {
                if (
                    this.state.session.target.backend === 'snowluma' &&
                    !Number.isSafeInteger(Number(cursor))
                )
                    throw new Error('此协议的历史游标超出可支持范围');
                params[
                    this.state.session.target.backend === 'snowluma' ? 'message_id' : 'message_seq'
                ] = this.state.session.target.backend === 'snowluma' ? Number(cursor) : cursor;
            }
            const data = record(
                dataOf(
                    await this.state.session.transport.call(
                        this.state.session.target.bot_id,
                        group ? 'get_group_msg_history' : 'get_friend_msg_history',
                        params,
                    ),
                ),
            );
            if (epoch !== this.state.session.epoch) return;
            if (navigation?.signal?.aborted) {
                set(false, wasDone);
                return;
            }
            if (!Array.isArray(data.messages)) throw new Error('此通道未返回可识别的消息历史');
            const rows = data.messages
                .map(record)
                .sort((a, b) => Number(a.time || 0) - Number(b.time || 0));
            const previous = this.state.snapshot.account.messages.filter(
                (message) => message.session === key,
            );
            const boundary = before && previous.findIndex((message) => message.id === before);
            const left =
                typeof boundary === 'number' && boundary > 0 ? previous[boundary - 1] : undefined;
            const incoming = rows.map((row) => ({
                ...row,
                message_type: group ? 'group' : 'private',
                ...(group
                    ? { group_id: this.state.session.peer(key) }
                    : { target_id: this.state.session.peer(key) }),
            }));
            const next = id(rows[0]?.message_id);
            const initial = this.state.timeline.initialReadingPosition(key);
            const current = initial
                ? { session: key, ...initial }
                : this.state.timeline.currentAnchor(key);
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
            this.state.timeline.setPagingAnchor(key, pagingAnchor);
            const state = this.state.snapshot.account;
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
                this.state.media.externalizeRows(incoming),
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
            this.state.account(account);
            this.state.timeline.setPagingAnchor(key);
            const oldest = this.state.snapshot.account.messages.find(
                (message) => message.session === key && message.id,
            );
            if (oldest?.id) this.state.timeline.setCursor(key, oldest.id);
            const done = before
                ? wasDone && oldest?.id === previous[0]?.id
                : !next || next === cursor || rows.length < HISTORY_PAGE_SIZE;
            set(false, done, '', true);
        } catch (error) {
            if (epoch === this.state.session.epoch)
                set(
                    false,
                    navigation?.signal?.aborted ? wasDone : false,
                    navigation?.signal?.aborted ? '' : errorText(error),
                );
        } finally {
            this.state.timeline.setPagingAnchor(key);
        }
    }
}
