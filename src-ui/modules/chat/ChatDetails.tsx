// 资料使用主窗口的轻量弹层，不占用第三栏。
import { useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import {
    Check,
    ChevronRight,
    Copy,
    Info,
    MessageCircle,
    Pin,
    RefreshCw,
    Search,
    Users,
    X,
} from 'lucide-react';
import type { Contact, Conversation } from '../../core/domain/chat/model';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import { useChatProfile } from '../../hooks/chat/useChatProfile';
import { Popover, PopoverClose, PopoverContent, PopoverTrigger } from '../../shared/ui/Popover';
import { ChatAvatar } from '../../shared/chat/ChatAvatar';
import { useMotion } from '../../hooks/preferences/useMotion';

interface Props {
    contact: Conversation;
    target?: DebugTarget;
    onPin: () => void;
    onMessage?: (contact: Contact) => void;
    onSearch?: () => void;
    onMembers?: () => void;
    open?: boolean;
    onOpenChange?: (open: boolean) => void;
}
export function ChatDetails({
    contact,
    target,
    onPin,
    onMessage,
    onSearch,
    onMembers,
    open,
    onOpenChange,
}: Props) {
    const [copyState, setCopyState] = useState('');
    const [internalOpen, setInternalOpen] = useState(false);
    const leaving = useRef(false);
    const visible = open ?? internalOpen;
    const {
        profile,
        isLoading,
        error: profileError,
        refresh,
    } = useChatProfile(target, contact, visible);
    // 原实现每次重取开始时清空 error;react-query 会保留上次 error,重取窗口内同样不展示。
    const [refreshing, setRefreshing] = useState(false);
    const reload = () => {
        setRefreshing(true);
        void refresh().finally(() => setRefreshing(false));
    };
    const error = profileError
        ? profileError instanceof Error
            ? profileError.message
            : '资料读取失败'
        : '';
    const loading = refreshing || isLoading;

    const change = (next: boolean) => {
        setCopyState('');
        setInternalOpen(next);
        onOpenChange?.(next);
    };
    const leave = (action: () => void) => {
        leaving.current = true;
        change(false);
        action();
    };
    const displayed = { ...contact, name: profile?.name || contact.name };
    const copy = async () => {
        try {
            await navigator.clipboard.writeText(displayed.id);
            setCopyState('已复制');
        } catch {
            setCopyState('复制失败');
        }
    };
    return (
        <Popover open={visible} onOpenChange={change}>
            <PopoverTrigger asChild>
                <button
                    type="button"
                    className="native-chat-icon"
                    aria-label="会话资料"
                    title="会话资料"
                >
                    <Info size={17} />
                </button>
            </PopoverTrigger>
            <PopoverContent
                align="end"
                className="native-chat-popover native-chat-details"
                aria-label="会话资料"
                onCloseAutoFocus={(event) => {
                    if (leaving.current) {
                        event.preventDefault();
                        leaving.current = false;
                    }
                }}
            >
                <div className="native-chat-details-heading">
                    <h3>{contact.type === 'group' ? '群资料' : '个人资料'}</h3>
                    {target && (
                        <button
                            type="button"
                            className="native-chat-icon"
                            aria-label="刷新会话资料"
                            disabled={loading}
                            onClick={reload}
                        >
                            <RefreshCw size={13} />
                        </button>
                    )}
                    <PopoverClose asChild>
                        <button
                            type="button"
                            className="native-chat-icon"
                            aria-label="关闭会话资料"
                        >
                            <X size={15} />
                        </button>
                    </PopoverClose>
                </div>
                <ProfileBody step={contact.key}>
                    <div className="native-chat-details-person">
                        <ChatAvatar contact={displayed} />
                        <div className="min-w-0">
                            <p className="break-words text-[14px] font-semibold">
                                {displayed.name}
                            </p>
                            <p className="mt-1 text-[11px] text-text-tertiary">
                                {contact.type === 'group' ? '群聊' : 'QQ 用户'}
                            </p>
                        </div>
                    </div>
                    <button
                        type="button"
                        className="native-chat-detail-action"
                        onClick={() => void copy()}
                        aria-label={`复制${displayed.type === 'group' ? '群号' : 'QQ 号'}`}
                    >
                        <span className="text-text-tertiary">
                            {displayed.type === 'group' ? '群号' : 'QQ'}
                        </span>
                        <span className="ml-auto font-mono text-[11px]">{displayed.id}</span>
                        {copyState === '已复制' ? <Check size={13} /> : <Copy size={13} />}
                    </button>
                    <dl className="native-chat-profile-fields">
                        {(profile?.fields ?? []).map((field) => (
                            <div key={field.label}>
                                <dt>{field.label}</dt>
                                <dd>{field.value}</dd>
                            </div>
                        ))}
                    </dl>
                    {loading && !profile && (
                        <p className="native-chat-profile-status" role="status">
                            正在读取资料…
                        </p>
                    )}
                    {error && (
                        <p className="native-chat-profile-status" role="status">
                            {error}
                            <button type="button" onClick={reload}>
                                重试
                            </button>
                        </p>
                    )}
                    {contact.type === 'private' && onMessage && (
                        <button
                            type="button"
                            className="native-chat-detail-action"
                            onClick={() => leave(() => onMessage(contact))}
                        >
                            <MessageCircle size={14} />
                            发消息
                            <ChevronRight size={13} className="ml-auto" />
                        </button>
                    )}
                    <button
                        type="button"
                        className="native-chat-detail-action"
                        aria-pressed={contact.pinned}
                        onClick={onPin}
                    >
                        <Pin size={14} />
                        置顶会话
                        {contact.pinned && <Check size={14} className="ml-auto text-brand" />}
                    </button>
                    {onSearch && (
                        <button
                            type="button"
                            className="native-chat-detail-action"
                            onClick={() => leave(onSearch)}
                        >
                            <Search size={14} />
                            查找聊天记录
                            <ChevronRight size={13} className="ml-auto" />
                        </button>
                    )}
                    {contact.type === 'group' && onMembers && (
                        <button
                            type="button"
                            className="native-chat-detail-action"
                            onClick={() => leave(onMembers)}
                        >
                            <Users size={14} />
                            查看群成员
                            <ChevronRight size={13} className="ml-auto" />
                        </button>
                    )}
                </ProfileBody>
                {copyState && (
                    <p role="status" className="pt-2 text-[11px] text-text-tertiary">
                        {copyState}
                    </p>
                )}
            </PopoverContent>
        </Popover>
    );
}

function ProfileBody({ step, children }: { step: string; children: ReactNode }) {
    const scroll = useRef<HTMLDivElement>(null);
    const content = useRef<HTMLDivElement>(null);
    const [height, setHeight] = useState<number>();
    const motion = useMotion();
    useLayoutEffect(() => {
        const element = content.current;
        if (!element) return;
        const measure = () => {
            const ceiling = scroll.current
                ? parseFloat(getComputedStyle(scroll.current).maxHeight)
                : Infinity;
            setHeight(
                Math.min(element.scrollHeight, Number.isFinite(ceiling) ? ceiling : Infinity),
            );
        };
        measure();
        const observer = new ResizeObserver(measure);
        observer.observe(element);
        return () => observer.disconnect();
    }, [step]);
    useLayoutEffect(() => {
        if (scroll.current) scroll.current.scrollTop = 0;
    }, [step]);
    return (
        <div
            ref={scroll}
            className="native-chat-details-body"
            data-motion={motion.enabled}
            style={{ height }}
        >
            <div ref={content} key={step} className="native-chat-details-content">
                {children}
            </div>
        </div>
    );
}
