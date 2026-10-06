// 顶栏最左边的 Bot 选择器：「发给谁」。按宿主分组，Bot 多了（> 6）才出搜索框。
//
// 列表是一组按钮，↑ ↓ 在按钮间移动焦点，搜索框里 ↓ 进列表、回车选第一个匹配的。

import {
    memo,
    useMemo,
    useRef,
    useState,
    type KeyboardEvent as ReactKeyboardEvent,
    type ReactNode,
} from 'react';
import { Check, ChevronDown, Search } from 'lucide-react';
import { cn } from '../../shared/utils/cn';
import { Popover, PopoverContent, PopoverTrigger, Spinner } from '../../shared/ui';
import { StatusDot, type StatusDotTone } from '../../shared/ui/motion';
import { useMotion } from '../../hooks/preferences/useMotion';
import { useServerProfiles } from '../../hooks/remote/useIsHostReachable';
import {
    backendShortLabel,
    filterTargets,
    groupTargets,
    targetDisplayName,
} from '../../core/domain/debug/targetGroups';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import type { BackendType } from '../../core/ipc/generated/domain/BackendType';

const SEARCH_THRESHOLD = 6;

export interface BotPickerProps {
    targets: readonly DebugTarget[];
    selected: DebugTarget | null;
    loading?: boolean;
    ariaLabel?: string;
    compact?: boolean;
    statusIndicator?: ReactNode;
    onSelect: (botId: string) => void;
    /** 列表底部「去机器人页」；不给就不显示 */
    onManageBots?: () => void;
}

function targetTone(t: DebugTarget): StatusDotTone {
    if (!t.running) return 'idle';
    return t.online === false ? 'warning' : 'success';
}

function targetState(t: DebugTarget): string | null {
    if (!t.running) return '未运行';
    if (t.online === false) return 'QQ 未登录';
    return null;
}

export function BackendTag({ backend, className }: { backend: BackendType; className?: string }) {
    const sl = backend === 'snowluma';
    return (
        <span
            title={sl ? 'SnowLuma' : 'NapCat'}
            className={cn(
                'inline-flex h-4 shrink-0 items-center rounded-xs px-1 font-mono text-[10px] font-semibold leading-none tracking-wide',
                sl ? 'bg-info-soft text-info' : 'bg-brand-soft text-brand',
                className,
            )}
        >
            {backendShortLabel(backend)}
        </span>
    );
}

