// 三栏工作台：左（接口 / 收藏 / 历史）+ 中（请求与响应）+ 右（事件与聊天），
// 只管栏宽、收起 / 展开、栏头这些跨栏的事；三栏内容在各自的 left/ center/ right/ 里。
import { memo, useCallback, useLayoutEffect, useRef, useState, type RefObject } from 'react';
import { PanelLeftClose, PowerOff, Search } from 'lucide-react';
import gsap from 'gsap';
import { cn } from '../../../shared/utils/cn';
import { Tooltip, TooltipContent, TooltipTrigger } from '../../../shared/ui';
import {
    ActionMotionIcon,
    GsapPresence,
    type EnterFn,
    type ExitFn,
} from '../../../shared/ui/motion';
import type { AppRoute } from '../../../shared/components/next/Sidebar';
import { debugWorkspaceStore } from '../../../hooks/debug/debugWorkspaceStore';
import {
    LEFT_RAIL_WIDTH,
    UNSET_COLUMN_WIDTH,
    dragBounds,
    resolveColumns,
} from '../../../core/domain/debug/workbenchLayout';
import type { DebugChannelId } from '../../../core/ipc/generated/debug/DebugChannelId';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import {
    COLUMN_HEADER_CLASS,
    ColumnFrame,
    ColumnRail,
    ColumnSplitter,
    useSlideAfterShift,
} from '../ColumnFrame';
import {
    LEFT_PANELS,
    revealLeftSearch,
    setLeftSearchOpen,
    useLeftSearch,
    type DebugLeftPanel,
} from '../leftPanels';
import { LeftColumn } from '../left/LeftColumn';
import { CenterColumn } from '../center/CenterColumn';
import { RightColumn } from '../right/RightColumn';
import { LEFT_TABS_LABEL, LeftPanelTabs } from './LeftPanelTabs';
import { useElementWidth } from './useElementWidth';

// 左栏停在哪个面板：纯界面状态，不落盘，但切走路由再回来应该还在原处
let lastLeftPanel: DebugLeftPanel = 'catalog';

// 右栏的进退场：横向滑一段 + 淡入淡出（只动 transform / autoAlpha，宽度始终瞬间到位）
const enterRightColumn: EnterFn = (el, env) =>
    gsap.fromTo(
        el,
        { autoAlpha: 0, x: 24 },
        { autoAlpha: 1, x: 0, duration: env.duration('base'), ease: env.ease.enter },
    );
const exitRightColumn: ExitFn = (el, env) =>
    gsap.to(el, { autoAlpha: 0, x: 24, duration: env.duration('fast'), ease: env.ease.exit });

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

export const Workbench = memo(function Workbench({
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
            {searchLabel ? (
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
                                'ml-auto inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-sm transition-colors',
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
            ) : (
                <span aria-hidden className="ml-auto h-7 w-7 shrink-0" />
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
