// 一条消息气泡。别人发的在左（头像 + 名字 · 群名 · 时间），Bot 自己发的在右（brand 浅底），
// 自己发的下面写这次调用的结果（成功耗时 / 失败 retcode 和原因）。
//
// 点气泡选中它：输入框就回复它（「全部」视图下也按它定发到哪个会话），再点一次取消。
// 悬停（或键盘聚焦）出一条小工具条：看原始 JSON、回复、复制 id、填入请求。工具条只在悬停时挂上，
// 几十行同时在屏上也不会多出几百个按钮。

import { memo, useCallback, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type RefObject } from 'react';
import { Braces, Check, Copy, FileInput, Reply } from 'lucide-react';
import { cn } from '../../../shared/utils/cn';
import { Popover, PopoverContent, PopoverTrigger } from '../../../shared/ui';
import { messagePreview } from '../../../core/domain/debug/segments';
import { retcodeHint } from '../../../core/domain/debug/errorCopy';
import type { ChatItem } from '../../../core/domain/debug/chat';
import { useChatView } from './chatContext';
import { ROLE_LABEL, callLine, clockTime, idsOfItem, sendActionOf, type MessageItem } from '../../../core/domain/debug/chatFormat';
import { Avatar, IconAction, useCopy } from './rightParts';
import { SegmentList, isPictureOnly } from './SegmentView';

/** 折叠后的最大高度：12 行 × 20px 行高 */
const CLAMP_PX = 12 * 20;

/** 字数或换行多到可能超过 12 行时才去量，免得每条消息挂上都读一次布局 */
function mightOverflow(item: MessageItem): boolean {
    let chars = 0;
    let lines = 0;
    for (const s of item.segments) {
        if (s.type === 'markdown') return true;
        if (s.type !== 'text' || typeof s.data.text !== 'string') continue;
        chars += s.data.text.length;
        for (let i = 0; i < s.data.text.length; i += 1) if (s.data.text.charCodeAt(i) === 10) lines += 1;
    }
    // 右栏最窄时一行十几个字，150 字就可能超过 12 行
    return chars > 150 || lines >= 11;
}

// ---------------------------------------------------------------------------
// 悬停工具条
// ---------------------------------------------------------------------------

/** 行上的悬停 / 聚焦状态；弹出菜单开着时即使指针离开也先别收 */
export function useRowHover() {
    const [hovered, setHovered] = useState(false);
    const [focused, setFocused] = useState(false);
    const [held, setHeld] = useState(false);
    const bind = useMemo(
        () => ({
            onMouseEnter: () => setHovered(true),
            onMouseLeave: () => setHovered(false),
            onFocus: () => setFocused(true),
            onBlur: (e: React.FocusEvent<HTMLElement>) => {
                if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setFocused(false);
            },
        }),
        [],
    );
    return { active: hovered || focused || held, bind, hold: setHeld };
}

