// 右栏标题行：「聊天 / 列表」切换、搜索（点开才展开）、筛选、暂停显示、清屏、接收状态。
//
// 暂停只是不刷新画面，后端和 store 照收照记；继续时跳到最新。
// 接收状态点开能「停止接收」/「重新接收」。标题行是一个容器查询：窄的时候把次要文字收起来。

import { memo, useEffect, useRef, useState } from 'react';
import { Eraser, List, ListFilter, MessagesSquare, Pause, Play, RotateCw, Search, Square, X } from 'lucide-react';
import { cn } from '../../../shared/utils/cn';
import {
    Checkbox,
    Popover,
    PopoverContent,
    PopoverTrigger,
    Spinner,
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from '../../../shared/ui';
import type { DebugReceiverView } from '../../../hooks/debug/debugEventStore';
import { channelShortLabel } from '../../../core/domain/debug/channelCopy';
import { receiverStateCopy } from '../../../core/domain/debug/receiverCopy';
import type { ChatFilter } from '../../../core/domain/debug/chatFilter';
import type { DebugChatView } from '../../../core/ipc/generated/debug/DebugChatView';
import { COLUMN_HEADER_CLASS } from '../ColumnFrame';
import { countFormat } from '../../../core/domain/debug/chatFormat';
import { IconAction } from './rightParts';

export type KindFilter = ChatFilter['kinds'];

const KIND_LABELS: Array<{ key: keyof KindFilter; label: string; hint: string }> = [
    { key: 'message', label: '消息', hint: '群聊、私聊，含 Bot 自己发的' },
    { key: 'notice', label: '通知', hint: '进群、撤回、禁言、戳一戳…' },
    { key: 'request', label: '请求', hint: '加好友、加群' },
    { key: 'call', label: '调用', hint: '和消息无关的接口调用' },
    { key: 'meta', label: '元事件', hint: '生命周期、心跳' },
];

export interface ChatToolbarProps {
    view: DebugChatView;
    onViewChange: (view: DebugChatView) => void;
    kinds: KindFilter;
    showHeartbeat: boolean;
    onKindsChange: (kinds: KindFilter, showHeartbeat: boolean) => void;
    filterActive: boolean;
    onResetFilter: () => void;
    search: string;
    onSearchChange: (text: string) => void;
    /** 搜索 / 筛选后剩几条（有搜索词时显示在框里） */
    matchCount: number;
    paused: boolean;
    onTogglePause: () => void;
    canClear: boolean;
    onClear: () => void;
    receiver: DebugReceiverView;
    running: boolean;
    onStopReceiving: () => Promise<void>;
    onRestartReceiving: () => Promise<void>;
}

export const ChatToolbar = memo(function ChatToolbar(props: ChatToolbarProps) {
    const { view, onViewChange, search, onSearchChange } = props;
    const [searchOpen, setSearchOpen] = useState(search !== '');
    const inputRef = useRef<HTMLInputElement>(null);

    useEffect(() => {
        if (searchOpen) inputRef.current?.focus();
    }, [searchOpen]);

    const closeSearch = () => {
        onSearchChange('');
        setSearchOpen(false);
    };

    return (
        <div className={cn(COLUMN_HEADER_CLASS, '@container')}>
            {searchOpen ? (
                <div className="flex h-7 min-w-0 flex-1 items-center gap-1 rounded-sm border border-border-subtle bg-field px-2 focus-within:border-brand/50">
                    <Search size={13} aria-hidden className="shrink-0 text-text-tertiary" />
                    <input
                        ref={inputRef}
                        value={search}
                        onChange={(e) => onSearchChange(e.target.value)}
                        onKeyDown={(e) => {
                            if (e.key === 'Escape') {
                                e.preventDefault();
                                closeSearch();
                            }
                        }}
                        onBlur={() => {
                            if (search === '') setSearchOpen(false);
                        }}
                        placeholder="搜消息、名字、QQ 号、接口"
                        aria-label="搜索事件"
                        className="min-w-0 flex-1 bg-transparent text-xs text-text outline-none placeholder:text-text-disabled"
                    />
                    {search !== '' && (
                        <span className="shrink-0 text-2xs tabular-nums text-text-tertiary">{countFormat.format(props.matchCount)} 条</span>
                    )}
                    <IconAction
                        label="关闭搜索"
                        tip="关闭搜索（Esc）"
                        onMouseDown={(e) => e.preventDefault()}
                        onClick={closeSearch}
                        className="h-5 w-5 rounded-xs"
                    >
                        <X size={12} aria-hidden />
                    </IconAction>
                </div>
            ) : (
                <>
                    <ViewSwitch view={view} onChange={onViewChange} />
                    <span className="flex-1" />
                    <IconAction label="搜索" onClick={() => setSearchOpen(true)}>
                        <Search size={14} aria-hidden />
                    </IconAction>
                </>
            )}
            <FilterButton {...props} />
            <IconAction
                label={props.paused ? '继续显示' : '暂停显示'}
                tip={props.paused ? '继续显示（跳到最新）' : '暂停显示：画面不动，后台照常接收'}
                active={props.paused}
                aria-pressed={props.paused}
                onClick={props.onTogglePause}
            >
                {props.paused ? <Play size={14} aria-hidden /> : <Pause size={14} aria-hidden />}
            </IconAction>
            <IconAction label="清屏" tip="清屏：只清这里的显示，不影响接收" disabled={!props.canClear} onClick={props.onClear}>
                <Eraser size={14} aria-hidden />
            </IconAction>
            <ReceiverChip
                receiver={props.receiver}
                running={props.running}
                onStop={props.onStopReceiving}
                onRestart={props.onRestartReceiving}
            />
        </div>
    );
});

function ViewSwitch({ view, onChange }: { view: DebugChatView; onChange: (v: DebugChatView) => void }) {
    const options: Array<{ id: DebugChatView; label: string; icon: typeof List }> = [
        { id: 'chat', label: '聊天', icon: MessagesSquare },
        { id: 'list', label: '列表', icon: List },
    ];
    return (
        <div role="radiogroup" aria-label="显示方式" className="flex shrink-0 items-center gap-0.5 rounded-md bg-inset p-0.5">
            {options.map((o) => {
                const on = o.id === view;
                const Icon = o.icon;
                return (
                    <button
                        key={o.id}
                        type="button"
                        role="radio"
                        aria-checked={on}
                        aria-label={o.label}
                        onClick={() => onChange(o.id)}
                        className={cn(
                            'inline-flex h-6 items-center gap-1 rounded-sm px-2 text-[12px] font-medium transition-colors',
                            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                            on ? 'bg-elevated text-text shadow-sm ring-1 ring-border-subtle' : 'text-text-tertiary hover:text-text',
                        )}
                    >
                        <Icon size={12} strokeWidth={2.2} aria-hidden />
                        <span className="hidden @min-[340px]:inline">{o.label}</span>
                    </button>
                );
            })}
        </div>
    );
}

function FilterButton({ kinds, showHeartbeat, onKindsChange, filterActive, onResetFilter }: ChatToolbarProps) {
    const [open, setOpen] = useState(false);
    return (
        <Popover open={open} onOpenChange={setOpen}>
            <PopoverTrigger asChild>
                <IconAction label="筛选" tip={filterActive ? '筛选（有条件在生效）' : '筛选'} active={filterActive}>
                    <ListFilter size={14} aria-hidden />
                </IconAction>
            </PopoverTrigger>
            <PopoverContent align="end" className="w-[230px] p-2">
                <p className="px-1 pb-1.5 text-2xs font-medium text-text-tertiary">显示哪些</p>
                <div className="flex flex-col gap-1.5 px-1">
                    {KIND_LABELS.map((k) => (
                        <Checkbox
                            key={k.key}
                            checked={kinds[k.key]}
                            onCheckedChange={(v) => onKindsChange({ ...kinds, [k.key]: v }, showHeartbeat)}
                            label={k.label}
                            hint={k.hint}
                        />
                    ))}
                    <div className="mt-1 border-t border-border-subtle/70 pt-2">
                        <Checkbox
                            checked={showHeartbeat}
                            disabled={!kinds.meta}
                            onCheckedChange={(v) => onKindsChange(kinds, v)}
                            label="显示心跳"
                            hint="心跳一直在来，默认藏起来"
                        />
                    </div>
                </div>
                {filterActive && (
                    <button
                        type="button"
                        onClick={onResetFilter}
                        className="mt-2 w-full rounded-xs px-2 py-1 text-left text-2xs text-info hover:bg-inset"
                    >
                        恢复默认
                    </button>
                )}
            </PopoverContent>
        </Popover>
    );
}

const DOT = {
    success: 'bg-success',
    warning: 'bg-warning',
    danger: 'bg-danger',
    neutral: 'bg-text-disabled',
} as const;
const TEXT = {
    success: 'text-success',
    warning: 'text-warning',
    danger: 'text-danger',
    neutral: 'text-text-tertiary',
} as const;

/** 胶囊上写的就是完整状态（「重连中（第 n 次，x 秒后）」「已停止：原因」）；窄的时候只剩圆点，悬停看全文 */
function receiverSummary(r: DebugReceiverView, running: boolean): { text: string; tone: keyof typeof DOT } {
    if (r.state) return receiverStateCopy(r.state);
    if (r.error) return { text: `接收失败：${r.error}`, tone: 'danger' };
    if (!running) return { text: 'Bot 没在运行', tone: 'neutral' };
    return { text: r.subscribed ? '连接中' : '还没开始接收', tone: 'neutral' };
}

function ReceiverChip({
    receiver,
    running,
    onStop,
    onRestart,
}: {
    receiver: DebugReceiverView;
    running: boolean;
    onStop: () => Promise<void>;
    onRestart: () => Promise<void>;
}) {
    const [open, setOpen] = useState(false);
    const [busy, setBusy] = useState<'stop' | 'restart' | null>(null);
    const s = receiverSummary(receiver, running);
    const stopped = !receiver.subscribed || receiver.state?.state === 'stopped';
    const info = receiver.receiver;

    const run = (kind: 'stop' | 'restart', fn: () => Promise<void>) => {
        setBusy(kind);
        void fn().finally(() => setBusy(null));
    };

    return (
        <Popover open={open} onOpenChange={setOpen}>
            <Tooltip>
                <TooltipTrigger asChild>
                    <PopoverTrigger asChild>
                        <button
                            type="button"
                            aria-label={`接收状态：${s.text}，点击管理`}
                            className={cn(
                                'inline-flex h-7 min-w-0 max-w-[14rem] shrink items-center gap-1.5 rounded-pill px-2 text-2xs font-medium transition-colors',
                                'hover:bg-inset data-[state=open]:bg-inset focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                                TEXT[s.tone],
                            )}
                        >
                            <span aria-hidden className={cn('h-1.5 w-1.5 shrink-0 rounded-full', DOT[s.tone])} />
                            <span aria-hidden className="hidden min-w-0 truncate @min-[360px]:inline">
                                {s.text}
                            </span>
                        </button>
                    </PopoverTrigger>
                </TooltipTrigger>
                <TooltipContent side="bottom" className="max-w-[280px] whitespace-normal">
                    {s.text}
                </TooltipContent>
            </Tooltip>
            <PopoverContent align="end" className="w-[260px] p-0">
                <div className="px-3 py-2.5">
                    <p className="font-display text-[13px] font-semibold text-text">事件接收</p>
                    <p className={cn('mt-1 break-words text-xs', TEXT[s.tone])}>{s.text}</p>
                    {info && (
                        <dl className="mt-2 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-2xs text-text-tertiary">
                            <dt>来源</dt>
                            <dd className="truncate text-text-secondary">{channelShortLabel(info.source)}</dd>
                            <dt>缓冲</dt>
                            <dd className="tabular-nums text-text-secondary">{countFormat.format(info.buffered)} 条</dd>
                            {info.dropped_total > 0 && (
                                <>
                                    <dt>丢了</dt>
                                    <dd className="tabular-nums text-warning">{countFormat.format(info.dropped_total)} 条</dd>
                                </>
                            )}
                        </dl>
                    )}
                    <p className="mt-2 text-2xs leading-relaxed text-text-tertiary">
                        离开调试台后照样在后台收；停止后要看新事件点「重新接收」。
                    </p>
                </div>
                <div className="flex justify-end gap-1.5 border-t border-border-subtle/70 px-3 py-2">
                    {stopped ? (
                        <button
                            type="button"
                            disabled={!running || busy !== null}
                            onClick={() => run('restart', onRestart)}
                            className="inline-flex h-7 items-center gap-1 rounded-xs bg-brand px-2.5 text-2xs font-medium text-white transition-colors hover:bg-brand-hover disabled:cursor-not-allowed disabled:opacity-50"
                        >
                            {busy === 'restart' ? <Spinner size="xs" label="正在开始接收" className="text-white" /> : <RotateCw size={11} aria-hidden />}
                            {running ? '重新接收' : 'Bot 没在运行'}
                        </button>
                    ) : (
                        <button
                            type="button"
                            disabled={busy !== null}
                            onClick={() => run('stop', onStop)}
                            className="inline-flex h-7 items-center gap-1 rounded-xs px-2.5 text-2xs font-medium text-text-secondary transition-colors hover:bg-danger-soft hover:text-danger disabled:cursor-not-allowed disabled:opacity-50"
                        >
                            {busy === 'stop' ? <Spinner size="xs" label="正在停止" /> : <Square size={10} strokeWidth={3} aria-hidden />}
                            停止接收
                        </button>
                    )}
                </div>
            </PopoverContent>
        </Popover>
    );
}
