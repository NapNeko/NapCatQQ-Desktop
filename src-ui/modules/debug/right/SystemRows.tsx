// 时间线里不是聊天消息的那些行：通知（居中灰条）、请求（卡片）、和消息无关的调用（虚线小标签）、
// 元事件、接收器状态、断线缺口、上游丢弃、缓冲裁掉的提示、时间分隔线。

import { memo, useRef, useState } from 'react';
import { Braces, Check, Copy, FileInput, TriangleAlert, UserPlus, Users, X } from 'lucide-react';
import { cn } from '../../../shared/utils/cn';
import { Button, Spinner } from '../../../shared/ui';
import { receiverStateCopy } from '../../../core/domain/debug/receiverCopy';
import type { ChatItem } from '../../../core/domain/debug/chat';
import {
    rejectLine,
    handleRequestCall,
    requestLine,
} from '../../../core/domain/debug/requestHandling';
import { DangerConfirmDialog, dangerConfirmSkipped } from '../DangerConfirmDialog';
import { useChatView } from './chatContext';
import {
    callLine,
    clockTime,
    countFormat,
    dayLabel,
    gapRange,
    originLabel,
} from '../../../core/domain/debug/chatFormat';
import { HoverActions, useRowHover } from './MessageBubble';
import { useCopy } from './rightParts';

type Of<K extends ChatItem['kind']> = Extract<ChatItem, { kind: K }>;

export function TimeSeparator({ at }: { at: number }) {
    return (
        <div
            className="px-3 pb-0.5 pt-2.5 text-center text-2xs tabular-nums text-text-tertiary"
            role="separator"
        >
            {dayLabel(at)}
        </div>
    );
}

export const NoticeRow = memo(function NoticeRow({
    item,
    showSessionName,
}: {
    item: Of<'notice'>;
    showSessionName: boolean;
}) {
    const api = useChatView();
    const ref = useRef<HTMLDivElement>(null);
    const hover = useRowHover();
    const sessionName = showSessionName && item.session ? api.sessionName(item.session) : undefined;
    return (
        <div ref={ref} {...hover.bind} className="relative flex justify-center px-10 py-1">
            <button
                type="button"
                onClick={(e) => api.openDetail(item, e.currentTarget)}
                title={clockTime(item.at)}
                className="max-w-full truncate rounded-pill bg-inset px-3 py-0.5 text-2xs text-text-tertiary transition-colors hover:text-text-secondary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
            >
                {item.text}
                {sessionName ? ` · ${sessionName}` : ''}
            </button>
            {hover.active && (
                <HoverActions item={item} side="right" anchorRef={ref} onHold={hover.hold} />
            )}
        </div>
    );
});

type HandleState =
    | { state: 'idle' }
    | { state: 'sending'; approve: boolean }
    | { state: 'done'; approve: boolean }
    | { state: 'failed'; reason: string };

