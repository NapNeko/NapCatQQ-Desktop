// 会话草稿即时写回账号分区，异步选文件不改变发送目标。
import {
    useEffect,
    useId,
    useMemo,
    useRef,
    useState,
    type ClipboardEvent,
    type DragEvent,
    type KeyboardEvent,
    type RefObject,
} from 'react';
import { ArrowUp, AtSign, File, ImagePlus, Loader2, Paperclip, Smile, X } from 'lucide-react';
import {
    EMPTY_DRAFT,
    type Attachment,
    type Contact,
    type Draft,
} from '../../core/domain/chat/model';
import { mentionLabel, mentionQueryAt, pruneMentions } from '../../core/domain/debug/composerModel';
import { errorText } from '../../core/domain/errors';
import { useChatSend } from '../../hooks/chat/useChatSend';
import { useChatSnapshot, type ChatAccountStore } from '../../hooks/chat/chatStore';
import { Popover, PopoverAnchor, PopoverContent, PopoverTrigger } from '../../shared/ui/Popover';
import { ChatEmojiPicker } from './media/ChatEmojiPicker';
import { QQFace } from './media/QQFace';
import { ChatAvatar } from './ChatAvatar';
import { useComposerResize } from './useComposerResize';
import { Button } from '../../shared/ui/Button';
import { ActionMotionIcon } from '../../shared/ui/motion/ActionMotionIcon';
import { ChatPresence, useChatComposerMotion } from './chatMotion';
import { useComposerCollapse } from './useComposerCollapse';

