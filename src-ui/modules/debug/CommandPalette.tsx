// 命令面板（Ctrl+K / 标签条上的「+」）：搜接口名、中文简介、别名，也能直接打开收藏。
//
// 回车只打开、从不发送：打开之后参数还得看一眼再发，危险接口更不能一键出去（两家 WebUI 的面板都是回车就执行）。
// 回车落在当前标签（没动过就顶替，动过就另开，和左栏单击一样），Ctrl / ⌘ + 回车总是另开。
// 列表是虚拟的（目录有近两百个接口）；高度固定，边输入边筛时对话框不忽高忽低。
// 上次搜的字留着：再打开时整段选中，直接敲就换掉，↓ 就接着用。

import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { CornerDownLeft, FileQuestion, RefreshCw, Search, Star } from 'lucide-react';
import { cn } from '../../shared/utils/cn';
import { Button, Dialog, DialogContent, DialogDescription, DialogTitle } from '../../shared/ui';
import { Shimmer } from '../../shared/ui/motion';
import {
    debugWorkspaceStore,
    useDebugWorkspaceSelector,
} from '../../hooks/debug/debugWorkspaceStore';
import { useDebugCatalog } from '../../hooks/debug/useDebugCatalog';
import { useDebugCollections } from '../../hooks/debug/useDebugCollections';
import { paramsTextOf } from '../../core/domain/debug/historyReplay';
import { SAFETY_DOT_CLASS, SAFETY_LABEL, onlyBackendLabel } from '../../core/domain/debug/safety';
import {
    buildPaletteRows,
    firstSelectable,
    isSelectable,
    stepSelectable,
    type PaletteRow,
    type SelectablePaletteRow,
} from '../../core/domain/debug/palette';
import type { BackendType } from '../../core/ipc/generated/domain/BackendType';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import { MOD_KEY_LABEL } from './TopBar';

export interface CommandPaletteProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    /** 当前选中的 Bot：搜它的目录；没有选中时是 null */
    target: DebugTarget | null;
}

const ROW_HEIGHT = 34;
const HEADER_HEIGHT = 26;
const MORE_HEIGHT = 26;
/** 一次 PageUp / PageDown 挪几行 */
const PAGE_STEP = 8;
const CHIP = 'inline-flex shrink-0 items-center rounded-xs px-1 text-[10px] font-medium leading-4';

// 上次搜的字：纯界面状态，跨打开、跨路由保留
let lastQuery = '';

/** 测试用 */
export function _resetCommandPaletteForTests(): void {
    lastQuery = '';
}

export function CommandPalette({ open, onOpenChange, target }: CommandPaletteProps) {
    // 关的动画期间内容还在，收起时不闪空；动画放完才卸，关着时不订目录、收藏
    const [mounted, setMounted] = useState(open);
    if (open && !mounted) setMounted(true);
    // 面板把结果落到了另一个标签上（另开、或原本一个标签都没有）：关闭时焦点要交给
    // 新标签，不能让 Radix 去还给打开面板前的元素——它多半已随旧标签卸载，焦点会掉到 body
    const focusNewTabOnClose = useRef(false);
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                size="lg"
                hideClose
                className="p-0"
                onExited={() => setMounted(false)}
                // 焦点交给搜索框，由里面自己选中上次的字
                onOpenAutoFocus={(e) => e.preventDefault()}
                onCloseAutoFocus={(e) => {
                    const focusNewTab = focusNewTabOnClose.current;
                    focusNewTabOnClose.current = false;
                    if (!focusNewTab) return;
                    e.preventDefault();
                    // 中栏一次只挂当前标签，「接口名」输入框此刻就是新标签的那个；等它挂上再聚焦。
                    // 选择器与 RequestHeader 的 focusActionInput 是同一个，改了要一起改
                    requestAnimationFrame(() => {
                        document
                            .querySelector<HTMLElement>('[role="combobox"][aria-label="接口名"]')
                            ?.focus();
                    });
                }}
            >
                <DialogTitle className="sr-only">搜索接口</DialogTitle>
                <DialogDescription className="sr-only">
                    输入接口名、中文简介或别名；回车在当前标签打开，{MOD_KEY_LABEL}
                    +回车在新标签打开，只打开不发送。
                </DialogDescription>
                {mounted && (
                    <PaletteBody
                        target={target}
                        onClose={() => onOpenChange(false)}
                        onOpenedOtherTab={() => {
                            focusNewTabOnClose.current = true;
                        }}
                    />
                )}
            </DialogContent>
        </Dialog>
    );
}