export const BotPicker = memo(function BotPicker({
    targets,
    selected,
    loading = false,
    ariaLabel,
    compact = false,
    statusIndicator,
    onSelect,
    onManageBots,
}: BotPickerProps) {
    const m = useMotion();
    const [open, setOpen] = useState(false);
    const [query, setQuery] = useState('');
    const searchRef = useRef<HTMLInputElement>(null);
    const listRef = useRef<HTMLDivElement>(null);

    const servers = useServerProfiles(open).data;
    const serverName = useMemo(() => {
        const map = new Map(
            (servers ?? []).map((s) => [s.id, s.name?.trim() || s.host?.trim() || s.id]),
        );
        return (id: string) => map.get(id);
    }, [servers]);

    const showSearch = targets.length > SEARCH_THRESHOLD;
    const filtered = useMemo(
        () => (showSearch ? filterTargets(targets, query) : [...targets]),
        [targets, query, showSearch],
    );
    const groups = useMemo(() => groupTargets(filtered, serverName), [filtered, serverName]);

    const choose = (botId: string) => {
        onSelect(botId);
        setOpen(false);
    };

    const handleOpenChange = (next: boolean) => {
        setOpen(next);
        if (!next) setQuery('');
    };

    const options = () =>
        Array.from(listRef.current?.querySelectorAll<HTMLButtonElement>('[role="option"]') ?? []);

    const onListKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
        const list = options();
        const idx = list.indexOf(document.activeElement as HTMLButtonElement);
        let next = -1;
        if (e.key === 'ArrowDown') next = Math.min(list.length - 1, idx + 1);
        else if (e.key === 'ArrowUp') {
            if (idx <= 0 && showSearch) {
                e.preventDefault();
                searchRef.current?.focus();
                return;
            }
            next = Math.max(0, idx - 1);
        } else if (e.key === 'Home') next = 0;
        else if (e.key === 'End') next = list.length - 1;
        else return;
        e.preventDefault();
        list[next]?.focus();
    };

    const onSearchKeyDown = (e: ReactKeyboardEvent<HTMLInputElement>) => {
        if (e.key === 'ArrowDown') {
            e.preventDefault();
            options()[0]?.focus();
        } else if (e.key === 'Enter') {
            e.preventDefault();
            // 选列表里看到的第一行：分组后本机排在最前，不一定是过滤结果的第一个
            const first = groups[0]?.targets[0];
            if (first) choose(first.bot_id);
        }
    };

    return (
        <Popover open={open} onOpenChange={handleOpenChange}>
            <PopoverTrigger asChild>
                <button
                    type="button"
                    aria-label={
                        ariaLabel ??
                        (selected
                            ? `当前 Bot：${targetDisplayName(selected)}，点击切换`
                            : '选择 Bot')
                    }
                    className={cn(
                        'group inline-flex h-8 min-w-0 max-w-[300px] shrink items-center gap-2 rounded-sm border border-border-subtle bg-surface pl-2.5 pr-2 text-left',
                        'transition-colors hover:border-border hover:bg-inset data-[state=open]:border-brand/50 data-[state=open]:bg-inset',
                        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
                    )}
                >
                    {selected ? (
                        <>
                            {statusIndicator ??
                                (!compact && (
                                    <StatusDot
                                        tone={targetTone(selected)}
                                        size={7}
                                        className="shrink-0"
                                    />
                                ))}
                            <span className="min-w-0 truncate text-[13px] font-medium text-text">
                                {targetDisplayName(selected)}
                            </span>
                            <BackendTag backend={selected.backend} />
                            <span
                                className={cn(
                                    'hidden shrink-0 font-mono text-2xs tabular-nums text-text-tertiary',
                                    !compact && '@min-[880px]:inline',
                                )}
                            >
                                {selected.qq_id || selected.bot_id}
                            </span>
                        </>
                    ) : loading ? (
                        <>
                            <Spinner size="xs" />
                            <span className="text-[13px] text-text-tertiary">正在读取 Bot…</span>
                        </>
                    ) : (
                        <span className="text-[13px] text-text-secondary">选择 Bot</span>
                    )}
                    <ChevronDown
                        size={14}
                        strokeWidth={2}
                        aria-hidden
                        className={cn(
                            'ml-auto shrink-0 text-text-tertiary group-data-[state=open]:rotate-180',
                            m.enabled && 'transition-transform duration-200',
                        )}
                    />
                </button>
            </PopoverTrigger>
            <PopoverContent
                align="start"
                className="flex w-[320px] flex-col overflow-hidden p-0"
                onOpenAutoFocus={(e) => {
                    // 有搜索框先落在搜索框；没有就落在当前选中的那一行，回车就是「不换」
                    e.preventDefault();
                    if (showSearch) searchRef.current?.focus();
                    else
                        (
                            listRef.current?.querySelector<HTMLButtonElement>(
                                '[aria-selected="true"]',
                            ) ?? options()[0]
                        )?.focus();
                }}
            >
                {showSearch && (
                    <div className="border-b border-border-subtle/70 p-2">
                        <label className="relative flex items-center">
                            <Search
                                size={13}
                                aria-hidden
                                className="pointer-events-none absolute left-2.5 text-text-tertiary"
                            />
                            <input
                                ref={searchRef}
                                value={query}
                                onChange={(e) => setQuery(e.target.value)}
                                onKeyDown={onSearchKeyDown}
                                placeholder="搜名字、QQ 号"
                                aria-label="搜索 Bot"
                                className={cn(
                                    'h-8 w-full rounded-sm border border-border-subtle bg-field pl-8 pr-2 text-[13px] text-text outline-none',
                                    'placeholder:text-text-tertiary focus:border-brand focus:ring-2 focus:ring-inset focus:ring-brand',
                                )}
                            />
                        </label>
                    </div>
                )}
                <div
                    ref={listRef}
                    role="listbox"
                    aria-label="Bot 列表"
                    onKeyDown={onListKeyDown}
                    className="max-h-[min(420px,60vh)] overflow-y-auto p-1"
                >
                    {groups.length === 0 ? (
                        <p className="px-3 py-6 text-center text-xs text-text-tertiary">
                            {targets.length === 0 ? '还没有 Bot' : '没有匹配的 Bot'}
                        </p>
                    ) : (
                        groups.map((g) => (
                            <div
                                key={g.key}
                                role="group"
                                aria-label={g.label}
                                className="pb-1 last:pb-0"
                            >
                                <div className="truncate px-2 pb-1 pt-2 text-2xs font-medium uppercase tracking-wider text-text-tertiary">
                                    {g.label}
                                </div>
                                {g.targets.map((t) => (
                                    <BotOption
                                        key={t.bot_id}
                                        target={t}
                                        selected={t.bot_id === selected?.bot_id}
                                        onChoose={choose}
                                    />
                                ))}
                            </div>
                        ))
                    )}
                </div>
                {onManageBots && (
                    <div className="border-t border-border-subtle/70 p-1">
                        <button
                            type="button"
                            onClick={() => {
                                setOpen(false);
                                onManageBots();
                            }}
                            className="w-full rounded-sm px-2 py-1.5 text-left text-xs text-text-secondary transition-colors hover:bg-inset hover:text-text focus-visible:bg-inset focus-visible:outline-none"
                        >
                            去「机器人」页管理 Bot…
                        </button>
                    </div>
                )}
            </PopoverContent>
        </Popover>
    );
});

function BotOption({
    target,
    selected,
    onChoose,
}: {
    target: DebugTarget;
    selected: boolean;
    onChoose: (botId: string) => void;
}) {
    const state = targetState(target);
    return (
        <button
            type="button"
            role="option"
            aria-selected={selected}
            onClick={() => onChoose(target.bot_id)}
            className={cn(
                'flex w-full items-center gap-2.5 rounded-sm px-2 py-1.5 text-left transition-colors',
                'hover:bg-inset focus-visible:bg-inset focus-visible:outline-none',
                selected && 'bg-brand-soft/60 hover:bg-brand-soft',
            )}
        >
            <StatusDot tone={targetTone(target)} size={7} className="shrink-0" />
            <span className="min-w-0 flex-1">
                <span className="flex min-w-0 items-center gap-1.5">
                    <span className="truncate text-[13px] font-medium text-text">
                        {targetDisplayName(target)}
                    </span>
                    <BackendTag backend={target.backend} />
                </span>
                <span className="mt-0.5 flex items-center gap-1.5 text-2xs text-text-tertiary">
                    <span className="font-mono tabular-nums">{target.qq_id || target.bot_id}</span>
                    {state && (
                        <>
                            <span aria-hidden className="text-border">
                                ·
                            </span>
                            <span
                                className={target.running ? 'text-warning' : 'text-text-disabled'}
                            >
                                {state}
                            </span>
                        </>
                    )}
                </span>
            </span>
            {selected && (
                <Check size={14} strokeWidth={2.4} aria-hidden className="shrink-0 text-brand" />
            )}
        </button>
    );
}
