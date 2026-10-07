import { useEffect, useId, useRef, useState, type KeyboardEvent } from 'react';
import { defaultRangeExtractor, useVirtualizer } from '@tanstack/react-virtual';
import { BellOff, CheckCheck, EyeOff, Pin, PinOff } from 'lucide-react';
import { EMPTY_DRAFT, type Contact, type SessionKey } from '../../core/domain/chat/model';
import { conversationDate } from '../../core/domain/chat/conversationDate';
import { useChatSnapshot, type ChatAccountStore } from '../../hooks/chat/chatStore';
import {
    ContextMenu,
    ContextMenuCheckboxItem,
    ContextMenuContent,
    ContextMenuItem,
    ContextMenuSeparator,
    ContextMenuTrigger,
} from '../../shared/ui/ContextMenu';
import { ChatAvatar as Avatar } from '../../shared/chat/ChatAvatar';
import { ChatUnreadBadge, useChatSelectionMotion } from './chatMotion';
import './conversationList.css';

export function useConversationNavigation(rows: Contact[], onOpen: (contact: Contact) => void) {
    const listId = useId();
    const [highlighted, setHighlighted] = useState<SessionKey | null>(null);
    const index = rows.findIndex((row) => row.key === highlighted);
    return {
        listId,
        highlighted: index >= 0 ? highlighted : null,
        activeDescendant: index >= 0 ? `${listId}-${highlighted}` : undefined,
        reset: () => setHighlighted(null),
        onKeyDown: (event: KeyboardEvent<HTMLInputElement>) => {
            if (
                event.defaultPrevented ||
                event.nativeEvent.isComposing ||
                event.keyCode === 229 ||
                event.ctrlKey ||
                event.metaKey ||
                event.altKey ||
                !rows.length
            )
                return;
            if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
                event.preventDefault();
                const next =
                    event.key === 'ArrowDown'
                        ? Math.min(index + 1, rows.length - 1)
                        : index < 0
                          ? rows.length - 1
                          : Math.max(0, index - 1);
                setHighlighted(rows[next].key);
            } else if (event.key === 'Enter') {
                event.preventDefault();
                onOpen(rows[Math.max(0, index)]);
                setHighlighted(null);
            } else if (event.key === 'Escape') setHighlighted(null);
        },
    };
}

interface ConversationListProps {
    rows: Contact[];
    active: SessionKey | null;
    store: ChatAccountStore;
    onOpen: (contact: Contact) => void;
    contacts: boolean;
    listId: string;
    highlighted: SessionKey | null;
    onHide?: (contact: Contact) => void;
    ignoredGroups?: Set<string>;
    qqMuted?: Map<string, boolean | null>;
    onIgnoreGroup?: (contact: Contact, ignored: boolean) => void;
    notificationBusy?: boolean;
}

