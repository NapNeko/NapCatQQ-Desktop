// 聊天搜索弹层：DOM 位置与关闭回焦逻辑保持原样，焦点归还给哪个触发按钮由外部两个 ref 决定。
import { type MutableRefObject, type RefObject } from 'react';
import type { Account, Message } from '../../core/domain/chat/model';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import { Dialog, DialogContent, DialogTitle } from '../../shared/ui/Dialog';
import { ChatSearch } from './ChatSearch';

interface ChatSearchDialogProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    target: DebugTarget;
    account: Account;
    messages: Message[];
    searchExpanded: boolean;
    searchScope: 'conversation' | 'account';
    searchInput: RefObject<HTMLInputElement>;
    searchTrigger: RefObject<HTMLButtonElement>;
    accountSearchTrigger: RefObject<HTMLButtonElement>;
    searchRevealed: MutableRefObject<boolean>;
    history?: { loading: boolean; loaded: boolean; done: boolean; error: string };
    canLoadEarlier: boolean;
    onLoadEarlier?: () => void;
    preventRecall: boolean;
    onClose: () => void;
    onFocusComposer: () => void;
    onRevealSavedMessage: (message: Message) => void;
    onReveal: (key: string) => void;
}

export function ChatSearchDialog({
    open,
    onOpenChange,
    target,
    account,
    messages,
    searchExpanded,
    searchScope,
    searchInput,
    searchTrigger,
    accountSearchTrigger,
    searchRevealed,
    history,
    canLoadEarlier,
    onLoadEarlier,
    preventRecall,
    onClose,
    onFocusComposer,
    onRevealSavedMessage,
    onReveal,
}: ChatSearchDialogProps) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                size="lg"
                hideClose
                className="native-chat-search-dialog"
                onCloseAutoFocus={(event) => {
                    event.preventDefault();
                    if (searchRevealed.current) {
                        searchRevealed.current = false;
                        onFocusComposer();
                    } else {
                        (searchScope === 'account'
                            ? accountSearchTrigger
                            : searchTrigger
                        ).current?.focus();
                    }
                }}
            >
                <DialogTitle className="native-chat-search-title">
                    <span>聊天记录</span>
                    {target.name}
                </DialogTitle>
                <ChatSearch
                    target={target}
                    initialSearchExpanded={searchExpanded}
                    inputRef={searchInput}
                    messages={messages}
                    archivedMessages={account.archiveMessages ?? account.messages}
                    conversations={account.conversations}
                    currentSession={account.active ?? undefined}
                    initialScope={searchScope}
                    preventRecall={preventRecall}
                    history={history}
                    canLoadEarlier={canLoadEarlier}
                    onLoadEarlier={onLoadEarlier}
                    onClose={onClose}
                    onRevealMessage={onRevealSavedMessage}
                    onReveal={onReveal}
                />
            </DialogContent>
        </Dialog>
    );
}
