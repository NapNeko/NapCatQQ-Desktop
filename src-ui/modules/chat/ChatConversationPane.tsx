// 当前会话栏：头部、群文件/群成员弹层、时间线与输入区；状态与动作仍由 ChatWorkspace 持有。
import { type MutableRefObject, type RefObject } from 'react';
import { ArrowLeft, FolderOpen, MessagesSquare, Search, Users } from 'lucide-react';
import type { ChatAccountStore } from '../../hooks/chat/chatStore';
import type { Contact, Conversation, Message } from '../../core/domain/chat/model';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import { Button } from '../../shared/ui/Button';
import { ChatAvatar as Avatar } from '../../shared/chat/ChatAvatar';
import { ChatDetails } from './ChatDetails';
import { GroupMembersDialog } from './GroupMembersDialog';
import { ChatGroupMemberMenu } from './ChatGroupMemberMenu';
import { GroupFilesDialog } from './files/GroupFilesDialog';
import { NativeTimeline } from './ChatTimeline';
import { ChatComposer } from './ChatComposer';

interface ChatConversationPaneProps {
    target: DebugTarget;
    store: ChatAccountStore;
    contact: Conversation | undefined;
    identity: string;
    messages: Message[];
    timelineEntry: number;
    connected: boolean;
    connectionLabel: string;
    fileSignal: string;
    narrowFocus: boolean;
    messageSelecting: boolean;
    detailsOpen: boolean;
    onDetailsOpenChange: (open: boolean) => void;
    searchOpen: boolean;
    searchTrigger: RefObject<HTMLButtonElement>;
    membersOpen: boolean;
    onMembersOpenChange: (open: boolean) => void;
    filesOpen: boolean;
    onFilesOpenChange: (open: boolean) => void;
    memberFocus?: string;
    composerInput: RefObject<HTMLTextAreaElement>;
    reveal: MutableRefObject<(key: string) => void>;
    preventRecall: boolean;
    onBack: () => void;
    onOpenSearch: () => void;
    onCloseSearch: () => void;
    onShowMembers: (memberId?: string) => void;
    onOpen: (contact: Contact) => void;
    onFocusComposer: () => void;
    onSelectionChange: (selecting: boolean) => void;
    onShowContacts: () => void;
}

export function ChatConversationPane({
    target,
    store,
    contact: active,
    identity,
    messages,
    timelineEntry,
    connected,
    connectionLabel,
    fileSignal,
    narrowFocus,
    messageSelecting,
    detailsOpen,
    onDetailsOpenChange,
    searchOpen,
    searchTrigger,
    membersOpen,
    onMembersOpenChange,
    filesOpen,
    onFilesOpenChange,
    memberFocus,
    composerInput,
    reveal,
    preventRecall,
    onBack,
    onOpenSearch,
    onCloseSearch,
    onShowMembers,
    onOpen,
    onFocusComposer,
    onSelectionChange,
    onShowContacts,
}: ChatConversationPaneProps) {
    return (
        <main className="native-chat-message-pane" aria-label="当前会话">
            <div className="native-chat-conversation-surface">
                {active ? (
                    <>
                        <header className="native-chat-conversation-header">
                            <Button
                                variant="ghost"
                                size="icon"
                                className="native-chat-back native-chat-icon"
                                aria-label="返回会话列表"
                                onClick={onBack}
                            >
                                <ArrowLeft size={18} />
                            </Button>
                            <button
                                type="button"
                                className="native-chat-profile"
                                aria-label={`查看${active.name}的资料`}
                                aria-expanded={detailsOpen}
                                onClick={() => onDetailsOpenChange(true)}
                            >
                                <Avatar contact={active} small />
                                <span className="min-w-0 text-left">
                                    <span className="block truncate text-[14px] font-semibold">
                                        {active.name}
                                    </span>
                                    <span className="block text-[11px] text-text-tertiary">
                                        {active.type === 'group'
                                            ? active.members
                                                ? `${active.members} 位成员`
                                                : '群聊'
                                            : active.id}
                                    </span>
                                </span>
                            </button>
                            <Button
                                ref={searchTrigger}
                                variant="ghost"
                                size="icon"
                                className="native-chat-icon"
                                aria-label="搜索当前会话"
                                title="搜索消息 · Ctrl+F"
                                aria-expanded={searchOpen}
                                onClick={() => (searchOpen ? onCloseSearch() : onOpenSearch())}
                            >
                                <Search size={17} />
                            </Button>
                            {active.type === 'group' && (
                                <Button
                                    variant="ghost"
                                    size="icon"
                                    className="native-chat-icon"
                                    aria-label="群成员"
                                    title="群成员"
                                    aria-expanded={membersOpen}
                                    onClick={() => onShowMembers()}
                                >
                                    <Users size={17} />
                                </Button>
                            )}
                            {active.type === 'group' && (
                                <Button
                                    variant="ghost"
                                    size="icon"
                                    className="native-chat-icon"
                                    aria-label="群文件"
                                    title="群文件"
                                    aria-expanded={filesOpen}
                                    onClick={() => onFilesOpenChange(true)}
                                >
                                    <FolderOpen size={17} />
                                </Button>
                            )}
                            <ChatDetails
                                key={active.key}
                                contact={active}
                                target={target}
                                onPin={() => store.pin(active.key)}
                                open={detailsOpen}
                                onOpenChange={onDetailsOpenChange}
                                onMessage={onOpen}
                                onSearch={onOpenSearch}
                                onMembers={
                                    active.type === 'group' ? () => onShowMembers() : undefined
                                }
                            />
                        </header>
                        {active.type === 'group' && (
                            <GroupFilesDialog
                                open={filesOpen}
                                onOpenChange={onFilesOpenChange}
                                target={target}
                                groupId={active.id}
                                groupName={active.name}
                                connected={connected}
                                refreshSignal={fileSignal}
                            />
                        )}
                        {active.type === 'group' && (
                            <GroupMembersDialog
                                key={`${identity}/${active.key}`}
                                open={membersOpen}
                                onOpenChange={onMembersOpenChange}
                                target={target}
                                contact={active}
                                connected={connected}
                                onMessage={onOpen}
                                initialMemberId={memberFocus}
                            />
                        )}
                        <NativeTimeline
                            key={`timeline:${active.key}:${timelineEntry}`}
                            store={store}
                            contact={active}
                            messages={messages}
                            preventRecall={preventRecall}
                            revealRef={reveal}
                            visible={narrowFocus}
                            onFocusComposer={onFocusComposer}
                            onSelectionChange={onSelectionChange}
                            renderAvatar={(message, avatar) => (
                                <ChatGroupMemberMenu
                                    target={target}
                                    store={store}
                                    contact={active}
                                    message={message}
                                    onMessage={onOpen}
                                    onViewMember={onShowMembers}
                                    onFocusComposer={onFocusComposer}
                                >
                                    {avatar}
                                </ChatGroupMemberMenu>
                            )}
                        />
                        <ChatComposer
                            key={`composer:${active.key}`}
                            store={store}
                            contact={active}
                            disabledReason={connected ? '' : connectionLabel}
                            inputRef={composerInput}
                            collapsed={messageSelecting}
                        />
                    </>
                ) : (
                    <div className="native-chat-welcome">
                        <span className="native-chat-welcome-mark">
                            <MessagesSquare size={36} strokeWidth={1.25} />
                        </span>
                        <h2>开始一段对话</h2>
                        <p>选择会话，或从联系人发起聊天</p>
                        <button className="native-chat-text-button" onClick={onShowContacts}>
                            查看联系人
                        </button>
                    </div>
                )}
            </div>
        </main>
    );
}
