// 左栏「历史」：调过的请求，新的在上。
//
// 按成败筛选、搜索（接口名或参数里的文字）、可只看当前 Bot；一次取 100 条，「加载更多」再取 100 条
// （同一个查询把 limit 加大，列表不重排、不跳）。单击一条在新标签里打开：参数写进编辑器，
// 当时的回包直接显示在响应面板里，不重新发。收藏、复制参数需要完整记录，先取回来再做；
// 收藏弹的是和中栏 ☆ 同一个起名框。键盘上高亮一行后：回车打开、S 收藏、C 复制参数。

import { memo, useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { History, RefreshCw, SearchX, Trash2, UserRound } from 'lucide-react';
import { cn } from '../../../shared/utils/cn';
import { Button, Spinner, Tooltip, TooltipContent, TooltipTrigger } from '../../../shared/ui';
import { useClearHistory, useDebugHistory, useHistoryEntry } from '../../../hooks/debug/useDebugHistory';
import { useDebugCatalog } from '../../../hooks/debug/useDebugCatalog';
import { debugWorkspaceStore } from '../../../hooks/debug/debugWorkspaceStore';
import { useScrollMemory } from '../../../hooks/debug/debugScrollMemory';
import { pushInfoBar } from '../../../hooks/ui/globalInfoBarStore';
import { pushErrorBar } from '../../../hooks/ui/pushErrorBar';
import { useNowMs } from '../../../hooks/ui/useNowMs';
import { errorText } from '../../../core/domain/errors';
import { targetDisplayName } from '../../../core/domain/debug/targetGroups';
import type { DebugHistoryEntry } from '../../../core/ipc/generated/debug/DebugHistoryEntry';
import type { DebugHistoryQuery } from '../../../core/ipc/generated/debug/DebugHistoryQuery';
import type { DebugHistorySummary } from '../../../core/ipc/generated/debug/DebugHistorySummary';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import { revealLeftSearch, setLeftSearchOpen, useLeftSearch } from '../leftPanels';
import { SaveRequestDialog } from '../SaveRequestDialog';
import { suggestedRequestName } from '../../../core/domain/debug/collectionsOps';
import { paramsTextOf, replayResponse } from '../../../core/domain/debug/historyReplay';
import { HISTORY_ROW_HEIGHT, HistoryRow, type HistoryRowIntent } from './HistoryRow';
import { ConfirmDialog, IconAction, PanelMessage, PanelSearch, PanelSearchRow, Segmented, SkeletonRows, useSlashFocus, type ConfirmRequest } from './panelParts';

export const HISTORY_PAGE_SIZE = 100;
/** 列表的名字里写上键盘用法：行上的悬停按钮不进 Tab 顺序，读屏用户靠这句知道还有 S / C */
const HISTORY_LIST_LABEL = '调用历史（↑↓ 选，回车打开，S 收藏，C 复制参数）';
const SEARCH_DEBOUNCE_MS = 250;
const FOOTER_HEIGHT = 44;

type OkFilter = 'all' | 'ok' | 'fail';
const OK_FILTERS: ReadonlyArray<{ id: OkFilter; label: string }> = [
    { id: 'all', label: '全部' },
    { id: 'ok', label: '成功' },
    { id: 'fail', label: '失败' },
];

// 筛选条件：纯界面状态，切面板 / 切路由回来还在
let saved = { ok: 'all' as OkFilter, text: '', onlyBot: false };

async function copyText(text: string, title: string): Promise<void> {
    try {
        await navigator.clipboard.writeText(text);
        pushInfoBar({ key: 'debug-copy', tone: 'info', title, autoDismissMs: 2000 });
    } catch (err) {
        pushErrorBar({ key: 'debug-copy', title: '复制失败', raw: errorText(err) });
    }
}

function openInNewTab(entry: DebugHistoryEntry): void {
    const tabId = debugWorkspaceStore.openAction(entry.action, { newTab: true, paramsText: paramsTextOf(entry.params) });
    debugWorkspaceStore.setRun(tabId, {
        last: { response: replayResponse(entry), at: entry.at_ms, botId: entry.bot_id, action: entry.action },
    });
}

export const HistoryPanel = memo(function HistoryPanel({ target }: { target: DebugTarget | null }) {
    const rootRef = useRef<HTMLDivElement>(null);
    const inputRef = useRef<HTMLInputElement>(null);
    const listRef = useRef<HTMLDivElement>(null);
    const baseId = useId();

    const [ok, setOkState] = useState<OkFilter>(saved.ok);
    const [text, setTextState] = useState(saved.text);
    const [debouncedText, setDebouncedText] = useState(saved.text);
    const [onlyBot, setOnlyBotState] = useState(saved.onlyBot);
    const [pages, setPages] = useState(1);

    const setOk = (v: OkFilter) => {
        saved = { ...saved, ok: v };
        setOkState(v);
    };
    const setText = useCallback((v: string) => {
        saved = { ...saved, text: v };
        setTextState(v);
    }, []);
    const setOnlyBot = (v: boolean) => {
        saved = { ...saved, onlyBot: v };
        setOnlyBotState(v);
    };

    useEffect(() => {
        if (text === debouncedText) return;
        // 清空立刻生效；打字时等停一下再查
        if (text.trim() === '') {
            setDebouncedText(text);
            return;
        }
        const t = window.setTimeout(() => setDebouncedText(text), SEARCH_DEBOUNCE_MS);
        return () => window.clearTimeout(t);
    }, [text, debouncedText]);

    const botId = onlyBot && target ? target.bot_id : null;
    const trimmed = debouncedText.trim();
    // 筛选一变就回到第一页
    const filterKey = `${ok}|${trimmed}|${botId ?? ''}`;
    const lastFilterKey = useRef(filterKey);
    if (lastFilterKey.current !== filterKey) {
        lastFilterKey.current = filterKey;
        if (pages !== 1) setPages(1);
    }

    const query: DebugHistoryQuery = useMemo(
        () => ({
            action: null,
            bot_id: botId,
            ok: ok === 'all' ? null : ok === 'ok',
            text: trimmed === '' ? null : trimmed,
            limit: HISTORY_PAGE_SIZE * pages,
            offset: 0,
        }),
        [botId, ok, trimmed, pages],
    );
    const historyQuery = useDebugHistory(query);
    const page = historyQuery.data;
    const entries = useMemo(() => page?.entries ?? [], [page]);
    const total = page?.total ?? 0;
    const hasMore = entries.length < total;
    const filtered = ok !== 'all' || trimmed !== '' || botId !== null;

    const nowMs = useNowMs(true, 30_000);

    // 键盘高亮按记录 id 记：刷新、加载更多后还指着同一条
    const [activeKey, setActiveKey] = useState<string | null>(null);
    const activeIndex = activeKey === null ? -1 : entries.findIndex((e) => e.id === activeKey);
    const [listFocused, setListFocused] = useState(false);
    const [keyboardNav, setKeyboardNav] = useState(false);

    // ---- 需要完整记录的操作：先取回来（取过的有缓存），到了再做
    const [intent, setIntent] = useState<{ id: string; kind: HistoryRowIntent } | null>(null);
    const entryQuery = useHistoryEntry(intent?.id ?? null);
    const catalog = useDebugCatalog(target).data;
    // 收藏框：开着时记着是哪一条（关的动画期间内容不变）
    const [saving, setSaving] = useState<DebugHistoryEntry | null>(null);
    const [saveOpen, setSaveOpen] = useState(false);

    const perform = useCallback((kind: HistoryRowIntent, entry: DebugHistoryEntry) => {
        if (kind === 'open') {
            openInNewTab(entry);
            return;
        }
        if (kind === 'copy') {
            void copyText(paramsTextOf(entry.params), '已复制参数');
            return;
        }
        setSaving(entry);
        setSaveOpen(true);
    }, []);

    useEffect(() => {
        if (!intent) return;
        if (entryQuery.isError) {
            // 错误条由 hook 弹过了
            setIntent(null);
            return;
        }
        if (entryQuery.data === undefined) return;
        const entry = entryQuery.data;
        setIntent(null);
        if (!entry) {
            pushInfoBar({ key: 'debug-history-gone', tone: 'warning', title: '这条记录已经不在了', content: '历史可能刚被清空或压缩过', autoDismissMs: 3000 });
            void historyQuery.refetch();
            return;
        }
        perform(intent.kind, entry);
    }, [intent, entryQuery.data, entryQuery.isError, perform, historyQuery]);

    const onIntent = useCallback((entry: DebugHistorySummary, kind: HistoryRowIntent) => {
        setActiveKey(entry.id);
        setIntent({ id: entry.id, kind });
    }, []);
    const onCopyAction = useCallback((name: string) => void copyText(name, '已复制接口名'), []);

    // ---- 清空
    const clearHistory = useClearHistory();
    const [confirm, setConfirm] = useState<ConfirmRequest | null>(null);
    // 清空删的是全部历史，不看筛选；筛选着的时候只知道筛出来的条数，要把这点说清楚
    const askClear = () =>
        setConfirm({
            title: '清空调用历史？',
            description: filtered
                ? `会删掉全部调用历史（不只是现在筛选出的 ${total} 条），连同当时的响应，不能撤销。收藏不受影响。`
                : `全部 ${total} 条记录连同当时的响应都会删掉，不能撤销。收藏不受影响。`,
            confirmLabel: '清空',
            onConfirm: () => clearHistory.mutate(),
        });

    // ---- 列表
    const rowCount = entries.length + (entries.length > 0 ? 1 : 0);

    const virtualizer = useVirtualizer({
        count: rowCount,
        getScrollElement: () => listRef.current,
        estimateSize: (i) => (i < entries.length ? HISTORY_ROW_HEIGHT : FOOTER_HEIGHT),
        getItemKey: (i) => entries[i]?.id ?? '__footer',
        overscan: 8,
        paddingStart: 4,
    });

    useScrollMemory('history', listRef);
    useSlashFocus(inputRef, rootRef, () => revealLeftSearch('history'));

    // ---- 搜索条的收放：开关在标题行（分段切换旁）的图标按钮上，状态见 leftPanels
    const { open: searchOpen, focusNonce } = useLeftSearch('history');
    // 搜索词跟着会话走：带着词进来时搜索条也得是开的，不然列表筛着却找不到在哪改
    useLayoutEffect(() => {
        if (saved.text.trim() !== '') setLeftSearchOpen('history', true);
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);
    // 条从外面关上了：词一起清掉，理由同上
    const prevSearchOpen = useRef(searchOpen);
    useLayoutEffect(() => {
        const was = prevSearchOpen.current;
        prevSearchOpen.current = searchOpen;
        if (was && !searchOpen) setText('');
    }, [searchOpen, setText]);
    // 点图标 / 按 / 打开（nonce +1）时聚焦输入框；挂载时自己同步开条不算，不抢焦点
    const seenFocusNonce = useRef(focusNonce);
    useLayoutEffect(() => {
        if (focusNonce === seenFocusNonce.current) return;
        seenFocusNonce.current = focusNonce;
        inputRef.current?.focus();
    }, [focusNonce]);
    const closeSearch = useCallback(() => setLeftSearchOpen('history', false), []);

    // 换了筛选：回到顶上
    const firstFilterRun = useRef(true);
    useEffect(() => {
        if (firstFilterRun.current) {
            firstFilterRun.current = false;
            return;
        }
        if (listRef.current) listRef.current.scrollTop = 0;
    }, [filterKey]);

    const moveTo = (index: number) => {
        if (entries.length === 0) return;
        const i = Math.max(0, Math.min(entries.length - 1, index));
        setActiveKey(entries[i].id);
        setKeyboardNav(true);
        virtualizer.scrollToIndex(i, { align: 'auto' });
    };

    const onListKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
        // 只认列表自己收到的键：行上右键菜单（门户）里的按键也会顺着 React 树冒上来
        if (e.target !== e.currentTarget) return;
        if (e.nativeEvent.isComposing || e.altKey) return;
        switch (e.key) {
            case 'ArrowDown':
                e.preventDefault();
                moveTo(activeIndex < 0 ? 0 : activeIndex + 1);
                return;
            case 'ArrowUp':
                e.preventDefault();
                if (activeIndex <= 0) inputRef.current?.focus();
                else moveTo(activeIndex - 1);
                return;
            case 'Home':
                e.preventDefault();
                moveTo(0);
                return;
            case 'End':
                e.preventDefault();
                moveTo(entries.length - 1);
                return;
            case 'Enter': {
                const entry = entries[activeIndex];
                if (!entry) return;
                e.preventDefault();
                onIntent(entry, 'open');
                return;
            }
        }
        // 悬停按钮不进 Tab 顺序，键盘上收藏 / 复制参数走这两个键；带 Ctrl / ⌘ 的是系统快捷键（Ctrl+C），不拦
        if (e.ctrlKey || e.metaKey || e.shiftKey) return;
        const key = e.key.toLowerCase();
        if (key !== 's' && key !== 'c') return;
        const entry = entries[activeIndex];
        if (!entry) return;
        e.preventDefault();
        onIntent(entry, key === 's' ? 'save' : 'copy');
    };

    const onSearchKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
        if (e.key !== 'ArrowDown' || entries.length === 0) return;
        e.preventDefault();
        listRef.current?.focus();
        moveTo(activeIndex < 0 ? 0 : activeIndex);
    };

    const showActive = listFocused && keyboardNav;
    const rowId = (key: string) => `${baseId}-${key}`;

    let message: React.ReactNode = null;
    if (!page && historyQuery.isError) {
        message = (
            <PanelMessage
                icon={History}
                tone="danger"
                title="读不到调用历史"
                hint={historyQuery.error?.message}
                action={
                    <Button size="sm" variant="secondary" onClick={() => void historyQuery.refetch()}>
                        <RefreshCw size={13} aria-hidden />
                        重试
                    </Button>
                }
            />
        );
    } else if (!page) {
        message = <SkeletonRows rows={8} rowHeight={HISTORY_ROW_HEIGHT} />;
    } else if (entries.length === 0 && filtered) {
        message = (
            <PanelMessage
                icon={SearchX}
                title="没有符合条件的记录"
                action={
                    <Button
                        size="sm"
                        variant="ghost"
                        onClick={() => {
                            setOk('all');
                            setText('');
                            setOnlyBot(false);
                        }}
                    >
                        清除筛选
                    </Button>
                }
            />
        );
    } else if (entries.length === 0) {
        message = (
            <PanelMessage
                icon={History}
                title="还没有调用记录"
                hint="在中栏发出的请求、在聊天栏发的消息都会记在这里，连同当时的响应，点一下就能重新打开。"
            />
        );
    }
    const showList = message === null;

    return (
        <div ref={rootRef} className="flex min-h-0 flex-1 flex-col">
            {searchOpen && (
                <PanelSearchRow>
                    <PanelSearch
                        ref={inputRef}
                        value={text}
                        onChange={setText}
                        placeholder="搜接口名、参数"
                        ariaLabel="搜索调用历史"
                        onKeyDown={onSearchKeyDown}
                        onRequestClose={closeSearch}
                    />
                </PanelSearchRow>
            )}
            <div className="flex shrink-0 items-center gap-1.5 border-b border-border-subtle/70 px-2 py-1.5">
                <Segmented value={ok} options={OK_FILTERS} onChange={setOk} ariaLabel="按成败筛选" />
                {target && (
                    <Tooltip>
                        <TooltipTrigger asChild>
                            <button
                                type="button"
                                aria-pressed={onlyBot}
                                aria-label={`只看当前 Bot（${targetDisplayName(target)}）`}
                                onClick={() => setOnlyBot(!onlyBot)}
                                className={cn(
                                    'inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-sm transition-colors',
                                    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                                    onlyBot ? 'bg-brand-soft text-brand' : 'text-text-tertiary hover:bg-inset hover:text-text',
                                )}
                            >
                                <UserRound size={13} strokeWidth={2.2} aria-hidden />
                            </button>
                        </TooltipTrigger>
                        <TooltipContent side="bottom">
                            {onlyBot ? '正在只看' : '只看'}当前 Bot（{targetDisplayName(target)}）
                        </TooltipContent>
                    </Tooltip>
                )}
                {/* 总条数写在列表底部（「已显示全部 N 条」/「加载更多（还有 N 条）」），这一行窄栏里放不下；
                    清空挪到这行右端：搜索条收起来以后，它不用再独占一行 */}
                <span className="ml-auto flex shrink-0 items-center gap-1">
                    {historyQuery.isFetching && page && <Spinner size="xs" label="正在刷新调用历史" />}
                    <IconAction
                        icon={Trash2}
                        label="清空历史"
                        tone="danger"
                        size="md"
                        tooltipSide="bottom"
                        busy={clearHistory.isPending}
                        disabledReason={total === 0 && !filtered ? '没有记录' : null}
                        onClick={askClear}
                    />
                </span>
            </div>
            {/* 滚动容器一直挂着：滚动记忆和虚拟列表认的是同一个元素 */}
            <div
                ref={listRef}
                role={showList ? 'listbox' : undefined}
                aria-label={showList ? HISTORY_LIST_LABEL : undefined}
                tabIndex={showList ? 0 : -1}
                aria-activedescendant={showList && showActive && activeIndex >= 0 ? rowId(entries[activeIndex].id) : undefined}
                onKeyDown={showList ? onListKeyDown : undefined}
                onFocus={() => setListFocused(true)}
                onBlur={() => setListFocused(false)}
                className={cn(
                    'flex min-h-0 flex-1 flex-col overflow-y-auto px-1.5 outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-brand/40',
                    // 换筛选 / 翻页取数的途中旧列表还在，稍微淡一点说明在更新
                    historyQuery.isPlaceholderData && 'opacity-70 transition-opacity',
                )}
            >
                {showList ? (
                    <div className="relative w-full shrink-0" style={{ height: virtualizer.getTotalSize() }}>
                        {virtualizer.getVirtualItems().map((v) => {
                            const entry = entries[v.index];
                            return (
                                <div
                                    key={entry?.id ?? '__footer'}
                                    className="absolute left-0 top-0 w-full"
                                    style={{ height: v.size, transform: `translateY(${v.start}px)` }}
                                >
                                    {entry ? (
                                        <HistoryRow
                                            id={rowId(entry.id)}
                                            entry={entry}
                                            nowMs={nowMs}
                                            active={showActive && v.index === activeIndex}
                                            busy={intent?.id === entry.id}
                                            onIntent={onIntent}
                                            onCopyAction={onCopyAction}
                                        />
                                    ) : (
                                        <div className="flex h-full items-center justify-center">
                                            {hasMore ? (
                                                <Button
                                                    size="sm"
                                                    variant="ghost"
                                                    disabled={historyQuery.isFetching}
                                                    onClick={() => setPages((p) => p + 1)}
                                                >
                                                    {historyQuery.isFetching && <Spinner size="xs" label="正在加载" />}
                                                    加载更多（还有 {total - entries.length} 条）
                                                </Button>
                                            ) : (
                                                <span className="text-[11px] text-text-tertiary">已显示全部 {total} 条</span>
                                            )}
                                        </div>
                                    )}
                                </div>
                            );
                        })}
                    </div>
                ) : (
                    message
                )}
            </div>
            <ConfirmDialog request={confirm} onClose={() => setConfirm(null)} />
            {saving && (
                <SaveRequestDialog
                    open={saveOpen}
                    onOpenChange={setSaveOpen}
                    action={saving.action}
                    params={plainParams(saving.params)}
                    channel={saving.channel}
                    channelFrom="history"
                    suggestedName={suggestedRequestName(
                        saving.action,
                        catalog?.actions.find((a) => a.name === saving.action || a.aliases.includes(saving.action))?.summary,
                    )}
                />
            )}
        </div>
    );
});

/** 收藏只收对象参数；历史里记的万一不是对象（旧数据、手改过），交给收藏框按「参数有错」拦下 */
function plainParams(params: unknown): Record<string, unknown> | null {
    if (params === null || params === undefined) return {};
    return typeof params === 'object' && !Array.isArray(params) ? (params as Record<string, unknown>) : null;
}
