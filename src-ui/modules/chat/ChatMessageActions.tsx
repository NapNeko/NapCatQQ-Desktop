import {
    useEffect,
    useLayoutEffect,
    useRef,
    useState,
    type KeyboardEvent,
    type ReactElement,
    type ReactNode,
} from 'react';
import {
    AtSign,
    Check,
    Copy,
    Forward,
    ListChecks,
    Reply as ReplyIcon,
    RotateCcw,
    SmilePlus,
    Undo2,
} from 'lucide-react';
import { useGSAP } from '@gsap/react';
import { useMotion } from '../../hooks/preferences/useMotion';
import { globalInfoBarStore } from '../../hooks/ui/globalInfoBarStore';
import { Button } from '../../shared/ui/Button';
import { accountKey, EMPTY_DRAFT, type Contact, type Message } from '../../core/domain/chat/model';
import { recoverDraft } from '../../core/domain/chat/recoverDraft';
import { messagePreview } from '../../core/domain/debug/segments';
import type { ChatAccountStore } from '../../hooks/chat/chatStore';
import {
    ContextMenu,
    ContextMenuContent,
    ContextMenuItem,
    ContextMenuSub,
    ContextMenuSubContent,
    ContextMenuSubTrigger,
    ContextMenuTrigger,
} from '../../shared/ui/ContextMenu';
import { draftWithMention } from './messageActions';
import { errorText } from '../../core/domain/errors';
import {
    canForwardMessage,
    canRecallMessage,
    canRepeatMessage,
} from '../../core/domain/chat/messageTransfer';
import {
    favoriteStickerService,
    FavoriteStickerError,
} from '../../core/services/favorite-sticker.service';

interface Props {
    store: ChatAccountStore;
    contact: Contact;
    message: Message;
    onFocusComposer?: () => void;
    onError: (error: string) => void;
    onForward?: (messageKey: string) => void;
    onSelect?: (messageKey: string) => void;
    selecting?: boolean;
    selected?: boolean;
    selectionDisabled?: boolean;
    onToggleSelection?: (messageKey: string) => void;
    children: (controls: ReactNode, wrapContent: (content: ReactNode) => ReactNode) => ReactElement;
}

function MessageSelection({
    selected,
    disabled,
    onToggle,
    children,
}: {
    selected: boolean;
    disabled: boolean;
    onToggle: () => void;
    children: ReactNode;
}) {
    const block = (event: { preventDefault: () => void; stopPropagation: () => void }) => {
        event.preventDefault();
        event.stopPropagation();
    };
    return (
        <div
            className="native-chat-selection-target"
            role="button"
            tabIndex={disabled ? -1 : 0}
            aria-label={selected ? '取消选择这条消息' : '选择这条消息'}
            aria-pressed={selected}
            aria-disabled={disabled}
            onPointerDownCapture={block}
            onPointerUpCapture={block}
            onMouseDownCapture={block}
            onMouseUpCapture={block}
            onContextMenuCapture={block}
            onDragStartCapture={block}
            onClickCapture={(event) => {
                block(event);
                event.currentTarget.focus({ preventScroll: true });
                if (!disabled) onToggle();
            }}
            onKeyDownCapture={(event) => {
                if (event.key !== 'Enter' && event.key !== ' ') return;
                block(event);
                if (!disabled && !event.repeat && !event.nativeEvent.isComposing) onToggle();
            }}
            onKeyUpCapture={(event) => {
                if (event.key === 'Enter' || event.key === ' ') block(event);
            }}
        >
            {/* inert 让媒体原控件离开 Tab 序列；外层捕获直接接住重定向后的点击。 */}
            <div className="native-chat-selection-content" {...{ inert: '' }}>
                {children}
            </div>
        </div>
    );
}