function PaletteBody({
    target,
    onClose,
    onOpenedOtherTab,
}: {
    target: DebugTarget | null;
    onClose: () => void;
    /** 这次选择把活动标签换成了另一个（原地顶替不算）；上面据此决定关闭时的焦点去向 */
    onOpenedOtherTab: () => void;
}) {
    const inputRef = useRef<HTMLInputElement>(null);
    const listRef = useRef<HTMLDivElement>(null);
    const [query, setQueryState] = useState(lastQuery);
    const setQuery = (q: string) => {
        lastQuery = q;
        setQueryState(q);
    };

    const catalogQuery = useDebugCatalog(target);
    const catalog = catalogQuery.data;
    const collections = useDebugCollections().data ?? null;
    const recent = useDebugWorkspaceSelector((s) => s.ws.recent_actions);

    const rows = useMemo(
        () => buildPaletteRows({ actions: catalog?.actions ?? [], collections, query, recent }),
        [catalog, collections, query, recent],
    );

    // 高亮按行的 key 记：数据后到（收藏晚一步读出来）时还指着同一行；指的行没了就回到第一条
    const [activeKey, setActiveKey] = useState<string | null>(null);
    const keyIndex = activeKey === null ? -1 : rows.findIndex((r) => r.key === activeKey);
    const activeIndex =
        keyIndex >= 0 && isSelectable(rows[keyIndex]) ? keyIndex : firstSelectable(rows);
    const active = activeIndex >= 0 ? rows[activeIndex] : undefined;

    const virtualizer = useVirtualizer({
        count: rows.length,
        getScrollElement: () => listRef.current,
        estimateSize: (i) => {
            const kind = rows[i]?.kind;
            return kind === 'header' ? HEADER_HEIGHT : kind === 'more' ? MORE_HEIGHT : ROW_HEIGHT;
        },
        getItemKey: (i) => rows[i]?.key ?? i,
        overscan: 8,
        paddingStart: 4,
        paddingEnd: 4,
    });

    // 换了搜索词：回到顶上、高亮第一条
    useEffect(() => {
        setActiveKey(null);
        if (listRef.current) listRef.current.scrollTop = 0;
    }, [query]);

    // 打开时聚焦并整段选中上次的字
    useEffect(() => {
        const frame = requestAnimationFrame(() => {
            inputRef.current?.focus();
            inputRef.current?.select();
        });
        return () => cancelAnimationFrame(frame);
    }, []);

    const moveTo = (index: number) => {
        const row = rows[index];
        if (!isSelectable(row)) return;
        setActiveKey(row.key);
        virtualizer.scrollToIndex(index, { align: 'auto' });
    };

    const choose = (row: SelectablePaletteRow, newTab: boolean) => {
        const prevTab = debugWorkspaceStore.getSnapshot().ws.active_tab;
        let tabId: string;
        if (row.kind === 'saved') {
            const req = row.request;
            tabId = debugWorkspaceStore.openAction(req.action, {
                newTab,
                paramsText: paramsTextOf(req.params),
            });
            debugWorkspaceStore.setTabChannel(tabId, req.channel);
        } else {
            const name = row.kind === 'action' ? row.action.name : row.name;
            tabId = debugWorkspaceStore.openAction(name, { newTab });
            debugWorkspaceStore.pushRecent(name);
        }
        // openAction 总是激活落点的标签：回了别的标签上，就说明旧标签（连同打开面板前有焦点的元素）要卸载了
        if (tabId !== prevTab) onOpenedOtherTab();
        onClose();
    };

    // 行随时在算（收藏不依赖 Bot、也不等目录，算出来的行不一定显示着），但键盘只能作用在
    // 真的显示出来的列表上：没选 Bot、目录在读 / 读失败、什么都没搜到时显示的是提示、骨架或
    // 「重试」，这时按键只拦默认行为、什么都不执行。判定和下面渲染列表用的是同一个 hasList
    const hasList = !!target && !!catalog && rows.length > 0;

    const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
        if (e.nativeEvent.isComposing) return;
        switch (e.key) {
            case 'ArrowDown':
            case 'ArrowUp': {
                e.preventDefault();
                if (!hasList || activeIndex < 0) return;
                moveTo(stepSelectable(rows, activeIndex, e.key === 'ArrowDown' ? 1 : -1));
                return;
            }
            case 'PageDown':
            case 'PageUp': {
                e.preventDefault();
                if (!hasList || activeIndex < 0) return;
                const dir = e.key === 'PageDown' ? 1 : -1;
                let i = activeIndex;
                for (let n = 0; n < PAGE_STEP; n += 1) i = stepSelectable(rows, i, dir);
                moveTo(i);
                return;
            }
            case 'Enter': {
                e.preventDefault();
                if (!hasList) return;
                if (isSelectable(active)) choose(active, e.ctrlKey || e.metaKey);
            }
        }
    };

    const listId = 'debug-palette-list';
    const rowId = (key: string) => `debug-palette-${key.replace(/[^\w-]/g, '_')}`;
    const backend = target?.backend ?? null;

    let body: React.ReactNode;
    if (!target) {
        body = (
            <PaletteMessage
                icon={Search}
                title="先在顶栏选一个 Bot"
                hint="命令面板搜的是这个 Bot 的接口目录。"
            />
        );
    } else if (!catalog && catalogQuery.isError) {
        body = (
            <PaletteMessage
                icon={FileQuestion}
                title="读不到接口目录"
                hint={catalogQuery.error?.message}
                action={
                    <Button
                        size="sm"
                        variant="secondary"
                        onClick={() => void catalogQuery.refetch()}
                    >
                        <RefreshCw size={13} aria-hidden />
                        重试
                    </Button>
                }
            />
        );
    } else if (!catalog) {
        body = <PaletteSkeleton />;
    } else if (rows.length === 0) {
        body = (
            <PaletteMessage
                icon={FileQuestion}
                title={`没有找到「${query.trim()}」`}
                hint="换个关键词试试：接口名、中文简介、别名都能搜。目录外的接口名直接敲完整（比如 get_xxx）就能打开。"
            />
        );
    } else {
        body = (
            <div className="relative w-full" style={{ height: virtualizer.getTotalSize() }}>
                {virtualizer.getVirtualItems().map((v) => {
                    const row = rows[v.index];
                    if (!row) return null;
                    return (
                        <div
                            key={v.key}
                            className="absolute left-0 top-0 w-full px-1.5"
                            style={{ height: v.size, transform: `translateY(${v.start}px)` }}
                        >
                            <PaletteRowView
                                row={row}
                                id={rowId(row.key)}
                                active={v.index === activeIndex}
                                backend={backend}
                                onHover={() =>
                                    v.index !== activeIndex &&
                                    isSelectable(row) &&
                                    setActiveKey(row.key)
                                }
                                onChoose={choose}
                            />
                        </div>
                    );
                })}
            </div>
        );
    }

    return (
        <div className="flex flex-col">
            <div className="flex items-center gap-2 border-b border-border-subtle px-4">
                <Search
                    size={15}
                    strokeWidth={2}
                    aria-hidden
                    className="shrink-0 text-text-tertiary"
                />
                <input
                    ref={inputRef}
                    type="text"
                    role="combobox"
                    aria-label="搜索接口"
                    aria-expanded={hasList}
                    aria-controls={hasList ? listId : undefined}
                    aria-activedescendant={hasList && active ? rowId(active.key) : undefined}
                    aria-autocomplete="list"
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                    onKeyDown={onKeyDown}
                    placeholder="搜接口名、中文简介、别名，或收藏的名字"
                    spellCheck={false}
                    autoComplete="off"
                    className="h-12 min-w-0 flex-1 bg-transparent text-[14px] text-text outline-none placeholder:text-text-tertiary"
                />
                {catalogQuery.isFetching && catalog && (
                    <span className="shrink-0 text-2xs text-text-tertiary">刷新中…</span>
                )}
            </div>
            <div
                ref={listRef}
                id={hasList ? listId : undefined}
                role={hasList ? 'listbox' : undefined}
                aria-label={hasList ? '搜索结果' : undefined}
                // 高度固定：边输入边筛时对话框不跟着忽高忽低
                className="h-[min(420px,56vh)] overflow-y-auto overscroll-contain"
            >
                {body}
            </div>
            <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-t border-border-subtle px-4 py-2 text-[11px] text-text-tertiary">
                <span className="inline-flex items-center gap-1">
                    <Kbd>↑↓</Kbd> 选择
                </span>
                <span className="inline-flex items-center gap-1">
                    <Kbd>
                        <CornerDownLeft size={10} aria-hidden />
                    </Kbd>
                    打开
                </span>
                <span className="inline-flex items-center gap-1">
                    <Kbd>{MOD_KEY_LABEL} ↵</Kbd> 新标签打开
                </span>
                <span className="inline-flex items-center gap-1">
                    <Kbd>Esc</Kbd> 关闭
                </span>
                <span className="ml-auto">只打开，不会发送</span>
            </div>
        </div>
    );
}

