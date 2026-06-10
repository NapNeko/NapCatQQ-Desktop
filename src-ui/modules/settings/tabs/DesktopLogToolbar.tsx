// 日志 Tab 统一工具条：筛选 + 操作（原 sticky 与卡片内条合并为一条）。

import {
    Copy,
    FolderOpen,
    Minus,
    Pause,
    Play,
    Plus,
    RefreshCw,
    Search,
} from 'lucide-react';
import {
    DESKTOP_LOG_LEVEL_OPTIONS,
    type DesktopLogLevelFilterValue,
} from '../../../core/domain/desktop-log';
import type { DesktopLogViewer } from '../../../hooks/diagnostics/useDesktopLogViewer';
import { ActionMotionIcon, LIVE_MOTION, refreshMotion } from '../../../shared/ui/motion';
import { Badge, Button, Select } from '../../../shared/ui';

type Props = Pick<
    DesktopLogViewer,
    | 'level'
    | 'setLevel'
    | 'query'
    | 'setQuery'
    | 'fontSize'
    | 'decFont'
    | 'incFont'
    | 'filteredCount'
    | 'loading'
    | 'reload'
    | 'onCopy'
    | 'onOpenLocation'
    | 'opening'
    | 'copyDisabled'
    | 'autoScroll'
    | 'setAutoScroll'
>;

export function DesktopLogToolbar(props: Props) {
    const {
        level,
        setLevel,
        query,
        setQuery,
        fontSize,
        decFont,
        incFont,
        filteredCount,
        loading,
        reload,
        onCopy,
        onOpenLocation,
        opening,
        copyDisabled,
        autoScroll,
        setAutoScroll,
    } = props;

    return (
        <div className="flex shrink-0 flex-wrap items-center gap-x-1.5 gap-y-1 border-t border-border-subtle/70 px-2.5 py-1.5">
            <div className="relative min-w-0 w-full basis-full sm:w-auto sm:min-w-[180px] sm:flex-1 sm:basis-auto">
                <Search
                    size={13}
                    className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-text-tertiary"
                />
                <input
                    type="search"
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                    placeholder="搜索关键字"
                    aria-label="搜索日志关键字"
                    className="h-7 w-full rounded-sm bg-transparent pl-7 pr-2 text-[12px] text-text outline-none transition-colors placeholder:text-text-tertiary hover:bg-inset/60 focus:bg-inset"
                />
            </div>

            <Select
                className="w-[7.25rem] shrink-0 gap-0 [&_button]:h-7 [&_button]:min-h-7 [&_button]:py-0 [&_button]:px-2 [&_button]:text-[12px] [&_button]:leading-none"
                value={level}
                onValueChange={(v) => setLevel(v as DesktopLogLevelFilterValue)}
                items={DESKTOP_LOG_LEVEL_OPTIONS.map((opt) => ({
                    value: opt.value,
                    label: opt.label,
                }))}
            />

            <div className="flex items-center gap-0.5 rounded-md bg-inset/60 p-0.5">
                <Button
                    variant="ghost"
                    size="icon"
                    className="h-6 w-6"
                    aria-label="缩小字号"
                    onClick={decFont}
                >
                    <Minus size={13} />
                </Button>
                <span className="min-w-[1.75rem] text-center font-mono text-[10.5px] tabular-nums text-text-tertiary">
                    {fontSize}
                </span>
                <Button
                    variant="ghost"
                    size="icon"
                    className="h-6 w-6"
                    aria-label="放大字号"
                    onClick={incFont}
                >
                    <Plus size={13} />
                </Button>
            </div>

            <Badge tone="neutral" appearance="soft" className="shrink-0 tabular-nums">
                {loading ? '…' : `${filteredCount} 行`}
            </Badge>

            <span className="hidden h-4 w-px shrink-0 bg-border-subtle/80 sm:block" aria-hidden />

            <div className="flex flex-wrap items-center gap-0.5 sm:ml-auto">
                <Button
                    variant="ghost"
                    size="sm"
                    className="h-7"
                    onClick={() => void reload()}
                    disabled={loading}
                    title="刷新"
                >
                    <ActionMotionIcon icon={RefreshCw} size={13} motion={refreshMotion(loading)} />
                    <span className="ml-1 hidden text-[11.5px] md:inline">刷新</span>
                </Button>

                <Button
                    variant="ghost"
                    size="sm"
                    className="h-7"
                    onClick={() => void onCopy()}
                    disabled={copyDisabled}
                    title="复制当前可见日志"
                >
                    <Copy size={13} />
                    <span className="ml-1 hidden text-[11.5px] md:inline">复制</span>
                </Button>

                <Button
                    variant="ghost"
                    size="sm"
                    className="h-7"
                    onClick={() => void onOpenLocation()}
                    disabled={opening}
                    title="打开日志目录"
                >
                    <FolderOpen size={13} strokeWidth={2.2} />
                    <span className="ml-1 hidden text-[11.5px] lg:inline">打开位置</span>
                </Button>

                <Button
                    variant="ghost"
                    size="sm"
                    className="h-7"
                    onClick={() => setAutoScroll(!autoScroll)}
                    title={autoScroll ? '已启用自动滚动' : '已暂停自动滚动'}
                >
                    {autoScroll ? (
                        <ActionMotionIcon icon={Pause} size={13} motion={LIVE_MOTION} />
                    ) : (
                        <ActionMotionIcon icon={Play} size={13} />
                    )}
                    <span className="ml-1 hidden text-[11.5px] lg:inline">
                        {autoScroll ? '滚动中' : '已暂停'}
                    </span>
                </Button>
            </div>
        </div>
    );
}