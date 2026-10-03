import { useEffect, useRef, useState, type KeyboardEvent, type ReactElement, type ReactNode } from 'react';
import { AtSign, Check, Copy, Reply as ReplyIcon, RotateCcw } from 'lucide-react';
import { EMPTY_DRAFT, type Contact, type Message } from '../../core/domain/chat/model';
import { recoverDraft } from '../../core/domain/chat/recoverDraft';
import { messagePreview } from '../../core/domain/debug/segments';
import type { ChatAccountStore } from '../../hooks/chat/chatStore';
import { ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuTrigger } from '../../shared/ui/ContextMenu';
import { mentionMessageSender } from './messageActions';

interface Props {
    store: ChatAccountStore;
    contact: Contact;
    message: Message;
    onFocusComposer?: () => void;
    onError: (error: string) => void;
    children: (controls: ReactNode) => ReactElement;
}

/** Both the context menu and visible controls use the same draft-preserving actions. */
export function ChatMessageActions({ store, contact, message, onFocusComposer, onError, children }: Props) {
    const [copied, setCopied] = useState(false);
    const restoreComposerFocus = useRef(false);
    useEffect(() => {
        if (!copied) return;
        const timer = setTimeout(() => setCopied(false), 1800);
        return () => clearTimeout(timer);
    }, [copied]);
    const currentDraft = () => store.getSnapshot().account.drafts[contact.key] ?? EMPTY_DRAFT;
    const reply = () => {
        if (!message.id) return;
        store.draft(contact.key, { ...currentDraft(), reply: { id: message.id, name: message.mine ? '我' : message.senderName, preview: messagePreview(message.segments) } });
        onFocusComposer?.();
    };
    const mention = () => {
        const sameName = new Set(store.getSnapshot().account.messages
            .filter(item => item.session === contact.key && item.senderName === message.senderName)
            .map(item => item.senderId));
        sameName.add(message.senderId);
        store.draft(contact.key, mentionMessageSender(currentDraft(), message, sameName.size));
        onFocusComposer?.();
    };
    const recover = () => {
        store.draft(contact.key, recoverDraft(message, currentDraft()));
        onFocusComposer?.();
    };
    const copy = async () => {
        setCopied(false);
        try {
            await navigator.clipboard.writeText(messagePreview(message.segments));
            setCopied(true);
        } catch {
            onError('复制失败，请重试或选择消息文字后复制');
        }
    };
    const selectDraftAction = (action: () => void) => {
        restoreComposerFocus.current = true;
        action();
    };
    const openWithKeyboard = (event: KeyboardEvent<HTMLElement>) => {
        if (event.key !== 'ContextMenu' && !(event.key === 'F10' && event.shiftKey)) return;
        event.preventDefault();
        const target = event.target instanceof HTMLElement ? event.target : event.currentTarget;
        const bounds = target.getBoundingClientRect();
        event.currentTarget.dispatchEvent(new MouseEvent('contextmenu', {
            bubbles: true, cancelable: true,
            clientX: bounds.left + Math.min(24, bounds.width / 2),
            clientY: bounds.top + Math.min(24, bounds.height / 2),
        }));
    };
    const canMention = contact.type === 'group' && !message.mine && !!message.senderId;
    const controls = message.recalled ? null : <div className="native-chat-message-actions" style={copied ? { opacity: 1 } : undefined}>
        {message.id && <button className="native-chat-icon" aria-label={`回复${message.senderName}的消息`} title="回复" onClick={reply}><ReplyIcon size={14} /></button>}
        <button className="native-chat-icon" aria-label={copied ? '消息已复制' : '复制消息'} title={copied ? '已复制' : '复制'} onClick={() => void copy()}>{copied ? <Check size={13} /> : <Copy size={13} />}</button>
        {message.status === 'failed' && <button className="native-chat-icon" aria-label="将失败消息放回输入框" title="放回输入框" onClick={recover}><RotateCcw size={13} /></button>}
        <span className="sr-only" role="status" aria-live="polite">{copied ? '已复制消息' : ''}</span>
    </div>;
    if (message.recalled) return children(null);
    return <ContextMenu onOpenChange={open => { if (open) restoreComposerFocus.current = false; }}>
        <ContextMenuTrigger asChild tabIndex={0} onKeyDown={openWithKeyboard}>{children(controls)}</ContextMenuTrigger>
        <ContextMenuContent aria-label="消息操作" onCloseAutoFocus={event => {
            if (restoreComposerFocus.current && onFocusComposer) {
                event.preventDefault();
                restoreComposerFocus.current = false;
                onFocusComposer();
            }
        }}>
            {message.id && <ContextMenuItem onSelect={() => selectDraftAction(reply)}><ReplyIcon size={14} />回复</ContextMenuItem>}
            <ContextMenuItem onSelect={() => void copy()}><Copy size={14} />复制消息</ContextMenuItem>
            {canMention && <ContextMenuItem onSelect={() => selectDraftAction(mention)}><AtSign size={14} />提及 {message.senderName}</ContextMenuItem>}
            {message.status === 'failed' && <ContextMenuItem onSelect={() => selectDraftAction(recover)}><RotateCcw size={14} />放回输入框</ContextMenuItem>}
        </ContextMenuContent>
    </ContextMenu>;
}