export function HoverActions({
    item,
    side,
    anchorRef,
    onHold,
}: {
    item: ChatItem;
    side: 'left' | 'right';
    /** 详情弹层对着谁弹 */
    anchorRef: RefObject<HTMLElement | null>;
    onHold: (held: boolean) => void;
}) {
    const api = useChatView();
    const { copied, copy } = useCopy();
    const [menuOpen, setMenuOpen] = useState(false);
    const [filled, setFilled] = useState<string[] | null>(null);
    // 挂上时算一次能填什么：工具条只在悬停时存在，当前标签在这期间一般不会变
    const plan = useMemo(() => api.previewFill(item), [api, item]);
    const ids = useMemo(() => idsOfItem(item), [item]);
    const message = item.kind === 'message' ? item : null;

    const copyEntries: Array<{ tag: string; label: string; value: string }> = [];
    if (ids.message_id !== undefined) copyEntries.push({ tag: 'mid', label: 'message_id', value: String(ids.message_id) });
    if (ids.user_id !== undefined) copyEntries.push({ tag: 'uid', label: 'user_id', value: String(ids.user_id) });
    if (ids.group_id !== undefined) copyEntries.push({ tag: 'gid', label: 'group_id', value: String(ids.group_id) });
    if (message) {
        const text = messagePreview(message.segments);
        if (text) copyEntries.push({ tag: 'text', label: '文字内容', value: text });
    }

    const setMenu = (open: boolean) => {
        setMenuOpen(open);
        onHold(open);
    };

    return (
        <div
            role="toolbar"
            aria-label="这条消息的操作"
            className={cn(
                'absolute top-0 z-10 flex items-center gap-0.5 rounded-sm border border-border-subtle bg-elevated p-0.5 shadow-popover',
                side === 'right' ? 'right-3' : 'left-3',
            )}
            onClick={(e) => e.stopPropagation()}
        >
            <IconAction
                label="看原始 JSON"
                onClick={() => {
                    const el = anchorRef.current;
                    if (el) api.openDetail(item, el);
                }}
            >
                <Braces size={13} aria-hidden />
            </IconAction>
            {message && message.messageId !== undefined && (
                <IconAction label="回复" tip="回复这条（在下面的输入框里写）" onClick={() => api.reply(message)}>
                    <Reply size={13} aria-hidden />
                </IconAction>
            )}
            {copyEntries.length > 0 && (
                <Popover open={menuOpen} onOpenChange={setMenu}>
                    <PopoverTrigger asChild>
                        <IconAction label="复制" tip={copied ? '已复制' : '复制 id'}>
                            {copied ? <Check size={13} aria-hidden className="text-success" /> : <Copy size={13} aria-hidden />}
                        </IconAction>
                    </PopoverTrigger>
                    <PopoverContent side="bottom" align={side === 'right' ? 'end' : 'start'} className="w-[220px] p-1">
                        {copyEntries.map((e) => (
                            <button
                                key={e.tag}
                                type="button"
                                onClick={() => {
                                    copy(e.value, e.tag);
                                    setMenu(false);
                                }}
                                className="flex w-full items-center gap-2 rounded-xs px-2 py-1.5 text-left text-xs text-text-secondary transition-colors hover:bg-inset hover:text-text"
                            >
                                <span className="shrink-0 font-mono text-[11px] text-text-tertiary">{e.label}</span>
                                <span className="min-w-0 flex-1 truncate text-right font-mono text-[11px]">{e.value}</span>
                            </button>
                        ))}
                    </PopoverContent>
                </Popover>
            )}
            <IconAction
                label="填入请求"
                aria-disabled={!plan.ok}
                tip={
                    filled
                        ? `已填入 ${filled.join('、')}`
                        : plan.ok
                          ? `填进当前请求：${plan.filled.join('、')}`
                          : plan.reason
                }
                className={cn(!plan.ok && 'cursor-not-allowed opacity-45 hover:bg-transparent hover:text-text-tertiary')}
                onClick={() => {
                    if (!plan.ok) return;
                    const res = api.fill(item);
                    if (res.ok) setFilled(res.filled);
                }}
            >
                {filled ? <Check size={13} aria-hidden className="text-success" /> : <FileInput size={13} aria-hidden />}
            </IconAction>
        </div>
    );
}

// ---------------------------------------------------------------------------
// 气泡
// ---------------------------------------------------------------------------

export interface MessageBubbleProps {
    item: MessageItem;
    /** 同一人紧接着发的：不再画头像和名字 */
    continued: boolean;
    selected: boolean;
    /** 「全部」视图：名字后面带上群名 / 私聊对象 */
    showSessionName: boolean;
}

