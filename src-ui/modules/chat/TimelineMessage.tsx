// 单行消息负责分组、头像、消息操作与发送反馈；时间线只管理视口与阅读状态。
import { memo, type ReactNode } from 'react';
import type { ChatAccountStore } from '../../hooks/chat/chatStore';
import type { Contact, Message } from '../../core/domain/chat/model';
import type { DebugStreamProgress } from '../../core/ipc/generated/debug/DebugStreamProgress';
import { ChatAvatar as Avatar } from '../../shared/chat/ChatAvatar';
import { dayLabel } from '../../core/domain/debug/chatFormat';
import { errorText } from '../../core/domain/errors';
import { cn } from '../../shared/utils/cn';
import { ChatMessageActions } from './ChatMessageActions';
import { ChatAvatarMenu } from './ChatAvatarMenu';
import { ChatMessageEntrance } from './chatMotion';
import { ChatSendStatus } from './ChatSendStatus';
import { ChatMessageContent } from './ChatMessageContent';

export const TimelineMessage = memo(function TimelineMessage({
    store,
    contact,
    message,
    animateEntry,
    progress,
    previous,
    highlighted,
    retryDisabled,
    preventRecall,
    renderAvatar,
    onFocusComposer,
    onError,
    selecting,
    selected,
    selectionFull,
    onSelect,
    onToggleSelection,
    onForward,
    onLoadGap,
    historyLoading,
    enter,
    takeEnter,
    order,
}: {
    store: ChatAccountStore;
    contact: Contact;
    message: Message;
    animateEntry: boolean;
    progress?: DebugStreamProgress;
    previous?: Message;
    highlighted: boolean;
    retryDisabled: boolean;
    preventRecall: boolean;
    renderAvatar?: (message: Message, avatar: ReactNode) => ReactNode;
    onFocusComposer?: () => void;
    onError: (error: string) => void;
    selecting: boolean;
    selected: boolean;
    selectionFull: boolean;
    onSelect: (messageKey: string) => void;
    onToggleSelection: (messageKey: string) => void;
    onForward: (messageKey: string) => void;
    onLoadGap: (message: Message) => void;
    historyLoading: boolean;
    enter: boolean;
    takeEnter: (key: string) => boolean;
    order: number;
}) {
    const showTime =
        !previous ||
        message.at - previous.at > 5 * 60_000 ||
        new Date(message.at).toDateString() !== new Date(previous.at).toDateString();
    const continuation =
        !showTime &&
        previous?.senderId === message.senderId &&
        previous?.mine === message.mine &&
        message.at - previous.at < 3 * 60_000;
    const gap = message.gapBefore && (
        <div className="native-chat-notice">
            <button
                type="button"
                className="native-chat-text-button"
                disabled={historyLoading || !message.id}
                onClick={() => onLoadGap(message)}
            >
                {historyLoading ? '正在加载…' : '加载这段消息'}
            </button>
        </div>
    );
    if (message.notice) {
        return (
            <>
                {gap}
                {showTime && <div className="native-chat-time">{dayLabel(message.at)}</div>}
                <div className="native-chat-notice">{message.notice}</div>
            </>
        );
    }
    const avatar = (
        <Avatar
            contact={{
                type: 'private',
                id: message.senderId,
                name: message.mine ? store.target.name : message.senderName,
            }}
            small
        />
    );
    return (
        <>
            {gap}
            {showTime && <div className="native-chat-time">{dayLabel(message.at)}</div>}
            <ChatMessageEntrance
                messageKey={message.key}
                mine={message.mine}
                enter={enter}
                takeEnter={takeEnter}
                order={order}
                animate={animateEntry}
            >
                <ChatMessageActions
                    store={store}
                    contact={contact}
                    message={message}
                    onFocusComposer={onFocusComposer}
                    onError={onError}
                    onForward={onForward}
                    onSelect={onSelect}
                    selecting={selecting}
                    selected={selected}
                    selectionDisabled={!selected && selectionFull}
                    onToggleSelection={onToggleSelection}
                >
                    {(controls, wrapContent) => (
                        <article
                            className={cn(
                                'native-chat-message',
                                message.mine && 'is-mine',
                                continuation && 'is-continuation',
                                showTime && 'is-after-time',
                            )}
                            data-highlight={highlighted}
                            data-selected={selected}
                            data-selecting={selecting}
                            data-local-send={!!message.requestId}
                        >
                            {continuation ? (
                                <span className="native-chat-avatar-space" aria-hidden />
                            ) : renderAvatar ? (
                                renderAvatar(message, avatar)
                            ) : (
                                <ChatAvatarMenu
                                    store={store}
                                    contact={contact}
                                    message={message}
                                    onFocusComposer={onFocusComposer}
                                    onError={onError}
                                >
                                    {avatar}
                                </ChatAvatarMenu>
                            )}
                            <div className="native-chat-message-body">
                                {contact.type === 'group' && !message.mine && !continuation && (
                                    <div className="native-chat-sender" title={message.senderName}>
                                        {message.senderName}
                                    </div>
                                )}
                                <div className="native-chat-message-content-line">
                                    {wrapContent(
                                        <ChatMessageContent
                                            message={message}
                                            preventRecall={preventRecall}
                                        />,
                                    )}
                                    {message.mine && (
                                        <span className="native-chat-delivery-slot">
                                            <ChatSendStatus
                                                status={message.status}
                                                progress={progress}
                                                onCancel={
                                                    selecting
                                                        ? undefined
                                                        : async () => {
                                                              try {
                                                                  await store.cancelSend(
                                                                      message.key,
                                                                  );
                                                              } catch (error) {
                                                                  onError(errorText(error));
                                                                  throw error;
                                                              }
                                                          }
                                                }
                                                error={message.error}
                                                retryDisabled={retryDisabled || selecting}
                                                onRetry={
                                                    message.recalled ||
                                                    message.segments.some(
                                                        (segment) => segment.type === 'forward',
                                                    )
                                                        ? undefined
                                                        : () => {
                                                              onError('');
                                                              void store
                                                                  .retry(message.key)
                                                                  .catch((error) =>
                                                                      onError(errorText(error)),
                                                                  );
                                                          }
                                                }
                                            />
                                        </span>
                                    )}
                                    {controls}
                                </div>
                            </div>
                        </article>
                    )}
                </ChatMessageActions>
            </ChatMessageEntrance>
        </>
    );
});
