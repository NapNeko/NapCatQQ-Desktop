// 调试台页面（一级路由 'debug'）：顶栏选 Bot 和通道，下面三栏工作台——左栏接口 / 收藏 / 历史，
// 中栏请求与响应，右栏聊天。
//
// 页面本身不滚：高度来自路由容器（一路 flex-1 + min-h-0 撑下来），三栏各自在内部滚。
// 这里管的是跨栏的事：工作区载入 / 写盘、选中哪个 Bot（含从 Bot 卡片跳进来时带的那个）、
// 选中且在跑的 Bot 自动开始收事件、栏宽和收起、标签页快捷键、命令面板开关。

import {
    memo,
    useCallback,
    useEffect,
    useLayoutEffect,
    useMemo,
    useRef,
    useState,
    type KeyboardEvent as ReactKeyboardEvent,
    type ReactNode,
    type RefObject,
} from 'react';
import {
    AlertTriangle,
    Bot,
    FlaskConical,
    PanelLeftClose,
    PowerOff,
    RefreshCw,
    Search,
    X,
} from 'lucide-react';
import gsap from 'gsap';
import { cn } from '../../shared/utils/cn';
import {
    Button,
    PagePlaceholder,
    Spinner,
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from '../../shared/ui';
import { ActionMotionIcon, GsapPresence, type EnterFn, type ExitFn } from '../../shared/ui/motion';
import type { AppRoute } from '../../shared/components/next/Sidebar';
import {
    debugWorkspaceStore,
    flushWorkspace,
    useDebugChannelChoice,
    useDebugLayout,
    useDebugWorkspaceSelector,
    useSelectedDebugBot,
} from '../../hooks/debug/debugWorkspaceStore';
import { debugEventStore } from '../../hooks/debug/debugEventStore';
import {
    consumePendingDebugBot,
    getPendingDebugBot,
    usePendingDebugBot,
} from '../../hooks/debug/debugNav';
import { useDebugTargets } from '../../hooks/debug/useDebugTargets';
import { useDebugChannels } from '../../hooks/debug/useDebugChannels';
import { useDebugStorageNotices } from '../../hooks/debug/useDebugStorageNotices';
import { channelIdKey } from '../../hooks/debug/keys';
import { AUTO_CHANNEL } from '../../core/domain/debug/channelPick';
import { defaultTargetId } from '../../core/domain/debug/targetGroups';
import {
    LEFT_RAIL_WIDTH,
    UNSET_COLUMN_WIDTH,
    dragBounds,
    resolveColumns,
} from '../../core/domain/debug/workbenchLayout';
import type { DebugChannelChoice } from '../../core/ipc/generated/debug/DebugChannelChoice';
import type { DebugChannelId } from '../../core/ipc/generated/debug/DebugChannelId';
import type { DebugStorageNotice } from '../../core/ipc/generated/debug/DebugStorageNotice';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import {
    COLUMN_HEADER_CLASS,
    ColumnFrame,
    ColumnRail,
    ColumnSplitter,
    useSlideAfterShift,
} from './ColumnFrame';
import { CommandPalette } from './CommandPalette';
import { TopBar } from './TopBar';
import { useDebugShortcuts } from './debugShortcuts';
import {
    LEFT_PANELS,
    revealLeftSearch,
    setLeftSearchOpen,
    useLeftSearch,
    type DebugLeftPanel,
} from './leftPanels';
import { LeftColumn } from './left/LeftColumn';
import { CenterColumn } from './center/CenterColumn';
import { RightColumn } from './right/RightColumn';

const DEFAULT_CHOICE: DebugChannelChoice = { call: AUTO_CHANNEL, events: AUTO_CHANNEL };

// 左栏停在哪个面板：纯界面状态，不落盘，但切走路由再回来应该还在原处
let lastLeftPanel: DebugLeftPanel = 'catalog';
// 存储损坏的提示看过、关掉了，这次运行里就别每进一次页面弹一次
let storageNoticesDismissed = false;

export interface DebugConsolePageProps {
    onNavigate?: (route: AppRoute) => void;
}

export function DebugConsolePage({ onNavigate }: DebugConsolePageProps) {
    const rootRef = useRef<HTMLDivElement>(null);
    const loaded = useDebugWorkspaceSelector((s) => s.loaded);
    const selectedId = useSelectedDebugBot();
    const layout = useDebugLayout();
    const targetsQuery = useDebugTargets();
    const targets = targetsQuery.data;
    const pendingBot = usePendingDebugBot();
    const { refetch: refetchTargets } = targetsQuery;
    /** 判断「选中的 Bot 还在不在」时，手里的列表至少得新到这个时刻 */
    const targetsFreshSince = useRef(0);

    // ---- Bot 列表：页面没挂着时收不到 Bot 状态事件，缓存里的可能已经过时（刚启动的 Bot 还显示未运行、
    // 刚加的 Bot 不在列表里）。进页面先按当下刷一遍；已经在拉就不重复拉
    useEffect(() => {
        targetsFreshSince.current = Date.now();
        void refetchTargets({ cancelRefetch: false });
    }, [refetchTargets]);

    // ---- 工作区：进来读一次（store 自己保证只读一次），离开时把防抖里没写的立刻写掉
    useEffect(() => {
        void debugWorkspaceStore.load();
        return () => {
            void flushWorkspace();
        };
    }, []);

    // ---- 从 Bot 卡片 / 右键菜单跳进来：带着的 Bot 取走并选中。页面已经挂着时也会走到这里
    useEffect(() => {
        if (pendingBot === null) return;
        const id = consumePendingDebugBot();
        if (!id) return;
        debugWorkspaceStore.selectBot(id);
        // 带进来的可能是刚建、刚启动的 Bot：列表也要新到这一刻
        targetsFreshSince.current = Date.now();
        void refetchTargets({ cancelRefetch: false });
    }, [pendingBot, refetchTargets]);

    // ---- 没有有效的选中（第一次进、上次选的 Bot 被删了）：默认挑一个在跑的
    // 刷新失败也算有了结论，不然会一直停在一个不存在的 Bot 上
    const targetsSettledAt = Math.max(targetsQuery.dataUpdatedAt, targetsQuery.errorUpdatedAt);
    useEffect(() => {
        if (!loaded || !targets) return;
        // 读 store 里最新的：同一轮 effect 里上面刚按待选 Bot 选过，闭包里的 selectedId 还是旧的
        if (getPendingDebugBot() !== null) return;
        const current = debugWorkspaceStore.getSnapshot().ws.selected_bot;
        if (current !== null && targets.some((t) => t.bot_id === current)) return;
        // 手里的列表比进页面 / 刚跳进来那一刻还旧：等这一轮刷新回来再判断，别拿旧列表把刚带进来的 Bot 顶掉
        if (targetsSettledAt < targetsFreshSince.current) return;
        const next = defaultTargetId(targets);
        if (next !== current) debugWorkspaceStore.selectBot(next);
    }, [loaded, targets, selectedId, targetsSettledAt]);

    const selected = useMemo(
        () => targets?.find((t) => t.bot_id === selectedId) ?? null,
        [targets, selectedId],
    );
    const botId = selected?.bot_id ?? null;
    const running = selected?.running ?? false;
    const selfId = selected && selected.qq_id > 0 ? selected.qq_id : undefined;

    // ---- 通道：和 Bot 列表一样，进页面、换 Bot 时按当下刷一遍状态
    const channelsQuery = useDebugChannels(botId);
    const { refetch: refetchChannels } = channelsQuery;
    useEffect(() => {
        if (botId) void refetchChannels({ cancelRefetch: false });
    }, [botId, refetchChannels]);
    const retryChannels = useCallback(() => void refetchChannels(), [refetchChannels]);
    const choice = useDebugChannelChoice(botId) ?? DEFAULT_CHOICE;
    const choiceRef = useRef(choice);
    choiceRef.current = choice;
    const eventsKey = channelIdKey(choice.events);
    const onChoiceChange = useCallback(
        (next: DebugChannelChoice) => {
            if (botId) debugWorkspaceStore.setChannelChoice(botId, next);
        },
        [botId],
    );
    const [callChannelOpen, setCallChannelOpen] = useState(false);
    const revealCallChannel = useCallback(() => setCallChannelOpen(true), []);

    // ---- 事件接收：选中且在跑就开始收（Bot 从停到跑时 running 翻转，这里跟着重来）；
    // 换 Bot / 离开页面只是「不看了」，后端照样在收
    useEffect(() => {
        if (!botId) return;
        return () => debugEventStore.releaseView(botId);
    }, [botId]);

    useEffect(() => {
        if (!botId || !running) return;
        void debugEventStore.ensureReceiving(botId, choiceRef.current.events, selfId);
    }, [botId, running, eventsKey, selfId]);

    // ---- 右栏开关在顶栏上
    const [rightAppear, setRightAppear] = useState(false);
    const toggleRight = useCallback(() => {
        const collapsed = debugWorkspaceStore.getSnapshot().ws.layout.right_collapsed;
        setRightAppear(collapsed);
        debugWorkspaceStore.setLayout({ right_collapsed: !collapsed });
    }, []);

    // ---- 命令面板
    const [paletteOpen, setPaletteOpen] = useState(false);
    const openPalette = useCallback(() => setPaletteOpen(true), []);

    const selectBot = useCallback((id: string) => debugWorkspaceStore.selectBot(id), []);
    const manageBots = useMemo(
        () => (onNavigate ? () => onNavigate('bots') : undefined),
        [onNavigate],
    );

    const hasTargets = !!targets && targets.length > 0;
    const ready = loaded && hasTargets;

    useDebugShortcuts(
        {
            palette: openPalette,
            closeTab: () => {
                const { active_tab } = debugWorkspaceStore.getSnapshot().ws;
                if (active_tab) debugWorkspaceStore.closeTab(active_tab);
            },
            nextTab: () => cycleTab(1),
            prevTab: () => cycleTab(-1),
            reopenTab: () => {
                debugWorkspaceStore.reopenClosed();
            },
        },
        { scopeRef: rootRef, enabled: ready },
    );

    let body: ReactNode;
    if (!targets && targetsQuery.isError) {
        body = (
            <PagePlaceholder>
                <EmptyIcon tone="danger" />
                <p className="font-display text-md font-semibold text-text">读不到 Bot 列表</p>
                <p className="max-w-sm text-xs text-text-secondary">
                    {targetsQuery.error?.message}
                </p>
                <Button size="sm" variant="secondary" onClick={() => void targetsQuery.refetch()}>
                    <RefreshCw size={13} aria-hidden />
                    重试
                </Button>
            </PagePlaceholder>
        );
    } else if (targets && targets.length === 0) {
        body = (
            <PagePlaceholder>
                <EmptyIcon tone="brand" />
                <p className="font-display text-md font-semibold text-text">还没有 Bot</p>
                <p className="max-w-sm text-xs leading-relaxed text-text-secondary">
                    调试台对着 Bot 发请求、看事件。先到「机器人」页添加一个 NapCat 或 SnowLuma 的
                    Bot 并启动，再回来这里。
                </p>
                {onNavigate && (
                    <Button size="sm" variant="primary" onClick={() => onNavigate('bots')}>
                        <Bot size={14} aria-hidden />
                        去「机器人」页
                    </Button>
                )}
            </PagePlaceholder>
        );
    } else if (!ready) {
        body = (
            <PagePlaceholder>
                <Spinner size="md" tone="brand" label="正在打开调试台" />
                <p className="text-[13px] text-text-secondary">正在打开调试台…</p>
            </PagePlaceholder>
        );
    } else {
        body = (
            <Workbench
                target={selected}
                callChannel={choice.call}
                leftCollapsed={layout.left_collapsed}
                rightCollapsed={layout.right_collapsed}
                leftWidth={layout.left_width}
                rightWidth={layout.right_width}
                rightAppear={rightAppear}
                onOpenPalette={openPalette}
                onRevealCallChannel={revealCallChannel}
                onNavigate={onNavigate}
            />
        );
    }

    return (
        <div ref={rootRef} className="flex min-h-0 min-w-0 flex-1 flex-col gap-3">
            {hasTargets || targetsQuery.isPending ? (
                <TopBar
                    targets={targets ?? []}
                    selected={selected}
                    targetsLoading={targetsQuery.isPending || !loaded}
                    onSelectBot={selectBot}
                    onManageBots={manageBots}
                    channels={channelsQuery.data}
                    channelsLoading={channelsQuery.isPending && !!botId}
                    channelsError={channelsQuery.isError}
                    onRetryChannels={retryChannels}
                    choice={choice}
                    onChoiceChange={onChoiceChange}
                    callChannelOpen={callChannelOpen}
                    onCallChannelOpenChange={setCallChannelOpen}
                    rightCollapsed={layout.right_collapsed}
                    onToggleRight={toggleRight}
                    onOpenPalette={openPalette}
                    onNavigate={onNavigate}
                />
            ) : (
                <header className="shrink-0 pt-2">
                    <p className="text-2xs uppercase leading-none tracking-widest text-text-tertiary">
                        debug
                    </p>
                    <h1 className="mt-1 font-display text-xl font-semibold leading-none text-text">
                        调试台
                    </h1>
                </header>
            )}
            <StorageNotices />
            {body}
            <CommandPalette open={paletteOpen} onOpenChange={setPaletteOpen} target={selected} />
        </div>
    );
}

export default DebugConsolePage;

function cycleTab(dir: 1 | -1): void {
    const { tabs, active_tab } = debugWorkspaceStore.getSnapshot().ws;
    if (tabs.length < 2) return;
    const idx = Math.max(
        0,
        tabs.findIndex((t) => t.id === active_tab),
    );
    const next = tabs[(idx + dir + tabs.length) % tabs.length];
    if (next) debugWorkspaceStore.setActive(next.id);
}

function EmptyIcon({ tone }: { tone: 'brand' | 'danger' }) {
    return (
        <span
            className={cn(
                'inline-flex h-12 w-12 items-center justify-center rounded-lg',
                tone === 'brand' ? 'bg-brand-soft text-brand' : 'bg-danger-soft text-danger',
            )}
        >
            <FlaskConical size={22} strokeWidth={1.8} aria-hidden />
        </span>
    );
}

// ---------------------------------------------------------------------------
// 存储损坏提示
// ---------------------------------------------------------------------------

const STORAGE_FILE_LABEL: Record<string, string> = {
    'workspace.json': '工作区（标签页和草稿）',
    'collections.json': '收藏',
    'history.jsonl': '调用历史',
};

function StorageNotices() {
    // 等工作区读完再取：那次加载里三份文件一起读过，损坏探测的结果已经齐了，
    // 不然这里会拿到加载前的空提示，而且 staleTime: Infinity 以后也不会再问
    const loaded = useDebugWorkspaceSelector((s) => s.loaded);
    const notices = useDebugStorageNotices({ enabled: loaded }).data;
    const [dismissed, setDismissed] = useState(storageNoticesDismissed);
    if (dismissed || !notices || notices.length === 0) return null;
    const close = () => {
        storageNoticesDismissed = true;
        setDismissed(true);
    };
    return (
        <div
            role="status"
            className="flex shrink-0 items-start gap-2.5 rounded-md border border-warning/30 bg-warning-soft/60 px-3 py-2"
        >
            <AlertTriangle
                size={14}
                strokeWidth={2.2}
                aria-hidden
                className="mt-0.5 shrink-0 text-warning"
            />
            <div className="min-w-0 flex-1 space-y-0.5 text-xs text-text-secondary">
                {notices.map((n: DebugStorageNotice) => (
                    <p key={n.file} className="break-words">
                        {STORAGE_FILE_LABEL[n.file] ?? n.file}读不出来（{n.reason}
                        ），这次从空白开始；原文件挪到了
                        <span className="mx-1 font-mono text-[11px] text-text">{n.moved_to}</span>
                    </p>
                ))}
            </div>
            <button
                type="button"
                onClick={close}
                aria-label="关闭提示"
                className="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-xs text-text-tertiary transition-colors hover:bg-warning-soft hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
            >
                <X size={13} aria-hidden />
            </button>
        </div>
    );
}

// ---------------------------------------------------------------------------
// 三栏工作台
// ---------------------------------------------------------------------------

/** 量一个元素的内容宽度（不含边框）；还没量到是 null */
function useElementWidth(el: HTMLElement | null): number | null {
    const [width, setWidth] = useState<number | null>(null);
    useLayoutEffect(() => {
        if (!el) {
            setWidth(null);
            return;
        }
        // 用 clientWidth 而不是 getBoundingClientRect：进场动画里的 scale 不该算进来
        setWidth(el.clientWidth || null);
        if (typeof ResizeObserver === 'undefined') return;
        const ro = new ResizeObserver((entries) => {
            const w = entries[0]?.contentRect.width;
            if (w) setWidth(Math.round(w));
        });
        ro.observe(el);
        return () => ro.disconnect();
    }, [el]);
    return width;
}

interface WorkbenchProps {
    target: DebugTarget | null;
    callChannel: DebugChannelId;
    leftCollapsed: boolean;
    rightCollapsed: boolean;
    leftWidth: number;
    rightWidth: number;
    /** 右栏是用户刚点展开的：内容滑进来 */
    rightAppear: boolean;
    onOpenPalette: () => void;
    onRevealCallChannel: () => void;
    /** 中栏错误卡片上「去哪解决」的出口按钮；没给就不画 */
    onNavigate?: (route: AppRoute) => void;
}

const Workbench = memo(function Workbench({
    target,
    callChannel,
    leftCollapsed,
    rightCollapsed,
    leftWidth,
    rightWidth,
    rightAppear,
    onOpenPalette,
    onRevealCallChannel,
    onNavigate,
}: WorkbenchProps) {
    const [rowEl, setRowEl] = useState<HTMLDivElement | null>(null);
    const available = useElementWidth(rowEl);
    const layout = {
        left_collapsed: leftCollapsed,
        right_collapsed: rightCollapsed,
        left_width: leftWidth,
        right_width: rightWidth,
    };
    const resolved = resolveColumns(layout, available);
    const leftRef = useRef<HTMLElement>(null);
    const rightRef = useRef<HTMLElement>(null);
    const centerRef = useRef<HTMLElement>(null);
    // 左栏收起 / 展开时中栏的左边缘跳一两百像素：内容滑过去，不是一下子闪到新位置
    const captureCenter = useSlideAfterShift(centerRef, leftCollapsed);

    const [panel, setPanelState] = useState<DebugLeftPanel>(lastLeftPanel);
    const setPanel = useCallback((p: DebugLeftPanel) => {
        lastLeftPanel = p;
        setPanelState(p);
    }, []);
    const [leftAppear, setLeftAppear] = useState(false);
    const focusAfterToggle = useRef(false);
    const setLeftCollapsed = (collapsed: boolean) => {
        captureCenter();
        setLeftAppear(true);
        focusAfterToggle.current = true;
        debugWorkspaceStore.setLayout({ left_collapsed: collapsed });
    };

    // 点的那个按钮收起 / 展开后就没了，焦点别掉回 body：收起后落在窄边的「展开」上，展开后落在选中的面板标签上
    useLayoutEffect(() => {
        if (!focusAfterToggle.current || !rowEl) return;
        focusAfterToggle.current = false;
        const next = leftCollapsed
            ? rowEl.querySelector<HTMLElement>('[data-rail-expand]')
            : rowEl.querySelector<HTMLElement>(
                  `[aria-label="${LEFT_TABS_LABEL}"] [role="tab"][aria-selected="true"]`,
              );
        next?.focus();
    }, [leftCollapsed, rowEl]);

    const previewWidth = (ref: RefObject<HTMLElement | null>) => (w: number) => {
        if (ref.current) ref.current.style.width = `${w}px`;
    };

    const { open: searchOpen } = useLeftSearch(panel);
    const searchLabel = LEFT_PANELS.find((p) => p.id === panel)?.searchLabel;

    // 右栏退场动画期间 resolved 已经按收起算了（right=0）：宽度冻在收起前的值，播完真卸载后再让中栏吃满
    const lastRightWidth = useRef(resolved.right);
    useLayoutEffect(() => {
        if (!rightCollapsed) lastRightWidth.current = resolved.right;
    });

    const leftHeader = (
        <div className={COLUMN_HEADER_CLASS}>
            <LeftPanelTabs active={panel} onChange={setPanel} />
            {searchLabel && (
                <Tooltip>
                    <TooltipTrigger asChild>
                        <button
                            type="button"
                            onClick={() =>
                                searchOpen
                                    ? setLeftSearchOpen(panel, false)
                                    : revealLeftSearch(panel)
                            }
                            aria-label={searchLabel}
                            aria-pressed={searchOpen}
                            className={cn(
                                'inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-sm transition-colors',
                                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                                searchOpen
                                    ? 'bg-brand-soft text-brand'
                                    : 'text-text-tertiary hover:bg-inset hover:text-text',
                            )}
                        >
                            <ActionMotionIcon icon={Search} size={15} strokeWidth={2} />
                        </button>
                    </TooltipTrigger>
                    <TooltipContent side="bottom">{searchLabel}（/）</TooltipContent>
                </Tooltip>
            )}
            <Tooltip>
                <TooltipTrigger asChild>
                    <button
                        type="button"
                        onClick={() => setLeftCollapsed(true)}
                        aria-label="收起左栏"
                        className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-sm text-text-tertiary transition-colors hover:bg-inset hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                    >
                        <ActionMotionIcon icon={PanelLeftClose} size={15} strokeWidth={2} />
                    </button>
                </TooltipTrigger>
                <TooltipContent side="bottom">收起左栏</TooltipContent>
            </Tooltip>
        </div>
    );

    const rightNotice =
        target && !target.running ? (
            <div className="flex shrink-0 items-center gap-2 border-b border-border-subtle/70 bg-warning-soft/40 px-3 py-2 text-2xs text-text-secondary">
                <PowerOff
                    size={12}
                    strokeWidth={2.2}
                    aria-hidden
                    className="shrink-0 text-warning"
                />
                <span>Bot 没在运行，启动后会自动开始接收</span>
            </div>
        ) : null;

    return (
        <div
            ref={setRowEl}
            className={cn(
                'flex min-h-0 min-w-0 flex-1 overflow-hidden rounded-lg border border-border-subtle',
                'bg-[color-mix(in_srgb,var(--surface-canvas)_82%,var(--surface-inset)_18%)]',
            )}
        >
            {leftCollapsed ? (
                <ColumnRail
                    width={LEFT_RAIL_WIDTH}
                    active={panel}
                    appear={leftAppear}
                    onExpand={() => setLeftCollapsed(false)}
                    onPick={(p) => {
                        setPanel(p);
                        setLeftCollapsed(false);
                    }}
                />
            ) : (
                <>
                    <ColumnFrame
                        ref={leftRef}
                        title="接口、收藏与历史"
                        errorTitle="左栏出错了"
                        width={resolved.left}
                        header={leftHeader}
                        appearFrom={leftAppear ? 'left' : null}
                    >
                        <LeftColumn target={target} panel={panel} />
                    </ColumnFrame>
                    <ColumnSplitter
                        side="left"
                        label="调整左栏宽度"
                        value={resolved.left}
                        getBounds={() => dragBounds('left', available, resolved)}
                        onPreview={previewWidth(leftRef)}
                        onCommit={(w) => debugWorkspaceStore.setLayout({ left_width: w })}
                        onReset={() =>
                            debugWorkspaceStore.setLayout({ left_width: UNSET_COLUMN_WIDTH })
                        }
                    />
                </>
            )}

            <ColumnFrame
                ref={centerRef}
                title="请求与响应"
                errorTitle="请求区出错了"
                className="bg-surface"
            >
                <CenterColumn
                    target={target}
                    callChannel={callChannel}
                    onOpenPalette={onOpenPalette}
                    onRevealCallChannel={onRevealCallChannel}
                    onNavigate={onNavigate}
                />
            </ColumnFrame>

            {/* 右栏收起也播动画：GsapPresence 等退场播完再真卸载；展开只在用户刚点过（rightAppear）时播，
                进页面的首帧不播——整页已经有路由切换动画 */}
            <GsapPresence
                visible={!rightCollapsed}
                onEnter={rightAppear ? enterRightColumn : undefined}
                onExit={exitRightColumn}
            >
                <div
                    className="flex min-h-0 min-w-0 shrink-0"
                    style={{ visibility: 'hidden', opacity: 0 }}
                >
                    <ColumnSplitter
                        side="right"
                        label="调整右栏宽度"
                        value={resolved.right}
                        getBounds={() => dragBounds('right', available, resolved)}
                        onPreview={previewWidth(rightRef)}
                        onCommit={(w) => debugWorkspaceStore.setLayout({ right_width: w })}
                        onReset={() =>
                            debugWorkspaceStore.setLayout({ right_width: UNSET_COLUMN_WIDTH })
                        }
                    />
                    <ColumnFrame
                        ref={rightRef}
                        title="事件与聊天"
                        errorTitle="聊天区出错了"
                        width={rightCollapsed ? lastRightWidth.current : resolved.right}
                        notice={rightNotice}
                    >
                        <RightColumn target={target} callChannel={callChannel} />
                    </ColumnFrame>
                </div>
            </GsapPresence>
        </div>
    );
});

const LEFT_TABS_LABEL = '左栏面板';

// 右栏的进退场：横向滑一段 + 淡入淡出（只动 transform / autoAlpha，宽度始终瞬间到位）
const enterRightColumn: EnterFn = (el, env) =>
    gsap.fromTo(
        el,
        { autoAlpha: 0, x: 24 },
        { autoAlpha: 1, x: 0, duration: env.duration('base'), ease: env.ease.enter },
    );
const exitRightColumn: ExitFn = (el, env) =>
    gsap.to(el, { autoAlpha: 0, x: 24, duration: env.duration('fast'), ease: env.ease.exit });

function LeftPanelTabs({
    active,
    onChange,
}: {
    active: DebugLeftPanel;
    onChange: (p: DebugLeftPanel) => void;
}) {
    const refs = useRef<Partial<Record<DebugLeftPanel, HTMLButtonElement | null>>>({});
    const onKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
        if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
        e.preventDefault();
        const idx = LEFT_PANELS.findIndex((p) => p.id === active);
        const next =
            LEFT_PANELS[
                (idx + (e.key === 'ArrowRight' ? 1 : -1) + LEFT_PANELS.length) % LEFT_PANELS.length
            ];
        onChange(next.id);
        refs.current[next.id]?.focus();
    };
    // 左栏拖到最窄（200px）时三个「图标 + 字」放不下，字会被截成「接.」：这时只留图标，字留给读屏，
    // 悬停提示照样写全名。提示在宽的时候也有，写的是比标签更完整的说法
    return (
        <div
            role="tablist"
            aria-label={LEFT_TABS_LABEL}
            onKeyDown={onKeyDown}
            className="@container/lefttabs flex min-w-0 flex-1 items-center gap-0.5 rounded-md bg-inset p-0.5"
        >
            {LEFT_PANELS.map((p) => {
                const on = p.id === active;
                const Icon = p.icon;
                return (
                    <Tooltip key={p.id}>
                        <TooltipTrigger asChild>
                            <button
                                ref={(node) => {
                                    refs.current[p.id] = node;
                                }}
                                type="button"
                                role="tab"
                                aria-selected={on}
                                tabIndex={on ? 0 : -1}
                                onClick={() => onChange(p.id)}
                                className={cn(
                                    'inline-flex min-w-0 flex-1 items-center justify-center gap-1 rounded-sm px-1.5 py-1 text-[12px] font-medium transition-colors',
                                    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
                                    on
                                        ? 'bg-elevated text-text shadow-sm ring-1 ring-border-subtle'
                                        : 'text-text-tertiary hover:bg-elevated/35 hover:text-text',
                                )}
                            >
                                <Icon
                                    size={12}
                                    strokeWidth={2.2}
                                    aria-hidden
                                    className="shrink-0"
                                />
                                <span className="truncate @max-[168px]/lefttabs:sr-only">
                                    {p.label}
                                </span>
                            </button>
                        </TooltipTrigger>
                        <TooltipContent side="bottom">{p.hint}</TooltipContent>
                    </Tooltip>
                );
            })}
        </div>
    );
}