export const MessageBubble = memo(function MessageBubble({ item, continued, selected, showSessionName }: MessageBubbleProps) {
    const api = useChatView();
    const out = item.direction === 'out';
    const rowRef = useRef<HTMLDivElement>(null);
    const bodyRef = useRef<HTMLDivElement>(null);
    const hover = useRowHover();
    const [expanded, setExpandedState] = useState(() => api.isExpanded(item.key));
    const [overflowing, setOverflowing] = useState(false);
    const clampable = useMemo(() => mightOverflow(item), [item]);
    const clamped = clampable && !expanded;

    // 挂上时量一次；栏宽变了（拖分隔条、窗口变窄）换行跟着变，要重新量，「展开」才会该出现时出现
    useLayoutEffect(() => {
        const el = bodyRef.current;
        if (!clamped || !el) return;
        const check = () => setOverflowing(el.scrollHeight > el.clientHeight + 1);
        check();
        if (typeof ResizeObserver === 'undefined') return;
        const ro = new ResizeObserver(check);
        ro.observe(el);
        return () => ro.disconnect();
    }, [clamped, item.segments]);

    const setExpanded = (v: boolean) => {
        api.setExpanded(item.key, v);
        setExpandedState(v);
    };

    const selfId = api.selfId();
    const avatarId = out ? item.senderId || selfId || 0 : item.senderId;
    const name = out && item.senderName === '我' ? (api.nameOf(avatarId) ?? '我') : item.senderName;
    const sessionName = showSessionName ? api.sessionName(item.session) : undefined;
    const role = item.senderRole ? ROLE_LABEL[item.senderRole] : undefined;
    const bare = isPictureOnly(item.segments);
    const failed = item.call?.ok === false;
    const line = item.call ? callLine(item.call) : null;

    const onBubbleClick = useCallback(() => {
        // 拖着选了一段字不算点击
        if (window.getSelection()?.toString()) return;
        api.toggleSelect(item);
    }, [api, item]);

    const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
        if (e.target !== e.currentTarget) return;
        if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            api.toggleSelect(item);
        }
    };

    return (
        <div
            ref={rowRef}
            {...hover.bind}
            role="article"
            tabIndex={0}
            aria-current={selected ? 'true' : undefined}
            aria-label={`${name}：${messagePreview(item.segments) || '（空消息）'}${selected ? '（已选中，输入框在回复它）' : ''}`}
            onKeyDown={onKeyDown}
            className={cn(
                'group/msg relative flex items-start gap-2 px-3 pb-0.5 outline-none',
                continued ? 'pt-0.5' : 'pt-2',
                out && 'flex-row-reverse',
                'focus-visible:bg-inset/40',
            )}
        >
            {continued ? <span aria-hidden className="w-8 shrink-0" /> : <Avatar id={avatarId} name={name} mine={out} />}
            <div className={cn('flex min-w-0 max-w-[82%] flex-col', out ? 'items-end' : 'items-start')}>
                {!continued && (
                    <div
                        className={cn(
                            'mb-0.5 flex min-w-0 max-w-full items-center gap-1 px-0.5 text-2xs text-text-tertiary',
                            out && 'flex-row-reverse',
                        )}
                    >
                        <span className="truncate font-medium text-text-secondary">{name}</span>
                        {role && (
                            <span
                                className={cn(
                                    'shrink-0 rounded-xs px-1 text-[10px] leading-4',
                                    item.senderRole === 'owner' ? 'bg-warning-soft text-warning' : 'bg-info-soft text-info',
                                )}
                            >
                                {role}
                            </span>
                        )}
                        {sessionName && (
                            <>
                                <span aria-hidden>·</span>
                                <span className="truncate">{sessionName}</span>
                            </>
                        )}
                        <span aria-hidden>·</span>
                        <span className="shrink-0 tabular-nums">{clockTime(item.at)}</span>
                    </div>
                )}
                <div
                    onClick={onBubbleClick}
                    title={continued ? clockTime(item.at) : undefined}
                    className={cn(
                        'relative min-w-0 max-w-full cursor-pointer text-[13px] leading-5 text-text transition-shadow duration-150',
                        bare
                            ? 'rounded-md'
                            : cn(
                                  'px-2.5 py-1.5',
                                  out
                                      ? 'rounded-[12px_4px_12px_12px] border border-brand/15 bg-brand-soft'
                                      : 'rounded-[4px_12px_12px_12px] border border-border-subtle bg-surface',
                                  failed && 'border-danger/40',
                              ),
                        selected && 'ring-2 ring-brand/50 ring-offset-1 ring-offset-canvas',
                    )}
                >
                    <div
                        ref={bodyRef}
                        className={cn('relative min-w-0', clamped && 'overflow-hidden')}
                        style={clamped ? { maxHeight: CLAMP_PX } : undefined}
                    >
                        <SegmentList segments={item.segments} mine={out} messageId={item.messageId !== undefined ? String(item.messageId) : undefined} />
                        {clamped && overflowing && (
                            <span
                                aria-hidden
                                className={cn(
                                    'pointer-events-none absolute inset-x-0 bottom-0 h-8 bg-gradient-to-t to-transparent',
                                    out ? 'from-brand-soft' : 'from-surface',
                                )}
                            />
                        )}
                    </div>
                    {clampable && (overflowing || expanded) && (
                        <button
                            type="button"
                            onClick={(e) => {
                                e.stopPropagation();
                                setExpanded(!expanded);
                            }}
                            className="mt-0.5 text-2xs font-medium text-info hover:underline"
                        >
                            {expanded ? '收起' : '展开'}
                        </button>
                    )}
                </div>
                {line && (
                    <div
                        title={failed && item.call?.retcode ? (retcodeHint(item.call.retcode) ?? undefined) : undefined}
                        className={cn(
                            'mt-0.5 max-w-full truncate px-0.5 font-mono text-[10.5px]',
                            line.ok ? 'text-success' : 'text-danger',
                        )}
                    >
                        ↗ {sendActionOf(item)} · {line.text}
                    </div>
                )}
            </div>
            {hover.active && <HoverActions item={item} side={out ? 'left' : 'right'} anchorRef={rowRef} onHold={hover.hold} />}
        </div>
    );
});