/** Both the context menu and visible controls use the same draft-preserving actions. */
export function ChatMessageActions({
    store,
    contact,
    message,
    onFocusComposer,
    onError,
    onForward,
    onSelect,
    selecting = false,
    selected = false,
    selectionDisabled = false,
    onToggleSelection,
    children,
}: Props) {
    const [copied, setCopied] = useState(false);
    const [recalling, setRecalling] = useState(false);
    const recallInFlight = useRef(false);
    const [repeating, setRepeating] = useState(false);
    const repeatInFlight = useRef(false);
    const [favoriteStates, setFavoriteStates] = useState<
        Record<number, 'saving' | 'saved' | 'unknown'>
    >({});
    const favoriteInFlight = useRef(false);
    const restoreComposerFocus = useRef(false);
    const copyIcon = useRef<HTMLSpanElement>(null);
    const controlsRef = useRef<HTMLDivElement>(null);
    const [verticalControls, setVerticalControls] = useState(false);
    const hasMedia = message.segments.some((segment) =>
        ['image', 'video', 'mface'].includes(segment.type),
    );
    useLayoutEffect(() => {
        if (selecting || !hasMedia) {
            setVerticalControls(false);
            return;
        }
        const bubble = controlsRef.current?.parentElement?.querySelector('.native-chat-bubble');
        if (!bubble) return;
        const measure = () => setVerticalControls(bubble.getBoundingClientRect().height >= 96);
        measure();
        const observer = new ResizeObserver(measure);
        observer.observe(bubble);
        return () => observer.disconnect();
    }, [hasMedia, selecting, message.key]);
    const previousCopied = useRef(false);
    const motion = useMotion();
    useGSAP(
        () => {
            const changed = copied && !previousCopied.current;
            previousCopied.current = copied;
            if (changed && copyIcon.current) motion.pop(copyIcon.current);
        },
        {
            scope: copyIcon,
            dependencies: [copied, motion.enabled, motion.level, motion.speed],
            revertOnUpdate: true,
        },
    );
    useEffect(() => {
        if (!copied) return;
        const timer = setTimeout(() => setCopied(false), 1800);
        return () => clearTimeout(timer);
    }, [copied]);
    const currentDraft = () => store.getSnapshot().account.drafts[contact.key] ?? EMPTY_DRAFT;
    const reply = () => {
        if (!message.id) return;
        store.draft(contact.key, {
            ...currentDraft(),
            reply: {
                id: message.id,
                name: message.mine ? '我' : message.senderName,
                preview: messagePreview(message.segments),
            },
        });
        onFocusComposer?.();
    };
    const mention = () => {
        const snapshot = store.getSnapshot();
        store.draft(
            contact.key,
            draftWithMention(
                snapshot.account.messages,
                snapshot.account.drafts,
                contact.key,
                message,
            ),
        );
        onFocusComposer?.();
    };
    const recover = () => {
        store.draft(contact.key, recoverDraft(message, currentDraft()));
        onFocusComposer?.();
    };
    const retry = () => {
        void store.retry(message.key).catch((error) => onError(errorText(error)));
    };
    const isForward = message.segments.some((segment) => segment.type === 'forward');
    const canRetry = message.mine && message.status === 'failed' && !isForward;
    const recall = async () => {
        if (recallInFlight.current) return;
        recallInFlight.current = true;
        setRecalling(true);
        onError('');
        try {
            await store.recall(message.key);
        } catch (error) {
            onError(errorText(error));
        } finally {
            recallInFlight.current = false;
            setRecalling(false);
        }
    };
    const actionDisconnected =
        !store.target.running ||
        store.target.online === false ||
        store.getSnapshot().connection.state !== 'connected';
    const retryDisabled =
        !store.target.running ||
        store.target.online === false ||
        store.getSnapshot().connection.state !== 'connected' ||
        store
            .getSnapshot()
            .account.messages.some(
                (item) => item.session === contact.key && item.status === 'sending',
            );
    const repeat = async () => {
        if (repeatInFlight.current) return;
        repeatInFlight.current = true;
        setRepeating(true);
        onError('');
        try {
            await store.repeat(message.key);
        } catch (error) {
            onError(errorText(error));
        } finally {
            repeatInFlight.current = false;
            setRepeating(false);
        }
    };
    const stickers = message.segments.flatMap((segment, index) =>
        segment.type === 'image' || segment.type === 'mface' ? [{ segment, index }] : [],
    );
    const addSticker = async (index: number) => {
        if (favoriteInFlight.current || favoriteStates[index]) return;
        const segment = message.segments[index];
        if (!segment) return;
        favoriteInFlight.current = true;
        const target = { ...store.target };
        const scope = accountKey(target.bot_id, String(target.qq_id));
        const current = () => scope === accountKey(store.target.bot_id, String(store.target.qq_id));
        setFavoriteStates((states) => ({ ...states, [index]: 'saving' }));
        onError('');
        try {
            await favoriteStickerService.add(target, segment, {
                messageId: message.id,
                imageIndex: message.segments
                    .slice(0, index)
                    .filter((item) => item.type === 'image' || item.type === 'mface').length,
            });
            if (current()) {
                setFavoriteStates((states) => ({ ...states, [index]: 'saved' }));
                globalInfoBarStore.push({
                    key: `chat:${scope}:favorite:${message.key}:${index}`,
                    tone: 'success',
                    title: `${target.name} · 收藏表情`,
                    content: '已添加到收藏表情',
                });
            }
        } catch (error) {
            if (current()) {
                setFavoriteStates((states) => {
                    const next = { ...states };
                    if (error instanceof FavoriteStickerError && error.uncertain)
                        next[index] = 'unknown';
                    else delete next[index];
                    return next;
                });
                onError(errorText(error));
            }
        } finally {
            favoriteInFlight.current = false;
        }
    };
    const favoriteLabel = (index: number) =>
        favoriteStates[index] === 'saving'
            ? '正在添加…'
            : favoriteStates[index] === 'saved'
              ? '已添加到表情'
              : favoriteStates[index] === 'unknown'
                ? '添加结果待确认'
                : '添加到表情';
    const favoriteDisabled = (index: number) =>
        actionDisconnected ||
        !!favoriteStates[index] ||
        Object.values(favoriteStates).includes('saving');
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
        event.currentTarget.dispatchEvent(
            new MouseEvent('contextmenu', {
                bubbles: true,
                cancelable: true,
                clientX: bounds.left + Math.min(24, bounds.width / 2),
                clientY: bounds.top + Math.min(24, bounds.height / 2),
            }),
        );
    };
    const canMention = contact.type === 'group' && !message.mine && !!message.senderId;
    const controls =
        message.recalled || selecting ? null : (
            <div
                ref={controlsRef}
                className="native-chat-message-actions"
                data-layout={verticalControls ? 'vertical' : 'horizontal'}
                role="toolbar"
                aria-label="消息快捷操作"
                style={copied ? { opacity: 1 } : undefined}
            >
                {message.id && (
                    <Button
                        variant="ghost"
                        size="icon"
                        className="native-chat-icon"
                        aria-label={`回复${message.senderName}的消息`}
                        title="回复"
                        onClick={reply}
                    >
                        <ReplyIcon size={14} />
                    </Button>
                )}
                <Button
                    variant="ghost"
                    size="icon"
                    className="native-chat-icon"
                    aria-label={copied ? '消息已复制' : '复制消息'}
                    title={copied ? '已复制' : '复制'}
                    onClick={() => void copy()}
                >
                    <span ref={copyIcon} className="inline-flex">
                        {copied ? <Check size={13} /> : <Copy size={13} />}
                    </span>
                </Button>
                {canRepeatMessage(message) && (
                    <Button
                        variant="ghost"
                        size="icon"
                        className="native-chat-icon native-chat-repeat"
                        aria-label="重复发送这条消息"
                        title="+1"
                        disabled={retryDisabled || repeating}
                        onClick={() => void repeat()}
                    >
                        <span aria-hidden>+1</span>
                    </Button>
                )}
                {message.status === 'failed' && !isForward && (
                    <Button
                        variant="ghost"
                        size="icon"
                        className="native-chat-icon"
                        aria-label="将失败消息放回输入框"
                        title="放回输入框"
                        onClick={recover}
                    >
                        <RotateCcw size={13} />
                    </Button>
                )}
                <span className="sr-only" role="status" aria-live="polite">
                    {copied ? '已复制消息' : ''}
                </span>
            </div>
        );
    const wrapContent = (content: ReactNode) =>
        selecting ? (
            <MessageSelection
                selected={selected}
                disabled={selectionDisabled || !canForwardMessage(message)}
                onToggle={() => onToggleSelection?.(message.key)}
            >
                {content}
            </MessageSelection>
        ) : (
            content
        );
    if (selecting || message.recalled) return children(null, wrapContent);
    return (
        <ContextMenu
            onOpenChange={(open) => {
                if (open) restoreComposerFocus.current = false;
            }}
        >
            <ContextMenuTrigger asChild tabIndex={0} onKeyDown={openWithKeyboard}>
                {children(controls, wrapContent)}
            </ContextMenuTrigger>
            <ContextMenuContent
                aria-label="消息操作"
                onCloseAutoFocus={(event) => {
                    if (restoreComposerFocus.current && onFocusComposer) {
                        event.preventDefault();
                        restoreComposerFocus.current = false;
                        onFocusComposer();
                    }
                }}
            >
                {message.id && (
                    <ContextMenuItem onSelect={() => selectDraftAction(reply)}>
                        <ReplyIcon size={14} />
                        回复
                    </ContextMenuItem>
                )}
                <ContextMenuItem onSelect={() => void copy()}>
                    <Copy size={14} />
                    复制消息
                </ContextMenuItem>
                {canMention && (
                    <ContextMenuItem onSelect={() => selectDraftAction(mention)}>
                        <AtSign size={14} />
                        提及 {message.senderName}
                    </ContextMenuItem>
                )}
                {canRepeatMessage(message) && (
                    <ContextMenuItem
                        disabled={retryDisabled || repeating}
                        onSelect={() => void repeat()}
                    >
                        <span className="native-chat-repeat-mark" aria-hidden>
                            +1
                        </span>
                        重复发送
                    </ContextMenuItem>
                )}
                {stickers.length === 1 && (
                    <ContextMenuItem
                        disabled={favoriteDisabled(stickers[0].index)}
                        onSelect={() => void addSticker(stickers[0].index)}
                    >
                        <SmilePlus size={14} />
                        {favoriteLabel(stickers[0].index)}
                    </ContextMenuItem>
                )}
                {stickers.length > 1 && (
                    <ContextMenuSub>
                        <ContextMenuSubTrigger className="gap-2">
                            <SmilePlus size={14} />
                            添加到表情
                        </ContextMenuSubTrigger>
                        <ContextMenuSubContent>
                            {stickers.map(({ index }, order) => (
                                <ContextMenuItem
                                    key={index}
                                    disabled={favoriteDisabled(index)}
                                    onSelect={() => void addSticker(index)}
                                >
                                    第 {order + 1} 张 · {favoriteLabel(index)}
                                </ContextMenuItem>
                            ))}
                        </ContextMenuSubContent>
                    </ContextMenuSub>
                )}
                {canForwardMessage(message) && onForward && (
                    <ContextMenuItem onSelect={() => onForward(message.key)}>
                        <Forward size={14} />
                        转发
                    </ContextMenuItem>
                )}
                {canForwardMessage(message) && onSelect && (
                    <ContextMenuItem onSelect={() => onSelect(message.key)}>
                        <ListChecks size={14} />
                        多选
                    </ContextMenuItem>
                )}
                {canRecallMessage(message) && (
                    <ContextMenuItem
                        disabled={actionDisconnected || recalling}
                        onSelect={() => void recall()}
                    >
                        <Undo2 size={14} />
                        {recalling ? '正在撤回…' : '撤回'}
                    </ContextMenuItem>
                )}
                {canRetry && (
                    <ContextMenuItem disabled={retryDisabled} onSelect={retry}>
                        <RotateCcw size={14} />
                        重新发送
                    </ContextMenuItem>
                )}
                {message.status === 'failed' && !isForward && (
                    <ContextMenuItem onSelect={() => selectDraftAction(recover)}>
                        <RotateCcw size={14} />
                        放回输入框
                    </ContextMenuItem>
                )}
            </ContextMenuContent>
        </ContextMenu>
    );
}
