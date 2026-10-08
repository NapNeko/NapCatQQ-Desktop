// 左栏「接口」：当前 Bot 的接口目录。
//
// 没搜索时按分类分组（分类可折叠，当前 Bot 不支持的收进最底下默认折叠的一段）；有搜索词时是一张
// 按匹配度排好的平铺结果。两种都是同一个定高虚拟列表，分类标题也是列表里的一行，所以折叠时不必
// 给整段量高度，行的移动用 FLIP 走 transform。
//
// 键盘：`/` 聚焦搜索框；搜索框里 ↑↓ 移动高亮、回车打开高亮的那个（Ctrl / ⌘+回车另开标签）；
// 没搜索词时 ↓ 进入列表。列表里 ↑↓ Home End 移动，→ ← 展开 / 收起分类，回车打开，直接打字回到搜索框。

import {
    memo,
    useCallback,
    useEffect,
    useId,
    useLayoutEffect,
    useMemo,
    useRef,
    useState,
} from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { Info, ListTree, RefreshCw, SearchX } from 'lucide-react';
import { Button, Spinner } from '../../../shared/ui';
import { useDebugCatalog } from '../../../hooks/debug/useDebugCatalog';
import {
    debugWorkspaceStore,
    useDebugWorkspaceSelector,
} from '../../../hooks/debug/debugWorkspaceStore';
import { useScrollMemory } from '../../../hooks/debug/debugScrollMemory';
import { pushInfoBar } from '../../../hooks/ui/globalInfoBarStore';
import { pushErrorBar } from '../../../hooks/ui/pushErrorBar';
import { errorText } from '../../../core/domain/errors';
import { groupActions, searchActions } from '../../../core/domain/debug/catalogView';
import type { DebugActionSummary } from '../../../core/ipc/generated/debug/DebugActionSummary';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import { revealLeftSearch, setLeftSearchOpen, useLeftSearch } from '../leftPanels';
import {
    CATALOG_HEADER_HEIGHT,
    CATALOG_ROW_HEIGHT,
    CatalogActionRow,
    CatalogHeaderRow,
} from './CatalogRow';
import {
    PanelMessage,
    PanelSearch,
    PanelSearchRow,
    SkeletonRows,
    useSlashFocus,
} from './panelParts';
import { useFlip } from './useFlip';

// 搜索词和哪些分类收起了：纯界面状态，不落盘；切到别的面板 / 路由再回来还在
let savedQuery = '';
const UNSUPPORTED_GROUP = 'unsupported';
let savedClosed: ReadonlySet<string> = new Set([UNSUPPORTED_GROUP]);

type CatalogItem =
    | {
          kind: 'header';
          key: string;
          groupId: string;
          label: string;
          count: number | null;
          open: boolean;
          muted: boolean;
      }
    | {
          kind: 'action';
          key: string;
          action: DebugActionSummary;
          groupKey: string | null;
          nested: boolean;
      };

const actionKey = (name: string) => `a:${name}`;
const headerKey = (groupId: string) => `h:${groupId}`;

function buildItems(
    actions: DebugActionSummary[],
    query: string,
    recent: string[],
    closed: ReadonlySet<string>,
): CatalogItem[] {
    if (query.trim() !== '') {
        return searchActions(actions, query, recent).map((action) => ({
            kind: 'action',
            key: actionKey(action.name),
            action,
            groupKey: null,
            nested: false,
        }));
    }
    const { groups, unsupported } = groupActions(actions);
    const items: CatalogItem[] = [];
    const pushGroup = (
        groupId: string,
        label: string,
        list: DebugActionSummary[],
        muted: boolean,
        count: number | null,
    ) => {
        const open = !closed.has(groupId);
        const key = headerKey(groupId);
        items.push({ kind: 'header', key, groupId, label, count, open, muted });
        if (!open) return;
        for (const action of list) {
            items.push({
                kind: 'action',
                key: actionKey(action.name),
                action,
                groupKey: key,
                nested: true,
            });
        }
    };
    for (const g of groups) pushGroup(g.category, g.label, g.actions, false, g.actions.length);
    if (unsupported.length > 0) {
        pushGroup(
            UNSUPPORTED_GROUP,
            `当前 Bot 不支持（${unsupported.length}）`,
            unsupported,
            true,
            null,
        );
    }
    return items;
}

function openAction(name: string, newTab: boolean): void {
    debugWorkspaceStore.openAction(name, { newTab });
    debugWorkspaceStore.pushRecent(name);
}