export const RequestCard = memo(function RequestCard({ item }: { item: Of<'request'> }) {
    const api = useChatView();
    const { copied, copy } = useCopy();
    const ref = useRef<HTMLDivElement>(null);
    // 当前标签随时会变：挂上时算一次，指针移到按钮上时再算一次
    const [plan, setPlan] = useState(() => api.previewFill(item));
    const refreshPlan = () => setPlan(api.previewFill(item));
    const user = api.nameOf(item.userId) ?? String(item.userId);
    const group =
        item.groupId !== undefined
            ? (api.sessionName(`group:${item.groupId}`) ?? String(item.groupId))
            : '';
    const title = requestLine(item, user, group);
    const Icon = item.requestType === 'friend' ? UserPlus : Users;

    // 「同意 / 拒绝」：卡片被虚拟列表卸掉再挂上会回到未处理，重复处理上游会挡，看见了重发一次就知道
    const [outcome, setOutcome] = useState<HandleState>({ state: 'idle' });
    const [confirmReject, setConfirmReject] = useState(false);
    // 没有 flag 的请求上游不收，不给按钮
    const handle = handleRequestCall(item, false);
    const sending = outcome.state === 'sending';

    const act = async (approve: boolean) => {
        setOutcome({ state: 'sending', approve });
        const res = await api.handleRequest(item, approve);
        setOutcome(res.ok ? { state: 'done', approve } : { state: 'failed', reason: res.reason });
    };
    const bot = api.bot();
    const onReject = () => {
        if (bot && handle && !dangerConfirmSkipped(bot.id, handle.action)) {
            setConfirmReject(true);
            return;
        }
        void act(false);
    };

    return (
        <div ref={ref} className="px-6 py-1.5">
            <div className="rounded-md border border-border-subtle bg-surface px-3 py-2 shadow-sm">
                <div className="flex min-w-0 items-center gap-1.5 text-[12.5px] font-medium text-text">
                    <Icon size={13} aria-hidden className="shrink-0 text-brand" />
                    <span className="min-w-0 flex-1 truncate">{title}</span>
                    <span className="shrink-0 text-2xs font-normal tabular-nums text-text-tertiary">
                        {clockTime(item.at)}
                    </span>
                </div>
                {item.comment && (
                    <p className="mt-1 break-words text-xs leading-relaxed text-text-secondary">
                        验证消息：{item.comment}
                    </p>
                )}
                {handle && (
                    <div className="mt-1.5 flex min-w-0 items-center gap-1.5">
                        {outcome.state === 'done' ? (
                            <span
                                className={cn(
                                    'inline-flex items-center gap-1 text-2xs font-medium',
                                    outcome.approve ? 'text-success' : 'text-text-tertiary',
                                )}
                            >
                                <Check size={12} strokeWidth={2.4} aria-hidden />已
                                {outcome.approve ? '同意' : '拒绝'}
                            </span>
                        ) : (
                            <>
                                <Button
                                    size="sm"
                                    variant="secondary"
                                    disabled={sending}
                                    onClick={() => void act(true)}
                                >
                                    {sending && outcome.approve ? (
                                        <Spinner size="xs" label="正在同意" />
                                    ) : (
                                        <Check size={12} strokeWidth={2.4} aria-hidden />
                                    )}
                                    同意
                                </Button>
                                <Button
                                    size="sm"
                                    variant="ghost"
                                    disabled={sending}
                                    onClick={onReject}
                                >
                                    {sending && !outcome.approve ? (
                                        <Spinner size="xs" label="正在拒绝" />
                                    ) : (
                                        <X size={12} strokeWidth={2.4} aria-hidden />
                                    )}
                                    拒绝
                                </Button>
                            </>
                        )}
                        {outcome.state === 'failed' && (
                            <span
                                className="min-w-0 truncate text-2xs text-danger"
                                title={outcome.reason}
                            >
                                {outcome.reason}
                            </span>
                        )}
                    </div>
                )}
                <div className="mt-1.5 flex min-w-0 flex-wrap items-center gap-x-1 gap-y-0.5 text-2xs text-text-tertiary">
                    <span className="min-w-0 max-w-full truncate font-mono text-[10.5px]">
                        flag: {item.flag || '（无）'}
                    </span>
                    <span className="flex-1" />
                    {item.flag && (
                        <button
                            type="button"
                            onClick={() => copy(item.flag)}
                            className="inline-flex items-center gap-1 rounded-xs px-1.5 py-0.5 hover:bg-inset hover:text-text"
                        >
                            {copied ? (
                                <Check size={11} aria-hidden />
                            ) : (
                                <Copy size={11} aria-hidden />
                            )}
                            {copied ? '已复制' : '复制 flag'}
                        </button>
                    )}
                    <button
                        type="button"
                        onClick={(e) => api.openDetail(item, e.currentTarget)}
                        className="inline-flex items-center gap-1 rounded-xs px-1.5 py-0.5 hover:bg-inset hover:text-text"
                    >
                        <Braces size={11} aria-hidden />
                        查看 JSON
                    </button>
                    <button
                        type="button"
                        aria-disabled={!plan.ok}
                        title={plan.ok ? `填进当前请求：${plan.filled.join('、')}` : plan.reason}
                        onMouseEnter={refreshPlan}
                        onFocus={refreshPlan}
                        onClick={() => {
                            const res = api.fill(item);
                            setPlan(res.ok ? api.previewFill(item) : res);
                        }}
                        className={cn(
                            'inline-flex items-center gap-1 rounded-xs px-1.5 py-0.5',
                            plan.ok
                                ? 'hover:bg-inset hover:text-text'
                                : 'cursor-not-allowed opacity-50',
                        )}
                    >
                        <FileInput size={11} aria-hidden />
                        填入请求
                    </button>
                </div>
            </div>
            {bot && handle && (
                <DangerConfirmDialog
                    open={confirmReject}
                    onOpenChange={setConfirmReject}
                    botId={bot.id}
                    botName={bot.name}
                    action={handle.action}
                    reason={rejectLine(item, user, group)}
                    params={handle.params}
                    onConfirm={() => void act(false)}
                />
            )}
        </div>
    );
});