function Kbd({ children }: { children: React.ReactNode }) {
    return (
        <kbd className="inline-flex h-4 min-w-4 items-center justify-center rounded-xs border border-border-subtle bg-inset px-1 font-mono text-[10px] leading-none text-text-secondary">
            {children}
        </kbd>
    );
}

function PaletteRowView({
    row,
    id,
    active,
    backend,
    onHover,
    onChoose,
}: {
    row: PaletteRow;
    id: string;
    active: boolean;
    backend: BackendType | null;
    onHover: () => void;
    onChoose: (row: SelectablePaletteRow, newTab: boolean) => void;
}) {
    if (row.kind === 'header') {
        return (
            <div
                aria-hidden
                className="flex h-full items-end px-2 pb-1 text-[11px] font-medium text-text-tertiary"
            >
                {row.label}
            </div>
        );
    }
    if (row.kind === 'more') {
        return (
            <div
                aria-hidden
                className="flex h-full items-center px-2 text-[11px] text-text-tertiary"
            >
                {row.text}
            </div>
        );
    }

    const optionProps = {
        id,
        role: 'option',
        'aria-selected': active,
        // 只认真的移动：面板打开时指针正好停在某一行上，不该把高亮抢过去
        onMouseMove: onHover,
        // 按下不抢走搜索框的焦点
        onMouseDown: (e: MouseEvent) => e.preventDefault(),
        onClick: (e: MouseEvent) => onChoose(row, e.ctrlKey || e.metaKey),
        onAuxClick: (e: MouseEvent) => {
            // 中键另开，和左栏一样
            if (e.button === 1) onChoose(row, true);
        },
        className: cn(
            'flex h-full cursor-pointer select-none items-center gap-2 rounded-sm px-2 transition-colors',
            active ? 'bg-inset ring-1 ring-inset ring-brand/40' : 'hover:bg-inset/60',
        ),
    } as const;

    if (row.kind === 'free') {
        return (
            <div {...optionProps} aria-label={`打开目录外的接口 ${row.name}`}>
                <span
                    aria-hidden
                    className="h-[7px] w-[7px] shrink-0 rounded-full border border-text-tertiary"
                />
                <span className="min-w-0 truncate text-[12.5px] text-text-secondary">
                    打开目录外的接口 <code className="font-mono text-text">{row.name}</code>
                </span>
                <span className="ml-auto shrink-0 text-[11px] text-text-tertiary">
                    没有分级，按有副作用处理
                </span>
            </div>
        );
    }

    if (row.kind === 'saved') {
        const req = row.request;
        return (
            <div
                {...optionProps}
                aria-label={`打开收藏：${req.name}（${req.action}）${row.safety ? `，${SAFETY_LABEL[row.safety]}` : ''}`}
            >
                <span
                    aria-hidden
                    className={cn(
                        'h-[7px] w-[7px] shrink-0 rounded-full',
                        row.safety ? SAFETY_DOT_CLASS[row.safety] : 'bg-text-disabled',
                    )}
                />
                <Star size={12} strokeWidth={2.2} aria-hidden className="shrink-0 text-brand" />
                <span className="min-w-0 truncate text-[13px] text-text">
                    <span className="text-text-tertiary">打开收藏：</span>
                    {req.name}
                </span>
                <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-text-tertiary">
                    {req.action}
                </span>
                {row.folderName && (
                    <span className={cn(CHIP, 'bg-inset text-text-secondary')}>
                        {row.folderName}
                    </span>
                )}
            </div>
        );
    }

    const a = row.action;
    const only = backend ? onlyBackendLabel(a, backend) : null;
    return (
        <div
            {...optionProps}
            aria-label={`${a.name}${a.summary ? `，${a.summary}` : ''}，${SAFETY_LABEL[a.safety]}${a.supported ? '' : '，当前 Bot 不支持'}`}
            className={cn(optionProps.className, !a.supported && 'opacity-60')}
        >
            <span
                aria-hidden
                className={cn('h-[7px] w-[7px] shrink-0 rounded-full', SAFETY_DOT_CLASS[a.safety])}
            />
            <span className="min-w-0 shrink-0 truncate font-mono text-[12.5px] text-text">
                {a.name}
            </span>
            {/* 简介只吃剩下的地方，窄了先没它；徽章始终完整 */}
            <span className="min-w-0 flex-1 truncate text-[12px] text-text-tertiary">
                {a.summary}
            </span>
            {row.recent && <span className={cn(CHIP, 'bg-inset text-text-secondary')}>最近</span>}
            {only && <span className={cn(CHIP, 'bg-info-soft text-info')}>{only}</span>}
            {a.param_diff && (
                <span className={cn(CHIP, 'bg-warning-soft text-warning')}>参数不同</span>
            )}
            {a.stream && <span className={cn(CHIP, 'bg-brand-soft text-brand')}>流式</span>}
            {!a.supported && <span className={cn(CHIP, 'bg-danger-soft text-danger')}>不支持</span>}
        </div>
    );
}

