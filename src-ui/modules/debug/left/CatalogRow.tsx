// 接口目录里的一行（接口）和分类标题行。都是虚拟列表里的定高行，
// 键盘焦点在外层列表上（aria-activedescendant），行本身不进 Tab 顺序。

import { memo } from 'react';
import { Copy, ExternalLink, SquarePlus } from 'lucide-react';
import { cn } from '../../../shared/utils/cn';
import {
    ContextMenu,
    ContextMenuContent,
    ContextMenuItem,
    ContextMenuLabel,
    ContextMenuSeparator,
    ContextMenuTrigger,
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from '../../../shared/ui';
import { ExpandChevron } from '../../../shared/ui/motion';
import type { BackendType } from '../../../core/ipc/generated/domain/BackendType';
import type { DebugActionSummary } from '../../../core/ipc/generated/debug/DebugActionSummary';
import { SAFETY_DOT_CLASS, SAFETY_TEXT, onlyBackendLabel } from '../../../core/domain/debug/safety';
import { catalogRowLabel } from '../../../core/domain/debug/catalogView';

export const CATALOG_ROW_HEIGHT = 30;
export const CATALOG_HEADER_HEIGHT = 28;

const CHIP = 'inline-flex shrink-0 items-center rounded-xs px-1 text-[10px] font-medium leading-4';

export interface CatalogActionRowProps {
    id: string;
    action: DebugActionSummary;
    backend: BackendType;
    /** 在列表里缩进一级（分组视图）还是顶格（搜索结果） */
    nested: boolean;
    /** 键盘高亮 */
    active: boolean;
    /** 是当前标签打开的接口 */
    current: boolean;
    onOpen: (name: string, newTab: boolean) => void;
    onCopyName: (name: string) => void;
}

export const CatalogActionRow = memo(function CatalogActionRow({
    id,
    action,
    backend,
    nested,
    active,
    current,
    onOpen,
    onCopyName,
}: CatalogActionRowProps) {
    const only = onlyBackendLabel(action, backend);
    const tip = [action.summary || '（没有简介）', SAFETY_TEXT[action.safety]];
    if (!action.supported) tip.push('当前 Bot 不支持这个接口');
    if (action.stream) tip.push('流式接口：分块传输只走内部通道');

    return (
        <ContextMenu>
            <Tooltip delayDuration={300}>
                <ContextMenuTrigger asChild>
                    <TooltipTrigger asChild>
                        <div
                            id={id}
                            role="treeitem"
                            aria-level={nested ? 2 : 1}
                            aria-selected={active}
                            aria-current={current ? 'true' : undefined}
                            aria-disabled={!action.supported || undefined}
                            data-flip={`a:${action.name}`}
                            onClick={(e) => onOpen(action.name, e.ctrlKey || e.metaKey)}
                            onMouseDown={(e) => {
                                // 中键按下默认会进入自动滚动模式，这里要的是「另开标签」
                                if (e.button === 1) e.preventDefault();
                            }}
                            onAuxClick={(e) => {
                                if (e.button === 1) onOpen(action.name, true);
                            }}
                            style={{ height: CATALOG_ROW_HEIGHT }}
                            className={cn(
                                'group relative flex cursor-pointer select-none items-center gap-2 rounded-sm pr-1.5 transition-colors',
                                nested ? 'pl-5' : 'pl-2',
                                current ? 'bg-elevated/60' : 'hover:bg-elevated/35',
                                active && 'bg-inset ring-1 ring-inset ring-brand/50',
                                !action.supported && 'opacity-60',
                            )}
                        >
                            <span
                                aria-hidden
                                className={cn(
                                    'absolute bottom-1.5 left-0 top-1.5 w-[2px] rounded-r-pill bg-brand transition-opacity',
                                    current ? 'opacity-100' : 'opacity-0',
                                )}
                            />
                            <span aria-hidden className={cn('h-[7px] w-[7px] shrink-0 rounded-full', SAFETY_DOT_CLASS[action.safety])} />
                            {/* 列表只放主名（简介剥掉参数括注；没有简介退回字段名），字段名和完整简介在悬停提示、右键菜单里 */}
                            {action.summary ? (
                                <span className="min-w-0 flex-1 truncate text-[12px] text-text">{catalogRowLabel(action)}</span>
                            ) : (
                                <span className="min-w-0 flex-1 truncate font-mono text-[12px] text-text">{action.name}</span>
                            )}
                            {only && <span className={cn(CHIP, 'bg-info-soft text-info')}>{only}</span>}
                            {action.param_diff && <span className={cn(CHIP, 'bg-warning-soft text-warning')}>参数不同</span>}
                            {action.stream && <span className={cn(CHIP, 'bg-brand-soft text-brand')}>流式</span>}
                        </div>
                    </TooltipTrigger>
                </ContextMenuTrigger>
                <TooltipContent side="right" className="max-w-[280px] whitespace-normal py-1.5 leading-relaxed">
                    <span className="block font-mono">{action.name}</span>
                    {tip.map((line) => (
                        <span key={line} className="block opacity-80">
                            {line}
                        </span>
                    ))}
                </TooltipContent>
            </Tooltip>
            <ContextMenuContent className="w-48">
                <ContextMenuLabel className="truncate font-mono text-2xs">{action.name}</ContextMenuLabel>
                <ContextMenuSeparator />
                <ContextMenuItem onClick={() => onOpen(action.name, false)}>
                    <ExternalLink size={13} />
                    <span>打开</span>
                </ContextMenuItem>
                <ContextMenuItem onClick={() => onOpen(action.name, true)}>
                    <SquarePlus size={13} />
                    <span>在新标签打开</span>
                </ContextMenuItem>
                <ContextMenuItem onClick={() => onCopyName(action.name)}>
                    <Copy size={13} />
                    <span>复制接口名</span>
                </ContextMenuItem>
            </ContextMenuContent>
        </ContextMenu>
    );
});

export interface CatalogHeaderRowProps {
    id: string;
    label: string;
    /** 标题里已经带了数量的（「当前 Bot 不支持（N）」）不再重复 */
    count: number | null;
    open: boolean;
    active: boolean;
    /** 「当前 Bot 不支持」那一段：和分类分开，放在最底下、颜色更淡 */
    muted?: boolean;
    onToggle: () => void;
    flipKey: string;
}

export const CatalogHeaderRow = memo(function CatalogHeaderRow({
    id,
    label,
    count,
    open,
    active,
    muted = false,
    onToggle,
    flipKey,
}: CatalogHeaderRowProps) {
    return (
        <div
            id={id}
            role="treeitem"
            aria-level={1}
            aria-expanded={open}
            aria-selected={active}
            data-flip={flipKey}
            onClick={onToggle}
            style={{ height: CATALOG_HEADER_HEIGHT }}
            className={cn(
                'flex cursor-pointer select-none items-center gap-1.5 rounded-sm px-1.5 text-[11px] font-medium transition-colors hover:bg-elevated/35',
                muted ? 'text-text-tertiary' : 'text-text-secondary',
                active && 'bg-inset ring-1 ring-inset ring-brand/50',
            )}
        >
            <ExpandChevron open={open} size={13} />
            <span className="min-w-0 truncate">{label}</span>
            {count !== null && <span className="ml-auto shrink-0 text-[10px] tabular-nums text-text-tertiary">{count}</span>}
        </div>
    );
});
