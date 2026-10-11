import { useId, useMemo, useRef, useState, type CSSProperties } from 'react';
import { ArrowUpRight, List, MessagesSquare, Search } from 'lucide-react';
import { useChatSnapshot, type ChatAccountStore } from '../../hooks/chat/chatStore';
import { useChatNotice } from '../../hooks/chat/useChatNotice';
import { useMotion } from '../../hooks/preferences/useMotion';
import { accountKey, type Contact } from '../../core/domain/chat/model';
import { type ForwardMode, ChatForwardError } from '../../core/domain/chat/messageTransfer';
import { errorText } from '../../core/domain/errors';
import { messagePreview } from '../../core/domain/debug/segments';
import { Button } from '../../shared/ui/Button';
import { ChatAvatar } from '../../shared/chat/ChatAvatar';
import {
    Dialog,
    DialogContent,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from '../../shared/ui/Dialog';
import { ConversationList, useConversationNavigation } from './ConversationList';
import './messageTransfer.css';

export function ChatForwardDialog({
    store,
    messageKeys,
    onClose,
    onSent,
}: {
    store: ChatAccountStore;
    messageKeys: readonly string[];
    onClose: () => void;
    onSent: () => void;
}) {
    const snapshot = useChatSnapshot(store);
    const motion = useMotion();
    const modeName = useId();
    const [query, setQuery] = useState('');
    const [target, setTarget] = useState<Contact | null>(null);
    const [mode, setMode] = useState<ForwardMode>('merged');
    const [progress, setProgress] = useState(0);
    const [busy, setBusy] = useState(false);
    const [uncertain, setUncertain] = useState(false);
    const [error, setError] = useState('');
    const inFlight = useRef(false);
    const scope = accountKey(store.target.bot_id, snapshot.account.selfId);
    useChatNotice(`${scope}:forward`, '转发消息失败', error);
    const rows = useMemo(() => {
        const contacts = new Map<string, Contact>(
            Object.values(snapshot.account.conversations).map((contact) => [contact.key, contact]),
        );
        for (const contact of snapshot.contacts) contacts.set(contact.key, contact);
        const needle = query.trim().toLocaleLowerCase();
        return [...contacts.values()]
            .filter((contact) =>
                `${contact.name} ${contact.id}`.toLocaleLowerCase().includes(needle),
            )
            .sort((a, b) => {
                const aTime = snapshot.account.conversations[a.key]?.lastAt ?? 0;
                const bTime = snapshot.account.conversations[b.key]?.lastAt ?? 0;
                return bTime - aTime || a.name.localeCompare(b.name, 'zh-CN');
            });
    }, [snapshot.contacts, snapshot.account.conversations, query]);
    const choose = (contact: Contact) => {
        if (!busy && !uncertain) setTarget(contact);
    };
    const navigation = useConversationNavigation(rows, choose);
    const messages = useMemo(() => {
        const byKey = new Map(
            [...(snapshot.account.archiveMessages ?? []), ...snapshot.account.messages].map(
                (message) => [message.key, message],
            ),
        );
        return messageKeys.flatMap((key) => {
            const message = byKey.get(key);
            return message ? [message] : [];
        });
    }, [snapshot.account.messages, snapshot.account.archiveMessages, messageKeys]);
    const disconnected =
        !store.target.running ||
        store.target.online === false ||
        snapshot.connection.state !== 'connected';
    const submit = async () => {
        if (!target || inFlight.current || uncertain) return;
        inFlight.current = true;
        setBusy(true);
        setError('');
        try {
            await store.forward(messageKeys, target, mode, setProgress);
            onSent();
        } catch (failure) {
            setUncertain(failure instanceof ChatForwardError && failure.uncertain);
            setError(errorText(failure));
        } finally {
            inFlight.current = false;
            setBusy(false);
        }
    };
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !inFlight.current) onClose();
            }}
        >
            <DialogContent
                size="md"
                className="native-chat-forward-dialog"
                dismissOnOutsideClick={!busy}
                hideClose={busy}
                style={
                    { '--forward-switch-duration': `${motion.duration('fast')}s` } as CSSProperties
                }
            >
                <DialogHeader>
                    <DialogTitle>转发 {messageKeys.length} 条消息</DialogTitle>
                </DialogHeader>
                <div
                    className="native-chat-forward-modes"
                    role="radiogroup"
                    aria-label="转发方式"
                    data-mode={mode}
                    data-disabled={busy || uncertain}
                >
                    {(['merged', 'individual'] as const).map((value) => (
                        <label key={value} className="native-chat-forward-mode">
                            <input
                                type="radio"
                                name={modeName}
                                value={value}
                                checked={mode === value}
                                disabled={busy || uncertain}
                                onChange={() => setMode(value)}
                            />
                            <span>
                                {value === 'merged' ? (
                                    <MessagesSquare size={15} strokeWidth={1.7} aria-hidden />
                                ) : (
                                    <List size={15} strokeWidth={1.7} aria-hidden />
                                )}
                                {value === 'merged' ? '合并转发' : '逐条转发'}
                            </span>
                        </label>
                    ))}
                </div>
                <section
                    className="native-chat-forward-preview"
                    aria-label="待转发消息"
                    data-mode={mode}
                >
                    <div className="native-chat-forward-preview-title">
                        {mode === 'merged' ? '聊天记录' : '逐条消息'}
                    </div>
                    <div className="native-chat-forward-preview-messages">
                        {messages.slice(0, 3).map((message) => (
                            <div key={message.key} className="native-chat-forward-preview-row">
                                <span className="native-chat-forward-preview-sender">
                                    {message.senderName}
                                </span>
                                <span className="native-chat-forward-preview-text">
                                    {messagePreview(message.segments)}
                                </span>
                            </div>
                        ))}
                    </div>
                    {messages.length > 3 && (
                        <span className="native-chat-forward-preview-more">
                            还有 {messages.length - 3} 条
                        </span>
                    )}
                </section>
                <div className="native-chat-search native-chat-forward-search">
                    <Search size={15} aria-hidden />
                    <input
                        aria-label="搜索转发目标"
                        role="combobox"
                        aria-autocomplete="list"
                        aria-expanded="true"
                        aria-controls={navigation.listId}
                        aria-activedescendant={navigation.activeDescendant}
                        placeholder="搜索联系人或会话"
                        value={query}
                        disabled={busy || uncertain}
                        onChange={(event) => {
                            setQuery(event.target.value);
                            navigation.reset();
                        }}
                        onKeyDown={navigation.onKeyDown}
                    />
                </div>
                <fieldset
                    className="native-chat-forward-contacts"
                    disabled={busy || uncertain}
                    aria-label="转发目标"
                    aria-busy={busy}
                >
                    {rows.length ? (
                        <ConversationList
                            rows={rows}
                            active={target?.key ?? null}
                            store={store}
                            onOpen={choose}
                            contacts
                            listId={navigation.listId}
                            highlighted={navigation.highlighted}
                        />
                    ) : (
                        <div className="native-chat-message-empty">
                            {snapshot.contactsLoading
                                ? '正在读取联系人…'
                                : query
                                  ? '没有匹配的联系人或会话'
                                  : '还没有联系人或会话'}
                        </div>
                    )}
                </fieldset>
                <DialogFooter className="native-chat-forward-footer">
                    <span className="native-chat-forward-target" aria-live="polite">
                        {target ? (
                            <>
                                <ChatAvatar contact={target} small />
                                <span className="native-chat-forward-target-label">发给</span>
                                <span
                                    className="native-chat-forward-target-name"
                                    title={target.name}
                                >
                                    {target.name}
                                </span>
                            </>
                        ) : (
                            '选择转发目标'
                        )}
                    </span>
                    <Button variant="ghost" size="sm" disabled={busy} onClick={onClose}>
                        取消
                    </Button>
                    <Button
                        variant="primary"
                        size="sm"
                        disabled={busy || uncertain || disconnected || !target}
                        onClick={() => void submit()}
                    >
                        {!busy && !uncertain && <ArrowUpRight size={14} aria-hidden />}
                        {busy
                            ? mode === 'individual'
                                ? `正在转发 ${progress}/${messageKeys.length}`
                                : '正在转发…'
                            : uncertain
                              ? '结果待确认'
                              : '转发'}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