function PaletteMessage({
    icon: Icon,
    title,
    hint,
    action,
}: {
    icon: typeof Search;
    title: string;
    hint?: string;
    action?: React.ReactNode;
}) {
    return (
        <div className="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
            <span className="inline-flex h-9 w-9 items-center justify-center rounded-md bg-inset text-text-tertiary">
                <Icon size={17} strokeWidth={1.9} aria-hidden />
            </span>
            <p className="font-display text-[13px] font-semibold text-text-secondary">{title}</p>
            {hint && <p className="max-w-sm text-2xs leading-relaxed text-text-tertiary">{hint}</p>}
            {action}
        </div>
    );
}

/** 目录还在读：和真实行一样高的骨架，读到后列表不跳 */
function PaletteSkeleton() {
    return (
        <div aria-hidden className="px-3.5 py-2">
            {Array.from({ length: 9 }, (_, i) => (
                <div key={i} className="flex items-center gap-2" style={{ height: ROW_HEIGHT }}>
                    <Shimmer height={7} className="w-[7px] shrink-0 !rounded-full" />
                    <Shimmer
                        height={11}
                        className={i % 3 === 0 ? 'w-32' : i % 3 === 1 ? 'w-44' : 'w-36'}
                    />
                    <Shimmer height={9} className="w-24 opacity-60" />
                </div>
            ))}
        </div>
    );
}