export function ChatComposer({
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
    const snapshot = useChatSnapshot(store);
    const { pickFile } = useChatSend(store);
    const draft = snapshot.account.drafts[contact.key] ?? EMPTY_DRAFT;
    const localInput = useRef<HTMLTextAreaElement>(null);
    const input = inputRef ?? localInput;
    const composing = useRef(false);
    const composer = useRef<HTMLDivElement>(null);
    const previouslyCollapsed = useRef(collapsed);
    const { handle: resize, remeasure } = useComposerResize(input, composer, draft.text, collapsed);
    useComposerCollapse(composer, collapsed, remeasure);
    const [emoji, setEmoji] = useState(false);
    const [error, setError] = useState('');
    const restoreAfterEmoji = useRef(false);
    const [pendingFiles, setPendingFiles] = useState(0);
    const [dragging, setDragging] = useState(false);
    const dragDepth = useRef(0);
    const [caret, setCaret] = useState(0);
    const [members, setMembers] = useState<{ id: string; name: string }[]>([]);
    const [memberError, setMemberError] = useState('');
    const [memberLoading, setMemberLoading] = useState(false);
    const [mentionDismissed, dismissMention] = useState(false);
    const [memberIndex, setMemberIndex] = useState(0);
    const [memberRetry, retryMembers] = useState(0);
    const membersLoaded = useRef(false);
    const memberList = useRef<HTMLDivElement>(null);
    const memberListId = useId();
    const query =
        contact.type === 'group' && !mentionDismissed ? mentionQueryAt(draft.text, caret) : null;
    const mentionOpen = query !== null;
    const memberVisible = mentionOpen && !disabledReason && !collapsed;
    useEffect(() => {
        let frame = 0;
        if (collapsed) {
            restoreAfterEmoji.current = false;
            setEmoji(false);
            dismissMention(true);
            setDragging(false);
            dragDepth.current = 0;
            if (composer.current?.contains(document.activeElement)) input.current?.blur();
        } else if (previouslyCollapsed.current) {
            frame = requestAnimationFrame(() => input.current?.focus({ preventScroll: true }));
        }
        previouslyCollapsed.current = collapsed;
        return () => cancelAnimationFrame(frame);
    }, [collapsed, input]);
    useEffect(() => {
        if (!mentionOpen || membersLoaded.current || disabledReason || collapsed) return;
        let cancelled = false;
        setMemberLoading(true);
        setMemberError('');
        void store
            .members(contact.key)
            .then((value) => {
                if (!cancelled) {
                    setMembers(value);
                    membersLoaded.current = true;
                }
            })
            .catch((e) => {
                if (!cancelled) setMemberError(errorText(e));
            })
            .finally(() => {
                if (!cancelled) setMemberLoading(false);
            });
        return () => {
            cancelled = true;
        };
    }, [mentionOpen, store, contact.key, disabledReason, memberRetry, collapsed]);
    const filteredMembers = query
        ? members
              .filter((m) =>
                  `${m.name} ${m.id}`.toLocaleLowerCase().includes(query.query.toLocaleLowerCase()),
              )
              .slice(0, 30)
        : [];
    const selectedMember = Math.min(memberIndex, Math.max(0, filteredMembers.length - 1));
    useEffect(() => {
        setMemberIndex(0);
    }, [query?.query]);
    useEffect(() => {
        memberList.current
            ?.querySelector('[aria-selected=true]')
            ?.scrollIntoView?.({ block: 'nearest' });
    }, [selectedMember]);
    useEffect(() => {
        if (draft.reply && !collapsed) input.current?.focus();
    }, [draft.reply, collapsed]);
    const current = () => store.getSnapshot().account.drafts[contact.key] ?? EMPTY_DRAFT;
    const patch = (value: Partial<Draft>) => store.draft(contact.key, { ...current(), ...value });
    const chooseEmoji = (attachment: Attachment) => {
        if (current().attachments.length >= 8) {
            setError('一次最多添加 8 个附件');
            return;
        }
        patch({ attachments: [...current().attachments, attachment] });
        restoreAfterEmoji.current = true;
        setEmoji(false);
        setError('');
    };
    const insert = (value: string) => {
        const currentDraft = current();
        const start = input.current?.selectionStart ?? currentDraft.text.length;
        const end = input.current?.selectionEnd ?? start;
        const text = currentDraft.text.slice(0, start) + value + currentDraft.text.slice(end);
        patch({ text, mentions: pruneMentions(text, currentDraft.mentions ?? []) });
        setCaret(start + value.length);
        requestAnimationFrame(() => {
            input.current?.focus();
            input.current?.setSelectionRange(start + value.length, start + value.length);
        });
    };
    const pick = async (type: 'image' | 'file') => {
        setError('');
        setPendingFiles((count) => count + 1);
        try {
            const file = await pickFile();
            if (!file) return;
            if (current().attachments.length >= 8) throw new Error('一次最多添加 8 个附件');
            if (type === 'image' && !/.(png|jpe?g|gif|webp|bmp)$/i.test(file.name))
                throw new Error('请选择 PNG、JPG、GIF、WebP 或 BMP 图片');
            patch({
                attachments: [
                    ...current().attachments,
                    { ...file, key: crypto.randomUUID(), type },
                ],
            });
        } catch (e) {
            setError(errorText(e));
        } finally {
            setPendingFiles((count) => count - 1);
        }
    };
    const addImage = async (file: globalThis.File) => {
        setPendingFiles((count) => count + 1);
        try {
            if (file.size > 10 * 1024 * 1024) throw new Error('粘贴图片不能超过 10 MB');
            const data = await new Promise<string>((resolve, reject) => {
                const reader = new FileReader();
                reader.onload = () => resolve(String(reader.result));
                reader.onerror = () => reject(new Error('读取图片失败'));
                reader.readAsDataURL(file);
            });
            if (current().attachments.length >= 8) throw new Error('一次最多添加 8 个附件');
            patch({
                attachments: [
                    ...current().attachments,
                    {
                        key: crypto.randomUUID(),
                        path: `base64://${data.slice(data.indexOf(',') + 1)}`,
                        name: file.name || '粘贴的图片.png',
                        type: 'image',
                    },
                ],
            });
        } catch (e) {
            setError(errorText(e));
        } finally {
            setPendingFiles((count) => count - 1);
        }
    };
    const paste = (event: ClipboardEvent) => {
        const images = Array.from(event.clipboardData.files).filter((f) =>
            f.type.startsWith('image/'),
        );
        if (images.length) {
            event.preventDefault();
            for (const file of images) void addImage(file);
        }
    };
    const drop = (event: DragEvent) => {
        event.preventDefault();
        dragDepth.current = 0;
        setDragging(false);
        setError('');
        const files = Array.from(event.dataTransfer.files);
        const images = files.filter((f) => f.type.startsWith('image/'));
        for (const file of images) void addImage(file);
        if (images.length !== files.length) setError('其他文件请用附件按钮选择');
    };
    const sending = snapshot.account.messages.some(
        (m) => m.session === contact.key && m.status === 'sending',
    );
    const canSend =
        !collapsed &&
        !disabledReason &&
        !sending &&
        !pendingFiles &&
        (!!draft.text.trim() || draft.attachments.length > 0);
    useChatComposerMotion(composer, canSend, error);
    const send = () => {
        if (!canSend) return;
        setEmoji(false);
        setError('');
        void store.send(contact.key).catch((e) => setError(errorText(e)));
        // 点击发送也回到编辑框，下一句话可以接着输入。
        input.current?.focus();
    };
    const chooseMember = (member: { id: string; name: string }) => {
        if (!query) return;
        const value = current();
        const label = mentionLabel(
            member.name,
            member.id,
            value.mentions ?? [],
            members.filter((m) => m.name === member.name).length,
        );
        patch({
            text: value.text.slice(0, query.start) + label + ' ' + value.text.slice(caret),
            mentions: [...(value.mentions ?? []), { qq: member.id, label }],
        });
        const nextCaret = query.start + label.length + 1;
        setCaret(nextCaret);
        dismissMention(true);
        requestAnimationFrame(() => {
            input.current?.focus();
            input.current?.setSelectionRange(nextCaret, nextCaret);
        });
    };
    const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
        if (collapsed) return;
        if (event.nativeEvent.isComposing || composing.current || event.keyCode === 229) return;
        if (event.key === 'Escape') {
            dismissMention(true);
            setEmoji(false);
            return;
        }
        if (memberVisible && query) {
            if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
                event.preventDefault();
                if (filteredMembers.length)
                    setMemberIndex(
                        (selectedMember +
                            (event.key === 'ArrowDown' ? 1 : -1) +
                            filteredMembers.length) %
                            filteredMembers.length,
                    );
                return;
            }
            if ((event.key === 'Enter' || event.key === 'Tab') && !event.shiftKey) {
                event.preventDefault();
                if (filteredMembers[selectedMember]) chooseMember(filteredMembers[selectedMember]);
                else if (event.key === 'Tab') dismissMention(true);
                return;
            }
        }
        if (
            event.key === 'Enter' &&
            !event.shiftKey &&
            !event.altKey &&
            !event.ctrlKey &&
            !event.metaKey
        ) {
            event.preventDefault();
            send();
        }
    };
    return (
        <div
            ref={composer}
            className="native-chat-composer"
            data-collapsed={collapsed}
            aria-hidden={collapsed || undefined}
            {...(collapsed ? { inert: '' } : {})}
            onPaste={paste}
            onDragOver={(e) => e.preventDefault()}
            onDrop={drop}
            onDragEnter={(e) => {
                if (Array.from(e.dataTransfer.types).includes('Files')) {
                    dragDepth.current++;
                    setDragging(true);
                }
            }}
            onDragLeave={() => {
                dragDepth.current = Math.max(0, dragDepth.current - 1);
                if (!dragDepth.current) setDragging(false);
            }}
        >
            <button {...resize} />
            <div className="native-chat-editor" data-dragging={dragging}>
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
                                patch({ reply: null });
                                input.current?.focus();
                            }}
                        >
                            <X size={13} />
                        </Button>
                    </div>
                </ChatPresence>
                <ChatAttachmentStrip
                    attachments={draft.attachments}
                    onRemove={(key) =>
                        patch({
                            attachments: current().attachments.filter((file) => file.key !== key),
                        })
                    }
                />
                <div className="native-chat-composer-tools">
                    <Popover
                        open={emoji && !collapsed}
                        onOpenChange={(open) => {
                            if (open) restoreAfterEmoji.current = false;
                            setEmoji(open);
                        }}
                    >
                        <PopoverTrigger asChild>
                            <Button
                                variant="ghost"
                                size="icon"
                                className="native-chat-icon"
                                aria-label="表情"
                                title="表情"
                            >
                                <Smile size={18} />
                            </Button>
                        </PopoverTrigger>
                        <PopoverContent
                            side="top"
                            align="start"
                            className="native-chat-popover w-auto p-2"
                            aria-label="选择表情"
                            onCloseAutoFocus={(e) => {
                                if (restoreAfterEmoji.current) {
                                    e.preventDefault();
                                    input.current?.focus();
                                }
                            }}
                        >
                            <ChatEmojiPicker
                                target={store.target}
                                onSelect={chooseEmoji}
                                disabledReason={disabledReason}
                            />
                        </PopoverContent>
                    </Popover>
                    <Button
                        variant="ghost"
                        size="icon"
                        className="native-chat-icon"
                        aria-label="添加图片"
                        title="添加图片"
                        disabled={!!pendingFiles}
                        onClick={() => void pick('image')}
                    >
                        <ActionMotionIcon
                            icon={pendingFiles ? Loader2 : ImagePlus}
                            motion={pendingFiles ? 'spin' : 'none'}
                            size={18}
                        />
                    </Button>
                    <Button
                        variant="ghost"
                        size="icon"
                        className="native-chat-icon"
                        aria-label="添加文件"
                        title="添加文件"
                        disabled={!!pendingFiles}
                        onClick={() => void pick('file')}
                    >
                        <Paperclip size={18} />
                    </Button>
                    {contact.type === 'group' && (
                        <Button
                            variant="ghost"
                            size="icon"
                            className="native-chat-icon"
                            aria-label="提及成员"
                            title="提及成员"
                            onClick={() => {
                                dismissMention(false);
                                insert('@');
                            }}
                        >
                            <AtSign size={18} />
                        </Button>
                    )}
                </div>
                <Popover
                    open={memberVisible}
                    onOpenChange={(open) => {
                        if (!open) dismissMention(true);
                    }}
                >
                    <PopoverAnchor asChild>
                        <div>
                            <textarea
                                ref={input}
                                aria-label={`发送消息给${contact.name}`}
                                placeholder="写点什么…"
                                value={draft.text}
                                rows={2}
                                aria-autocomplete={contact.type === 'group' ? 'list' : undefined}
                                aria-controls={memberVisible ? memberListId : undefined}
                                aria-activedescendant={
                                    memberVisible && filteredMembers[selectedMember]
                                        ? `${memberListId}-${filteredMembers[selectedMember].id}`
                                        : undefined
                                }
                                onChange={(e) => {
                                    patch({
                                        text: e.target.value,
                                        mentions: pruneMentions(
                                            e.target.value,
                                            draft.mentions ?? [],
                                        ),
                                    });
                                    setCaret(e.target.selectionStart);
                                    dismissMention(false);
                                }}
                                onSelect={(e) => setCaret(e.currentTarget.selectionStart)}
                                onCompositionStart={() => {
                                    composing.current = true;
                                }}
                                onCompositionEnd={() => {
                                    composing.current = false;
                                }}
                                onKeyDown={onKeyDown}
                            />
                        </div>
                    </PopoverAnchor>
                    <PopoverContent
                        side="top"
                        align="start"
                        className="native-chat-popover native-chat-mentions"
                        onOpenAutoFocus={(e) => e.preventDefault()}
                        onCloseAutoFocus={(e) => e.preventDefault()}
                        onInteractOutside={(e) => {
                            if (e.detail.originalEvent.target === input.current) e.preventDefault();
                        }}
                        onEscapeKeyDown={() => dismissMention(true)}
                    >
                        <div
                            ref={memberList}
                            id={memberListId}
                            role="listbox"
                            aria-label="群成员建议"
                        >
                            {memberLoading ? (
                                <p role="status">正在读取群成员…</p>
                            ) : memberError ? (
                                <>
                                    <p role="status">{memberError}</p>
                                    <button onClick={() => retryMembers((n) => n + 1)}>
                                        重试读取成员
                                    </button>
                                </>
                            ) : !filteredMembers.length ? (
                                <p role="status">没有匹配的成员</p>
                            ) : (
                                filteredMembers.map((member, index) => (
                                    <button
                                        key={member.id}
                                        id={`${memberListId}-${member.id}`}
                                        role="option"
                                        aria-selected={index === selectedMember}
                                        tabIndex={-1}
                                        onMouseDown={(e) => e.preventDefault()}
                                        onClick={() => chooseMember(member)}
                                    >
                                        <ChatAvatar
                                            contact={{
                                                type: 'private',
                                                id: member.id,
                                                name: member.name || member.id,
                                            }}
                                            small
                                        />
                                        <span className="min-w-0 flex-1 truncate">
                                            {member.name || member.id}
                                        </span>
                                        <span>{member.id}</span>
                                    </button>
                                ))
                            )}
                        </div>
                    </PopoverContent>
                </Popover>
                <ChatPresence visible={dragging}>
                    <div className="native-chat-drop-hint">松开添加图片</div>
                </ChatPresence>
                <footer>
                    <span role="status" className={error ? 'text-danger' : 'text-text-tertiary'}>
                        {error || (pendingFiles ? '正在读取附件…' : disabledReason)}
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
}

