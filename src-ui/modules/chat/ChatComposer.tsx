// 会话输入协调层：组合草稿、附件、提及和编辑器，负责发送与收起后的焦点。
import { memo, useEffect, useMemo, useRef, useState, type RefObject } from 'react';
import { ArrowUp, Loader2, X } from 'lucide-react';
import type { Contact } from '../../core/domain/chat/model';
import { errorText } from '../../core/domain/errors';
import { useChatSelector, type ChatAccountStore } from '../../hooks/chat/chatStore';
import { useChatScreenshot } from '../../hooks/chat/useChatScreenshot';
import { Button } from '../../shared/ui/Button';
import { ActionMotionIcon } from '../../shared/ui/motion/ActionMotionIcon';
import { ChatAttachmentStrip } from './ChatAttachmentStrip';
import { createSendingSelector } from './chatViewSelectors';
import { useComposerResize } from './useComposerResize';
import { ChatPresence, useChatComposerMotion } from './chatMotion';
import { useComposerCollapse } from './useComposerCollapse';
import { useComposerDraft } from './composer/useComposerDraft';
import { useComposerAttachments } from './composer/useComposerAttachments';
import { useComposerMentions } from './composer/useComposerMentions';
import { ComposerEditor } from './composer/ComposerEditor';
import { ComposerTools } from './composer/ComposerTools';

export const ChatComposer = memo(function ChatComposer({
    store,
    contact,
    disabledReason,
    inputRef,
    collapsed = false,
}: {
    store: ChatAccountStore;
    contact: Contact;
    disabledReason: string;
    inputRef?: RefObject<HTMLTextAreaElement>;
    collapsed?: boolean;
}) {
    const localInput = useRef<HTMLTextAreaElement>(null);
    const input = inputRef ?? localInput;
    const composer = useRef<HTMLDivElement>(null);
    const { draft, editor } = useComposerDraft(store, contact.key, input);
    const sendingSelector = useMemo(() => createSendingSelector(contact.key), [contact.key]);
    const sending = useChatSelector(store, sendingSelector);
    const [emoji, setEmoji] = useState(false);
    const [error, setError] = useState('');
    const attachments = useComposerAttachments({
        store,
        editor,
        composer,
        input,
        collapsed,
        onError: setError,
    });
    const mentions = useComposerMentions({
        store,
        contact,
        text: draft.text,
        editor,
        enabled: !collapsed && !disabledReason,
    });
    const screenshot = useChatScreenshot(store, contact.key, collapsed, () =>
        input.current?.focus({ preventScroll: true }),
    );
    const { handle: resize, remeasure } = useComposerResize(input, composer, draft.text, collapsed);
    useComposerCollapse(composer, collapsed, remeasure);
    const previouslyCollapsed = useRef(collapsed);
    const dismissMention = mentions.dismiss;
    useEffect(() => {
        let frame = 0;
        if (collapsed) {
            setEmoji(false);
            dismissMention(true);
            if (composer.current?.contains(document.activeElement)) input.current?.blur();
        } else if (previouslyCollapsed.current) {
            frame = requestAnimationFrame(() => input.current?.focus({ preventScroll: true }));
        }
        previouslyCollapsed.current = collapsed;
        return () => cancelAnimationFrame(frame);
    }, [collapsed, input, dismissMention]);
    useEffect(() => {
        if (draft.reply && !collapsed) input.current?.focus();
    }, [draft.reply, collapsed, input]);
    const canSend =
        !collapsed &&
        !disabledReason &&
        !sending &&
        !attachments.pending &&
        !screenshot.capturing &&
        (!!draft.text.trim() || draft.attachments.length > 0);
    useChatComposerMotion(composer, canSend, error);
    const send = () => {
        if (!canSend) return;
        setEmoji(false);
        setError('');
        void store.send(contact.key).catch((error) => setError(errorText(error)));
        input.current?.focus();
    };
    return (
        <div
            ref={composer}
            className="native-chat-composer"
            data-collapsed={collapsed}
            aria-hidden={collapsed || undefined}
            {...(collapsed ? { inert: '' } : {})}
            {...attachments.handlers}
        >
            <button {...resize} />
            <div className="native-chat-editor" data-dragging={attachments.dragging}>
                <ChatPresence visible={!!draft.reply}>
                    <div className="native-chat-reply">
                        <span className="min-w-0 flex-1 truncate">
                            回复 {draft.reply?.name}：{draft.reply?.preview}
                        </span>
                        <Button
                            variant="ghost"
                            size="icon"
                            className="native-chat-icon"
                            aria-label="取消引用"
                            onClick={() => {
                                editor.patch({ reply: null });
                                input.current?.focus();
                            }}
                        >
                            <X size={13} />
                        </Button>
                    </div>
                </ChatPresence>
                <ChatAttachmentStrip
                    attachments={draft.attachments}
                    readLocalImage={attachments.readLocalImage}
                    onRemove={attachments.remove}
                />
                <ComposerTools
                    target={store.target}
                    group={contact.type === 'group'}
                    input={input}
                    screenshot={screenshot}
                    attachments={attachments}
                    collapsed={collapsed}
                    disabledReason={disabledReason}
                    emoji={emoji}
                    setEmoji={setEmoji}
                    onMention={mentions.insert}
                />
                <ComposerEditor
                    input={input}
                    contact={contact}
                    draft={draft}
                    editor={editor}
                    mentions={mentions}
                    collapsed={collapsed}
                    onSend={send}
                    onEscape={() => setEmoji(false)}
                />
                <ChatPresence visible={attachments.dragging}>
                    <div className="native-chat-drop-hint">松开添加图片或文件</div>
                </ChatPresence>
                <footer>
                    <span role="status" className={error ? 'text-danger' : 'text-text-tertiary'}>
                        {error ||
                            (screenshot.capturing
                                ? '截图中…'
                                : attachments.pending
                                  ? '正在读取附件…'
                                  : disabledReason)}
                    </span>
                    <Button
                        variant="ghost"
                        size="icon"
                        className="native-chat-send"
                        aria-label="发送消息"
                        title={disabledReason || '发送消息'}
                        disabled={!canSend}
                        onClick={send}
                    >
                        <ActionMotionIcon
                            icon={sending ? Loader2 : ArrowUp}
                            motion={sending ? 'spin' : 'none'}
                            size={16}
                        />
                    </Button>
                </footer>
            </div>
        </div>
    );
});