export function ConversationList({
    rows,
    active,
    store,
    onOpen,
    contacts,
    listId,
    highlighted,
    onHide,
    ignoredGroups,
    qqMuted,
    onIgnoreGroup,
    notificationBusy,
}: ConversationListProps) {
    const scroll = useRef<HTMLDivElement>(null);
    const account = useChatSnapshot(store).account;
    const highlightedIndex = rows.findIndex((row) => row.key === highlighted);
    useChatSelectionMotion(
        scroll,
        rows.findIndex((row) => row.key === active),
        active,
    );
    const virtual = useVirtualizer({
        count: rows.length,
        getScrollElement: () => scroll.current,
        estimateSize: () => 68,
        overscan: 6,
        getItemKey: (i) => rows[i].key,
        // 保留键盘选中的行，使 combobox 的 aria-activedescendant 始终指向已挂载的选项。
        rangeExtractor: (range) =>
            Array.from(
                new Set([
                    ...defaultRangeExtractor(range),
                    ...(highlightedIndex >= 0 ? [highlightedIndex] : []),
                ]),
            ).sort((a, b) => a - b),
    });
    useEffect(() => {
        if (highlightedIndex >= 0) virtual.scrollToIndex(highlightedIndex, { align: 'auto' });
    }, [highlightedIndex, virtual]);
    return (
        <div
            className="native-chat-conversations"
            ref={scroll}
            id={listId}
            role="listbox"
            aria-label={contacts ? '联系人' : '会话'}
        >
            <div
                role="presentation"
                style={{ height: virtual.getTotalSize(), position: 'relative' }}
            >
                <span className="native-chat-selection-indicator" aria-hidden />
                {virtual.getVirtualItems().map((row) => {
                    const contact = rows[row.index];
                    const ignored = contact.type === 'group' && !!ignoredGroups?.has(contact.id);
                    const muted =
                        ignored || (contact.type === 'group' && qqMuted?.get(contact.id) === true);
                    const conversation = account.conversations[contact.key];
                    const draft = account.drafts[contact.key] ?? EMPTY_DRAFT;
                    const date =
                        !contacts && conversation ? conversationDate(conversation.lastAt) : null;
                    const hasMenu =
                        !!onHide ||
                        (!contacts && !!conversation) ||
                        (contact.type === 'group' && !!onIgnoreGroup);
                    return (
                        <ContextMenu key={row.key}>
                            <div
                                role="presentation"
                                className="native-chat-conversation-row"
                                style={{ transform: `translateY(${row.start}px)` }}
                                data-active={active === contact.key}
                                data-highlighted={highlighted === contact.key}
                            >
                                <ContextMenuTrigger asChild disabled={!hasMenu}>
                                    <button
                                        type="button"
                                        id={`${listId}-${contact.key}`}
                                        role="option"
                                        aria-selected={highlighted === contact.key}
                                        aria-current={active === contact.key ? 'true' : undefined}
                                        aria-posinset={row.index + 1}
                                        aria-setsize={rows.length}
                                        aria-haspopup={hasMenu ? 'menu' : undefined}
                                        title={hasMenu ? '右键或 Shift+F10 管理会话' : undefined}
                                        className="native-chat-conversation-button"
                                        onClick={() => onOpen(contact)}
                                        onKeyDown={(event) => {
                                            if (
                                                !hasMenu ||
                                                event.nativeEvent.isComposing ||
                                                !(
                                                    event.key === 'ContextMenu' ||
                                                    (event.shiftKey && event.key === 'F10')
                                                )
                                            )
                                                return;
                                            event.preventDefault();
                                            const rect =
                                                event.currentTarget.getBoundingClientRect();
                                            event.currentTarget.dispatchEvent(
                                                new MouseEvent('contextmenu', {
                                                    bubbles: true,
                                                    clientX: rect.left + 20,
                                                    clientY: rect.top + rect.height / 2,
                                                }),
                                            );
                                        }}
                                    >
                                        <Avatar contact={contact} />
                                        <span className="min-w-0 flex-1">
                                            <span className="flex items-center gap-1.5">
                                                <span className="truncate text-[13px] font-medium">
                                                    {contact.name}
                                                </span>
                                                {muted && (
                                                    <span
                                                        title={
                                                            ignored
                                                                ? '已开启本地免打扰'
                                                                : 'QQ 已开启免打扰'
                                                        }
                                                    >
                                                        <BellOff
                                                            size={11}
                                                            aria-label="消息免打扰"
                                                            className="shrink-0 text-text-tertiary"
                                                        />
                                                    </span>
                                                )}
                                                {conversation?.pinned && (
                                                    <Pin
                                                        size={10}
                                                        aria-label="已置顶"
                                                        className="shrink-0 text-text-tertiary"
                                                    />
                                                )}
                                            </span>
                                            <span className="mt-1 block truncate text-[11.5px] text-text-tertiary">
                                                {contacts ? (
                                                    `${contact.type === 'group' ? '群聊' : contact.categoryName || '好友'} · ${contact.id}`
                                                ) : draft.text || draft.attachments.length ? (
                                                    <>
                                                        <span className="text-brand">草稿 </span>
                                                        {draft.text || '[附件]'}
                                                    </>
                                                ) : (
                                                    conversation?.preview || '暂无消息'
                                                )}
                                            </span>
                                        </span>
                                        <span className="flex h-10 shrink-0 flex-col items-end justify-between">
                                            <time
                                                className="text-[10px] text-text-tertiary"
                                                dateTime={date?.dateTime}
                                                title={date?.title}
                                            >
                                                {date?.label}
                                            </time>
                                            {!contacts && (
                                                <ChatUnreadBadge
                                                    count={conversation?.unread ?? 0}
                                                />
                                            )}
                                        </span>
                                    </button>
                                </ContextMenuTrigger>
                            </div>
                            {hasMenu && (
                                <ContextMenuContent aria-label={`${contact.name}的会话操作`}>
                                    {conversation && (
                                        <ContextMenuItem onSelect={() => store.pin(contact.key)}>
                                            {conversation.pinned ? (
                                                <PinOff size={14} />
                                            ) : (
                                                <Pin size={14} />
                                            )}
                                            {conversation.pinned ? '取消置顶' : '置顶会话'}
                                        </ContextMenuItem>
                                    )}
                                    {!!conversation?.unread && (
                                        <ContextMenuItem
                                            onSelect={() => store.markRead(contact.key)}
                                        >
                                            <CheckCheck size={14} />
                                            标为已读
                                        </ContextMenuItem>
                                    )}
                                    {conversation &&
                                        (onHide || (contact.type === 'group' && onIgnoreGroup)) && (
                                            <ContextMenuSeparator />
                                        )}
                                    {contact.type === 'group' && onIgnoreGroup && (
                                        <ContextMenuCheckboxItem
                                            checked={ignored}
                                            disabled={notificationBusy}
                                            title={
                                                qqMuted?.get(contact.id) === true
                                                    ? 'QQ 已开启免打扰，关闭本地免打扰不会改变 QQ 设置'
                                                    : '只影响此账号的本地提醒'
                                            }
                                            onCheckedChange={(checked) =>
                                                onIgnoreGroup(contact, checked === true)
                                            }
                                        >
                                            本地免打扰
                                        </ContextMenuCheckboxItem>
                                    )}
                                    {onHide && (
                                        <ContextMenuItem onSelect={() => onHide(contact)}>
                                            <EyeOff size={14} />
                                            隐藏会话
                                        </ContextMenuItem>
                                    )}
                                </ContextMenuContent>
                            )}
                        </ContextMenu>
                    );
                })}
            </div>
        </div>
    );
}