export const CallChip = memo(function CallChip({ item }: { item: Of<'call'> }) {
    const api = useChatView();
    const line = callLine(item.call);
    const origin = originLabel(item.origin);
    return (
        <div className="flex justify-center px-6 py-1">
            <button
                type="button"
                onClick={(e) => api.openDetail(item, e.currentTarget)}
                title={item.summary ? `${item.action} ${item.summary}` : item.action}
                className={cn(
                    'inline-flex max-w-full items-center gap-1 rounded-pill border border-dashed px-2.5 py-0.5 font-mono text-[11px] transition-colors',
                    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                    line.ok
                        ? 'border-border text-text-tertiary hover:border-text-tertiary hover:text-text-secondary'
                        : 'border-danger/40 text-danger hover:border-danger',
                )}
            >
                <span className="truncate">
                    ↗ {item.action} {line.text}
                    {origin ? ` · ${origin}` : ''}
                    {item.summary ? ` · ${item.summary}` : ''}
                </span>
            </button>
        </div>
    );
});

export const MetaRow = memo(function MetaRow({ item }: { item: Of<'meta'> }) {
    const api = useChatView();
    return (
        <div className="flex justify-center px-6 py-0.5">
            <button
                type="button"
                onClick={(e) => api.openDetail(item, e.currentTarget)}
                className="max-w-full truncate rounded-xs px-1.5 text-2xs text-text-tertiary hover:text-text-secondary"
            >
                {item.text} · {clockTime(item.at)}
            </button>
        </div>
    );
});

const RECEIVER_DOT = {
    success: 'bg-success',
    warning: 'bg-warning',
    danger: 'bg-danger',
    neutral: 'bg-text-disabled',
} as const;

export const ReceiverRow = memo(function ReceiverRow({ item }: { item: Of<'receiver'> }) {
    const copy = receiverStateCopy(item.state);
    return (
        <div className="flex items-center justify-center gap-1.5 px-6 py-0.5 text-2xs text-text-tertiary">
            <span aria-hidden className={cn('h-1.5 w-1.5 rounded-full', RECEIVER_DOT[copy.tone])} />
            <span className="truncate">
                接收：{copy.text} · {clockTime(item.at)}
            </span>
        </div>
    );
});

/** 琥珀色分隔线：这段时间的数据不完整 */
function WarnDivider({ children }: { children: React.ReactNode }) {
    return (
        <div role="note" className="flex items-center gap-2 px-4 py-2 text-2xs text-warning">
            <span aria-hidden className="h-px flex-1 bg-warning/35" />
            <TriangleAlert size={11} strokeWidth={2.2} aria-hidden className="shrink-0" />
            <span className="min-w-0 text-center">{children}</span>
            <span aria-hidden className="h-px flex-1 bg-warning/35" />
        </div>
    );
}

export const GapRow = memo(function GapRow({ item }: { item: Of<'gap'> }) {
    return <WarnDivider>{gapRange(item.fromMs, item.toMs)} 断开期间可能漏了事件</WarnDivider>;
});

export const DroppedRow = memo(function DroppedRow({ item }: { item: Of<'dropped'> }) {
    return <WarnDivider>上游丢了 {countFormat.format(item.count)} 条（接收太慢）</WarnDivider>;
});

/** 时间线最顶上：更早的已经被缓冲挤掉了 */
export function TrimmedNote({ count }: { count: number }) {
    return (
        <div
            role="note"
            className="flex items-center gap-2 px-6 pb-1 pt-2 text-2xs text-text-tertiary"
        >
            <span aria-hidden className="h-px flex-1 border-t border-dashed border-border" />
            <span>更早的 {countFormat.format(count)} 条已丢弃</span>
            <span aria-hidden className="h-px flex-1 border-t border-dashed border-border" />
        </div>
    );
}