function ChatAttachmentStrip({
    attachments,
    onRemove,
}: {
    attachments: Attachment[];
    onRemove: (key: string) => void;
}) {
    const [retained, setRetained] = useState(attachments);
    const currentKeys = useMemo(() => new Set(attachments.map((file) => file.key)), [attachments]);
    const latestKeys = useRef(currentKeys);
    latestKeys.current = currentKeys;
    const displayed = useMemo(
        () => [...attachments, ...retained.filter((file) => !currentKeys.has(file.key))],
        [attachments, retained, currentKeys],
    );
    useEffect(() => {
        if (!attachments.length) return;
        setRetained((previous) => {
            const files = new Map(previous.map((file) => [file.key, file]));
            for (const file of attachments) files.set(file.key, file);
            return [...files.values()];
        });
    }, [attachments]);
    if (!displayed.length) return null;
    return (
        <div className="native-chat-attachments" role="list" aria-label="待发送附件">
            {displayed.map((file) => (
                <ChatPresence
                    key={file.key}
                    visible={currentKeys.has(file.key)}
                    onExited={() => {
                        if (!latestKeys.current.has(file.key))
                            setRetained((previous) =>
                                previous.filter((item) => item.key !== file.key),
                            );
                    }}
                >
                    <div className="native-chat-attachment" data-type={file.type} role="listitem">
                        <div className="native-chat-attachment-preview">
                            <AttachmentPreview attachment={file} />
                        </div>
                        <span className="native-chat-attachment-name" title={file.name}>
                            {file.name}
                        </span>
                        <Button
                            variant="ghost"
                            size="icon"
                            className="native-chat-attachment-remove"
                            aria-label={`移除${file.name}`}
                            title={`移除${file.name}`}
                            onClick={() => onRemove(file.key)}
                        >
                            <X size={12} />
                        </Button>
                    </div>
                </ChatPresence>
            ))}
        </div>
    );
}

function AttachmentPreview({ attachment }: { attachment: Attachment }) {
    const [failed, setFailed] = useState('');
    if (attachment.type === 'face') return <QQFace id={attachment.id} size={40} />;
    if (attachment.type === 'file') return <File size={28} strokeWidth={1.5} />;
    const source = attachment.path.startsWith('base64://')
        ? 'data:image/png;base64,' + attachment.path.slice(9)
        : /^https?:\/\//i.test(attachment.path)
          ? attachment.path
          : '';
    return source && attachment.path !== failed ? (
        <img
            src={source}
            alt={attachment.name}
            referrerPolicy="no-referrer"
            decoding="async"
            draggable={false}
            onError={() => setFailed(attachment.path)}
        />
    ) : (
        <ImagePlus size={28} strokeWidth={1.5} />
    );
}
