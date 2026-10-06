// 发送条：这个标签走哪条通道、参数问题汇总、发送 / 取消。
//
// 能不能发、为什么不能发都直接写在按钮旁边，不让用户对着一个灰按钮猜；参数校验有问题时照样能发
// （按钮变成「仍然发送」）。发送中按钮换成计时和「取消」，提示取消只是不再等。
// 危险接口的确认框由外面弹，这里只负责把按钮画成红的。

import { memo, useEffect, useRef, useState } from 'react';
import { AlertTriangle, Check, ChevronDown, Send, Square } from 'lucide-react';
import { cn } from '../../../shared/utils/cn';
import {
    Button,
    Popover,
    PopoverContent,
    PopoverTrigger,
    Spinner,
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from '../../../shared/ui';
import { StatusDot, type StatusDotTone } from '../../../shared/ui/motion';
import { useMotion } from '../../../hooks/preferences/useMotion';
import { useNowMs } from '../../../hooks/ui/useNowMs';
import { channelIdKey } from '../../../hooks/debug/keys';
import { channelShortLabel, channelStatusCopy } from '../../../core/domain/debug/channelCopy';
import {
    channelSelectable,
    channelTriggerLabel,
    effectiveChannelId,
    findChannel,
    sameChannel,
} from '../../../core/domain/debug/channelPick';
import { progressText } from '../../../core/domain/debug/streamActions';
import type { ParamIssue } from '../../../core/domain/debug/validate';
import type { DebugActionSafety } from '../../../core/ipc/generated/debug/DebugActionSafety';
import type { DebugChannelId } from '../../../core/ipc/generated/debug/DebugChannelId';
import type { DebugChannels } from '../../../core/ipc/generated/debug/DebugChannels';
import type { DebugStreamProgress } from '../../../core/ipc/generated/debug/DebugStreamProgress';
import { MOD_KEY_LABEL } from '../TopBar';
import { issueRoot } from './ParamsForm';
import { Kbd } from './centerParts';
import { elapsedText } from './viewHelpers';

/** 不可见的不换行空格：加在播报末尾让同一句话变成「新内容」 */
const NBSP = String.fromCharCode(0xa0);

const TONE_DOT: Record<ReturnType<typeof channelStatusCopy>['tone'], StatusDotTone> = {
    success: 'success',
    warning: 'warning',
    danger: 'danger',
    neutral: 'idle',
};

export interface SendBarProps {
    /** 标签自己指定的通道；null 表示跟顶栏 */
    tabChannel: DebugChannelId | null;
    /** 顶栏为这个 Bot 选的调用通道 */
    callChannel: DebugChannelId;
    channels: DebugChannels | undefined;
    onTabChannelChange: (channel: DebugChannelId | null) => void;
    blocker: string | null;
    issues: readonly ParamIssue[];
    safety: DebugActionSafety | null;
    /** 正在等回包的那次调用是什么时候发的；没在等是 null */
    inflightSince: number | null;
    /** 流式调用（分块上传 / 下载）的最新一拍进度；普通调用是 null */
    progress: DebugStreamProgress | null;
    onSend: () => void;
    onCancel: () => void;
    onJumpToIssue: (name: string) => void;
    /** 被挡住时按了 Ctrl+Enter：每按一次加一，原因那行抖一下 */
    blockedNonce: number;
}

export const SendBar = memo(function SendBar({
    tabChannel,
    callChannel,
    channels,
    onTabChannelChange,
    blocker,
    issues,
    safety,
    inflightSince,
    progress,
    onSend,
    onCancel,
    onJumpToIssue,
    blockedNonce,
}: SendBarProps) {
    const m = useMotion();
    const reasonRef = useRef<HTMLSpanElement>(null);
    const inflight = inflightSince !== null;
    const now = useNowMs(inflight, 100);
    const danger = safety === 'dangerous';
    const hasIssues = issues.length > 0 && !blocker;

    // 读屏播报「发不了」：按下那一刻把原因拍下来，之后原因变了（Bot 起来了、JSON 改好又改坏）不跟着再读，
    // 只有再按一次才读。同一个原因连按两次时末尾换个不可见字符，读屏才会再读一遍
    const [blockedSay, setBlockedSay] = useState('');
    useEffect(() => {
        if (blockedNonce > 0 && reasonRef.current) m.shake(reasonRef.current);
        if (blockedNonce > 0 && blocker)
            setBlockedSay(`发不了：${blocker}${blockedNonce % 2 === 0 ? NBSP : ''}`);
        // 只在按键那一下抖、那一下读
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [blockedNonce]);
    useEffect(() => {
        // 原因变了，上一次拍下的那句就过时了；清掉不会触发播报
        setBlockedSay('');
    }, [blocker]);

    const roots = [...new Set(issues.map((i) => issueRoot(i.path)))];

    return (
        <div className="shrink-0 border-t border-border-subtle/70 bg-surface">
            {hasIssues && (
                <div className="flex min-w-0 items-center gap-1.5 px-3 pt-1.5 text-2xs text-danger">
                    <AlertTriangle size={11} strokeWidth={2.4} aria-hidden className="shrink-0" />
                    <span className="shrink-0">{issues.length} 处参数有问题：</span>
                    <span className="flex min-w-0 flex-1 items-center gap-1 overflow-hidden">
                        {roots.slice(0, 4).map((name) => {
                            const first = issues.find((i) => issueRoot(i.path) === name);
                            return (
                                <button
                                    key={name}
                                    type="button"
                                    onClick={() => onJumpToIssue(name)}
                                    title={first ? `${first.path}：${first.message}` : undefined}
                                    className="shrink-0 truncate rounded-xs px-1 font-mono underline decoration-danger/40 underline-offset-2 hover:bg-danger-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-danger"
                                >
                                    {name}
                                    <span className="font-sans no-underline">
                                        ：{first?.message}
                                    </span>
                                </button>
                            );
                        })}
                        {roots.length > 4 && (
                            <span className="shrink-0 text-text-tertiary">
                                等 {roots.length} 个
                            </span>
                        )}
                    </span>
                </div>
            )}
            {/* 开始等回包、按了 Ctrl+Enter 却发不了，都在这里读一句 */}
            <span className="sr-only" role="status">
                {inflight ? '已发送，正在等回包' : blockedSay}
            </span>
            <div className="flex min-w-0 items-center gap-2 px-3 py-2">
                <ChannelChip
                    tabChannel={tabChannel}
                    callChannel={callChannel}
                    channels={channels}
                    onChange={onTabChannelChange}
                />
                <span ref={reasonRef} className="min-w-0 flex-1 truncate text-2xs">
                    {inflight ? (
                        <span className="text-text-tertiary">
                            取消只是不再等回包，上游可能已经执行了
                        </span>
                    ) : blocker ? (
                        <span className="text-warning">{blocker}</span>
                    ) : danger ? (
                        <span className="text-danger">危险接口，发送前会再确认一次</span>
                    ) : null}
                </span>
                {inflight ? (
                    <div className="flex shrink-0 items-center gap-2">
                        {/* 计时每 100ms 变一次，不能做成 live region；开始 / 结束由下面那段隐藏文字播报。
                            有进度时进度比计时更有用，收在右边：字数随节拍变但不刷屏 */}
                        <span className="inline-flex items-center gap-1.5 text-xs tabular-nums text-text-secondary">
                            <Spinner size="xs" tone="brand" label="正在等回包" />
                            {progress ? (
                                <span className="max-w-[16rem] truncate">
                                    {progressText(progress)}
                                </span>
                            ) : (
                                elapsedText(now - inflightSince)
                            )}
                        </span>
                        <Button size="sm" variant="secondary" onClick={onCancel}>
                            <Square size={11} strokeWidth={2.6} aria-hidden />
                            取消
                            <Kbd>Esc</Kbd>
                        </Button>
                    </div>
                ) : (
                    <Button
                        size="sm"
                        variant={hasIssues ? 'secondary' : danger ? 'danger' : 'primary'}
                        disabled={!!blocker}
                        onClick={onSend}
                        className="shrink-0"
                        aria-keyshortcuts="Control+Enter Meta+Enter"
                    >
                        {danger ? (
                            <AlertTriangle size={12} strokeWidth={2.4} aria-hidden />
                        ) : (
                            <Send size={12} strokeWidth={2.4} aria-hidden />
                        )}
                        {hasIssues ? '仍然发送' : '发送'}
                        {hasIssues && (
                            <span className="rounded-pill bg-danger-soft px-1.5 py-px text-[10px] font-semibold tabular-nums text-danger">
                                {issues.length}
                            </span>
                        )}
                        <Kbd className="hidden @min-[420px]:inline">{MOD_KEY_LABEL} ↵</Kbd>
                    </Button>
                )}
            </div>
        </div>
    );
});

// ---------------------------------------------------------------------------
// 通道
// ---------------------------------------------------------------------------

function ChannelChip({
    tabChannel,
    callChannel,
    channels,
    onChange,
}: {
    tabChannel: DebugChannelId | null;
    callChannel: DebugChannelId;
    channels: DebugChannels | undefined;
    onChange: (channel: DebugChannelId | null) => void;
}) {
    const [open, setOpen] = useState(false);
    const choice = tabChannel ?? callChannel;
    const label = channelTriggerLabel(channels, choice, 'call');
    const effective = findChannel(channels, effectiveChannelId(channels, choice, 'call'));
    const tone: StatusDotTone =
        label.none || label.missing
            ? 'danger'
            : effective
              ? TONE_DOT[channelStatusCopy(effective.status).tone]
              : 'idle';
    const topLabel = channelTriggerLabel(channels, callChannel, 'call').text;
    const own = tabChannel !== null;

    const choose = (next: DebugChannelId | null) => {
        onChange(next);
        setOpen(false);
    };

    return (
        <Popover open={open} onOpenChange={setOpen}>
            <Tooltip>
                <TooltipTrigger asChild>
                    <PopoverTrigger asChild>
                        <button
                            type="button"
                            aria-label={`这个标签的调用通道：${label.text}${own ? '（本标签指定）' : '（跟顶栏）'}`}
                            className={cn(
                                'inline-flex h-7 min-w-0 max-w-[45%] shrink items-center gap-1.5 rounded-sm border px-2 text-[12px] transition-colors',
                                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
                                own
                                    ? 'border-brand/40 bg-brand-soft/60 text-text hover:bg-brand-soft'
                                    : 'border-border-subtle text-text-secondary hover:border-border hover:bg-inset',
                            )}
                        >
                            <StatusDot tone={tone} size={6} className="shrink-0" />
                            <span
                                className={cn(
                                    'min-w-0 truncate',
                                    (label.none || label.missing) && 'text-danger',
                                )}
                            >
                                {label.text}
                                {label.missing && '（已不存在）'}
                            </span>
                            <ChevronDown
                                size={12}
                                aria-hidden
                                className="shrink-0 text-text-tertiary"
                            />
                        </button>
                    </PopoverTrigger>
                </TooltipTrigger>
                <TooltipContent side="top">
                    {own ? '这个标签单独指定了通道' : '跟着顶栏选的调用通道'}
                </TooltipContent>
            </Tooltip>
            <PopoverContent side="top" align="start" className="w-[300px] p-1">
                <p className="px-2 pb-1.5 pt-1 text-2xs text-text-tertiary">
                    这个标签发请求走哪条通道
                </p>
                <ChannelOption selected={!own} onChoose={() => choose(null)}>
                    <span className="block text-[13px] text-text">跟顶栏</span>
                    <span className="block text-2xs text-text-tertiary">眼下是 {topLabel}</span>
                </ChannelOption>
                {channels && channels.channels.length > 0 && (
                    <div aria-hidden className="mx-2 my-1 h-px bg-border-subtle/70" />
                )}
                {channels?.channels.map((c) => {
                    const ok = channelSelectable(c, 'call');
                    const status = channelStatusCopy(c.status);
                    return (
                        <ChannelOption
                            key={channelIdKey(c.id)}
                            selected={own && sameChannel(tabChannel, c.id)}
                            disabled={!ok.ok}
                            title={ok.reason}
                            onChoose={() => choose(c.id)}
                        >
                            <span className="block truncate text-[13px] text-text">
                                {channelShortLabel(c.id)}
                            </span>
                            <span className="flex items-center gap-1 text-2xs text-text-tertiary">
                                <StatusDot tone={TONE_DOT[status.tone]} size={5} />
                                <span className="truncate">{ok.ok ? status.text : ok.reason}</span>
                            </span>
                        </ChannelOption>
                    );
                })}
                {!channels && (
                    <p className="px-2 py-2 text-2xs text-text-tertiary">通道列表还没读到</p>
                )}
            </PopoverContent>
        </Popover>
    );
}

function ChannelOption({
    selected,
    disabled,
    title,
    onChoose,
    children,
}: {
    selected: boolean;
    disabled?: boolean;
    title?: string;
    onChoose: () => void;
    children: React.ReactNode;
}) {
    return (
        <button
            type="button"
            aria-pressed={selected}
            disabled={disabled}
            title={title}
            onClick={onChoose}
            className={cn(
                'flex w-full min-w-0 items-start gap-2 rounded-sm px-2 py-1.5 text-left transition-colors',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-brand',
                'disabled:cursor-not-allowed disabled:opacity-50',
                selected ? 'bg-brand-soft/60' : 'hover:bg-inset',
            )}
        >
            <span className="mt-0.5 inline-flex h-3.5 w-3.5 shrink-0 items-center justify-center text-brand">
                {selected && <Check size={12} strokeWidth={3} aria-hidden />}
            </span>
            <span className="min-w-0 flex-1">{children}</span>
        </button>
    );
}