async function copyName(name: string): Promise<void> {
    try {
        await navigator.clipboard.writeText(name);
        pushInfoBar({
            key: 'debug-copy',
            tone: 'info',
            title: '已复制接口名',
            content: name,
            autoDismissMs: 2000,
        });
    } catch (err) {
        pushErrorBar({ key: 'debug-copy', title: '复制失败', raw: errorText(err) });
    }
}

export const CatalogPanel = memo(function CatalogPanel({ target }: { target: DebugTarget | null }) {
    const rootRef = useRef<HTMLDivElement>(null);
    const inputRef = useRef<HTMLInputElement>(null);
    const listRef = useRef<HTMLDivElement>(null);
    const baseId = useId();

    const catalogQuery = useDebugCatalog(target);
    const catalog = catalogQuery.data;
    const recent = useDebugWorkspaceSelector((s) => s.ws.recent_actions);
    const currentAction = useDebugWorkspaceSelector(
        (s) => s.ws.tabs.find((t) => t.id === s.ws.active_tab)?.action ?? null,
    );

    const [query, setQueryState] = useState(savedQuery);
    const [closed, setClosedState] = useState<ReadonlySet<string>>(savedClosed);
    const searching = query.trim() !== '';

    const items = useMemo(
        () => (catalog ? buildItems(catalog.actions, query, recent, closed) : []),
        [catalog, query, recent, closed],
    );

    // 高亮行按键记，不按下标：折叠 / 列表刷新后还指着同一个接口
    const [activeKey, setActiveKey] = useState<string | null>(null);
    const activeIndex = activeKey === null ? -1 : items.findIndex((i) => i.key === activeKey);
    const [listFocused, setListFocused] = useState(false);
    const [searchFocused, setSearchFocused] = useState(false);
    // 鼠标点的行不画键盘高亮框；按了方向键之后才画
    const [keyboardNav, setKeyboardNav] = useState(false);

    const virtualizer = useVirtualizer({
        count: items.length,
        getScrollElement: () => listRef.current,
        estimateSize: (i) =>
            items[i]?.kind === 'header' ? CATALOG_HEADER_HEIGHT : CATALOG_ROW_HEIGHT,
        getItemKey: (i) => items[i]?.key ?? i,
        overscan: 10,
        paddingStart: 4,
        paddingEnd: 8,
    });

    useScrollMemory('catalog', listRef);
    useSlashFocus(inputRef, rootRef, () => revealLeftSearch('catalog'));
    const flip = useFlip(listRef, items);

    const setQuery = useCallback((q: string) => {
        savedQuery = q;
        setQueryState(q);
    }, []);

    // ---- 搜索条的收放：开关在标题行（分段切换旁）的图标按钮上，状态见 leftPanels
    const { open: searchOpen, focusNonce } = useLeftSearch('catalog');
    // 搜索词跟着会话走：带着词进来时搜索条也得是开的，不然列表筛着却找不到在哪改
    useLayoutEffect(() => {
        if (savedQuery.trim() !== '') setLeftSearchOpen('catalog', true);
    }, []);
    // 条从外面关上了：词一起清掉，理由同上
    const prevSearchOpen = useRef(searchOpen);
    useLayoutEffect(() => {
        const was = prevSearchOpen.current;
        prevSearchOpen.current = searchOpen;
        if (was && !searchOpen) setQuery('');
    }, [searchOpen, setQuery]);
    // 点图标 / 按 / 打开（nonce +1）时聚焦输入框；挂载时自己同步开条不算，不抢焦点
    const seenFocusNonce = useRef(focusNonce);
    useLayoutEffect(() => {
        if (focusNonce === seenFocusNonce.current) return;
        seenFocusNonce.current = focusNonce;
        inputRef.current?.focus();
    }, [focusNonce]);
    const closeSearch = useCallback(() => setLeftSearchOpen('catalog', false), []);

    // 搜索词变了：回到顶上，高亮第一个结果（回车就开它）；清空后不留高亮
    const firstQueryRun = useRef(true);
    useEffect(() => {
        if (firstQueryRun.current) {
            firstQueryRun.current = false;
            return;
        }
        if (listRef.current) listRef.current.scrollTop = 0;
    }, [query]);
    const firstResultKey = searching ? (items[0]?.key ?? null) : null;
    useEffect(() => {
        setActiveKey(firstResultKey);
        setKeyboardNav(false);
    }, [query, firstResultKey]);

    const toggleGroup = useCallback(
        (groupId: string) => {
            flip();
            setClosedState((prev) => {
                const next = new Set(prev);
                if (next.has(groupId)) next.delete(groupId);
                else next.add(groupId);
                savedClosed = next;
                return next;
            });
        },
        [flip],
    );

    const onOpen = useCallback((name: string, newTab: boolean) => {
        setActiveKey(actionKey(name));
        setKeyboardNav(false);
        openAction(name, newTab);
    }, []);

    const moveTo = (index: number) => {
        if (items.length === 0) return;
        const i = Math.max(0, Math.min(items.length - 1, index));
        setActiveKey(items[i].key);
        setKeyboardNav(true);
        virtualizer.scrollToIndex(i, { align: 'auto' });
    };

    const activate = (item: CatalogItem | undefined, newTab: boolean) => {
        if (!item) return;
        if (item.kind === 'header') toggleGroup(item.groupId);
        else openAction(item.action.name, newTab);
    };

    const onSearchKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
        if (e.nativeEvent.isComposing) return;
        if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
            if (items.length === 0) return;
            e.preventDefault();
            if (!searching) {
                // 分组视图里 ↓ 进列表，焦点跟过去
                listRef.current?.focus();
                moveTo(activeIndex < 0 ? 0 : activeIndex);
                return;
            }
            moveTo(activeIndex < 0 ? 0 : activeIndex + (e.key === 'ArrowDown' ? 1 : -1));
        } else if (e.key === 'Enter') {
            const item = items[activeIndex] ?? (searching ? items[0] : undefined);
            if (item?.kind !== 'action') return;
            e.preventDefault();
            activate(item, e.ctrlKey || e.metaKey);
        }
    };

    const onListKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
        // 焦点在列表本身（高亮靠 aria-activedescendant）。行上的右键菜单是门户，里面的按键会顺着 React 树冒到这里，
        // 回车会打开另一行，只认列表自己收到的键
        if (e.target !== e.currentTarget) return;
        if (e.nativeEvent.isComposing || e.altKey) return;
        const item = items[activeIndex];
        switch (e.key) {
            case 'ArrowDown':
                e.preventDefault();
                moveTo(activeIndex < 0 ? 0 : activeIndex + 1);
                return;
            case 'ArrowUp':
                e.preventDefault();
                if (activeIndex <= 0) {
                    inputRef.current?.focus();
                    return;
                }
                moveTo(activeIndex - 1);
                return;
            case 'Home':
                e.preventDefault();
                moveTo(0);
                return;
            case 'End':
                e.preventDefault();
                moveTo(items.length - 1);
                return;
            case 'ArrowRight':
                if (item?.kind !== 'header') return;
                e.preventDefault();
                if (item.open) moveTo(activeIndex + 1);
                else toggleGroup(item.groupId);
                return;
            case 'ArrowLeft':
                if (!item) return;
                e.preventDefault();
                if (item.kind === 'header') {
                    if (item.open) toggleGroup(item.groupId);
                } else if (item.groupKey) {
                    const parent = items.findIndex((i) => i.key === item.groupKey);
                    if (parent >= 0) moveTo(parent);
                }
                return;
            case 'Enter':
            case ' ':
                if (!item) return;
                e.preventDefault();
                activate(item, e.ctrlKey || e.metaKey);
                return;
            default:
                // 在列表里直接打字：回到搜索框接着输入（条收着时先展开）
                if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && e.key !== '/') {
                    e.preventDefault();
                    setQuery(query + e.key);
                    revealLeftSearch('catalog');
                }
        }
    };

    const rowId = (key: string) => `${baseId}-${key}`;
    const showActive = (listFocused && keyboardNav) || (searchFocused && searching);
    const activeDescendant =
        showActive && activeIndex >= 0 ? rowId(items[activeIndex].key) : undefined;

    let message: React.ReactNode = null;
    if (!target) {
        message = (
            <PanelMessage
                icon={ListTree}
                title="还没选 Bot"
                hint="先在顶栏选一个 Bot，这里列出它能调的接口。"
            />
        );
    } else if (!catalog && catalogQuery.isError) {
        message = (
            <PanelMessage
                icon={ListTree}
                tone="danger"
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
        message = <SkeletonRows rows={12} rowHeight={CATALOG_ROW_HEIGHT} />;
    } else if (catalog.actions.length === 0) {
        message = (
            <PanelMessage
                icon={ListTree}
                title="目录是空的"
                hint="上游没有报告任何接口。可以直接在中栏输入接口名调用。"
            />
        );
    } else if (items.length === 0) {
        message = (
            <PanelMessage
                icon={SearchX}
                title={`没有匹配「${query.trim()}」的接口`}
                hint="按名字、别名或中文简介搜。目录里没有的接口也可以在中栏直接输入名字调用。"
            />
        );
    }
    const showList = message === null && !!catalog;

    // 滚动容器一直挂着（加载中、没结果时也在）：滚动记忆和虚拟列表都认这一个元素，状态切换时不丢
    const body = (
        <div
            ref={listRef}
            role={showList ? 'tree' : undefined}
            id={`${baseId}-tree`}
            aria-label={showList ? '接口目录' : undefined}
            tabIndex={showList ? 0 : -1}
            aria-activedescendant={showList ? activeDescendant : undefined}
            onKeyDown={showList ? onListKeyDown : undefined}
            onFocus={() => setListFocused(true)}
            onBlur={() => setListFocused(false)}
            className="flex min-h-0 flex-1 flex-col overflow-y-auto px-1.5 outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-brand/40"
        >
            {showList && catalog ? (
                <div
                    className="relative w-full shrink-0"
                    style={{ height: virtualizer.getTotalSize() }}
                >
                    {virtualizer.getVirtualItems().map((v) => {
                        const item = items[v.index];
                        if (!item) return null;
                        const active = showActive && v.index === activeIndex;
                        return (
                            <div
                                key={item.key}
                                className="absolute left-0 top-0 w-full"
                                style={{ height: v.size, transform: `translateY(${v.start}px)` }}
                            >
                                {item.kind === 'header' ? (
                                    <CatalogHeaderRow
                                        id={rowId(item.key)}
                                        label={item.label}
                                        count={item.count}
                                        open={item.open}
                                        muted={item.muted}
                                        active={active}
                                        flipKey={item.key}
                                        onToggle={() => toggleGroup(item.groupId)}
                                    />
                                ) : (
                                    <CatalogActionRow
                                        id={rowId(item.key)}
                                        action={item.action}
                                        backend={catalog.backend}
                                        nested={item.nested}
                                        active={active}
                                        current={item.action.name === currentAction}
                                        onOpen={onOpen}
                                        onCopyName={copyName}
                                    />
                                )}
                            </div>
                        );
                    })}
                </div>
            ) : (
                message
            )}
        </div>
    );

    const resultCount = searching && catalog ? items.length : undefined;

    return (
        <div ref={rootRef} className="relative flex min-h-0 flex-1 flex-col">
            {searchOpen && (
                <PanelSearchRow className="gap-2">
                    <PanelSearch
                        ref={inputRef}
                        value={query}
                        onChange={setQuery}
                        placeholder="搜接口（按 / 聚焦）"
                        ariaLabel="搜索接口"
                        trailing={resultCount}
                        onKeyDown={onSearchKeyDown}
                        onFocus={() => setSearchFocused(true)}
                        onBlur={() => setSearchFocused(false)}
                        onRequestClose={closeSearch}
                        controls={showList ? `${baseId}-tree` : undefined}
                        activeDescendant={searchFocused ? activeDescendant : undefined}
                    />
                    {catalog && catalogQuery.isFetching && (
                        <Spinner size="xs" label="正在刷新接口目录" />
                    )}
                </PanelSearchRow>
            )}
            {/* 搜索条收着时的刷新指示：不占一行，浮在列表角上 */}
            {!searchOpen && catalog && catalogQuery.isFetching && (
                <span className="pointer-events-none absolute right-2.5 top-1.5 z-10">
                    <Spinner size="xs" label="正在刷新接口目录" />
                </span>
            )}
            {target && catalog?.source === 'snapshot' && (
                <div className="flex shrink-0 items-start gap-1.5 border-b border-border-subtle/70 bg-warning-soft/40 px-2.5 py-1.5 text-2xs leading-snug text-text-secondary">
                    <Info
                        size={12}
                        strokeWidth={2.2}
                        aria-hidden
                        className="mt-px shrink-0 text-warning"
                    />
                    <span>按内置目录显示（{catalog.snapshot_version}），可能和你的版本不同</span>
                    <button
                        type="button"
                        onClick={() => void catalogQuery.refetch()}
                        disabled={catalogQuery.isFetching}
                        className="ml-auto shrink-0 font-medium text-brand transition-colors hover:underline disabled:pointer-events-none disabled:opacity-50"
                    >
                        重新获取
                    </button>
                </div>
            )}
            {body}
        </div>
    );
});
