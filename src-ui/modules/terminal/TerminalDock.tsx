// 底部终端面板：浮在页面上面（不挤页面的空间），多标签、标签里可以分两块；切页面、收起都不断，
// 拖顶边改高度，能最大化。应用根上常驻一个：收起时只是不画，会话和 xterm 实例都还在。

import '@xterm/xterm/css/xterm.css';
import './terminal.css';

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { ChevronDown, Columns2, Maximize2, Minimize2, Rows2, SquareTerminal } from 'lucide-react';
import { Button } from '../../shared/ui';
import { cn } from '../../shared/utils/cn';
import { useThemeTokens } from '../../hooks/theme/useThemeTokens';
import { useMotion } from '../../hooks/preferences/useMotion';
import gsap from 'gsap';
import {
    DOCK_HEIGHT_MIN,
    terminalLayout,
    useTerminalLayout,
    useTerminalPrefs,
} from '../../hooks/terminal/terminalPrefs';
import {
    terminalStore,
    useTerminalState,
    type TerminalGroup,
} from '../../hooks/terminal/terminalStore';
import { uploadToSession } from '../../hooks/terminal/useTerminalFiles';
import {
    useTerminalFileDrop,
    type TerminalDropTarget,
} from '../../hooks/terminal/useTerminalFileDrop';
import { pushInfoBar } from '../../hooks/ui/globalInfoBarStore';
import { buildPalette } from '../../core/domain/terminal/palette';
import { quotePath, shellSyntaxOf } from '../../core/domain/terminal/paths';
import { getRuntime, setTerminalTheme, startTerminalRuntimes } from './registry';
import { TerminalPane } from './TerminalPane';
import { TerminalNewMenu, TerminalTabs } from './TerminalTabs';

/** 面板顶上最高能拖到哪：标题栏（44）下面再留一截，看得见后面的页面；要全屏走最大化 */
const DOCK_TOP_KEEP = 44 + 72;

function useDrag(onMove: (e: PointerEvent) => void, onEnd?: () => void) {
    const [dragging, setDragging] = useState(false);
    const start = useCallback(
        (e: React.PointerEvent) => {
            e.preventDefault();
            setDragging(true);
            const move = (ev: PointerEvent) => onMove(ev);
            const up = () => {
                setDragging(false);
                window.removeEventListener('pointermove', move);
                window.removeEventListener('pointerup', up);
                onEnd?.();
            };
            window.addEventListener('pointermove', move);
            window.addEventListener('pointerup', up);
        },
        [onMove, onEnd],
    );
    return { dragging, start };
}

function GroupView({
    group,
    visible,
    drop,
}: {
    group: TerminalGroup;
    visible: boolean;
    drop: TerminalDropTarget | null;
}) {
    const boxRef = useRef<HTMLDivElement>(null);
    const divider = useDrag((e) => {
        const rect = boxRef.current?.getBoundingClientRect();
        if (!rect) return;
        const ratio =
            group.split === 'row'
                ? (e.clientX - rect.left) / rect.width
                : (e.clientY - rect.top) / rect.height;
        terminalStore.setRatio(group.id, ratio);
    });
    const [first, second] = group.panes;
    const zoneOf = (id: string | undefined) =>
        id && drop?.sessionId === id ? (drop.dir ? 'files' : 'terminal') : null;
    return (
        <div
            ref={boxRef}
            className={cn(
                'ncd-term-group flex min-h-0 min-w-0 flex-1',
                group.split === 'column' ? 'flex-col' : 'flex-row',
            )}
        >
            {first && (
                <div
                    className="flex min-h-0 min-w-0"
                    style={
                        second
                            ? { flexBasis: `${group.ratio * 100}%`, flexGrow: 0, flexShrink: 0 }
                            : { flex: 1 }
                    }
                >
                    <TerminalPane
                        sessionId={first}
                        focused={group.focused === first}
                        visible={visible}
                        dropZone={zoneOf(first)}
                        showHeader
                    />
                </div>
            )}
            {second && (
                <>
                    <div
                        className="ncd-term-divider"
                        data-split={group.split}
                        data-dragging={divider.dragging}
                        onPointerDown={divider.start}
                    />
                    <div
                        className="ncd-term-pane-in flex min-h-0 min-w-0 flex-1"
                        data-split={group.split}
                    >
                        <TerminalPane
                            sessionId={second}
                            focused={group.focused === second}
                            visible={visible}
                            dropZone={zoneOf(second)}
                            showHeader
                        />
                    </div>
                </>
            )}
        </div>
    );
}

/** 没分屏时点了把当前这个目标在右边再开一块；分了以后换成左右 / 上下切换，按钮不挪位置 */
function SplitButton({
    group,
    focusedId,
}: {
    group: TerminalGroup;
    focusedId: string | undefined;
}) {
    const split = group.panes.length > 1;
    const label = !split ? '左右分屏' : group.split === 'row' ? '改成上下分屏' : '改成左右分屏';
    const Icon = split && group.split === 'row' ? Rows2 : Columns2;
    const onClick = () => {
        if (split) {
            terminalStore.setSplit(group.id, group.split === 'row' ? 'column' : 'row');
            return;
        }
        const view = focusedId ? terminalStore.getSnapshot().sessions[focusedId] : undefined;
        if (view)
            void terminalStore.open(view.info.target, {
                shell: view.info.shell,
                splitFrom: focusedId,
                split: 'row',
            });
    };
    return (
        <button
            type="button"
            title={label}
            aria-label={label}
            onClick={onClick}
            className="flex h-7 w-7 items-center justify-center rounded-sm text-text-tertiary hover:bg-inset hover:text-text"
        >
            <Icon size={14} />
        </button>
    );
}

export function TerminalDock() {
    const state = useTerminalState();
    const layout = useTerminalLayout();
    const prefs = useTerminalPrefs();
    const dockRef = useRef<HTMLElement>(null);
    const [liveHeight, setLiveHeight] = useState<number | null>(null);
    const liveHeightRef = useRef<number | null>(null);
    const motion = useMotion();
    // 收起时先放完退场动画再摘掉；还原时先把铺满的面板收回去再换布局，
    // 所以「画不画」「按不按最大化排」各有一份，比 store 晚一拍
    const [mounted, setMounted] = useState(state.open);
    const [shownMax, setShownMax] = useState(state.maximized);
    if (!mounted && shownMax !== state.maximized) setShownMax(state.maximized);
    if (state.open && !mounted) setMounted(true);
    if (state.maximized && !shownMax) setShownMax(true);
    const openAnim = useRef<gsap.core.Animation | null>(null);
    const maxAnim = useRef<gsap.core.Animation | null>(null);

    useEffect(() => startTerminalRuntimes(), []);

    // 开 / 收：只动 opacity 和 transform，结束后清掉，免得面板里 fixed 定位的东西以它为参照
    useLayoutEffect(() => {
        const el = dockRef.current;
        if (!el) return;
        openAnim.current?.kill();
        if (state.open) {
            if (!motion.enabled) {
                gsap.set(el, { clearProps: 'opacity,transform' });
                return;
            }
            openAnim.current = gsap.fromTo(
                el,
                { opacity: 0, y: 18, scale: 0.992 },
                {
                    opacity: 1,
                    y: 0,
                    scale: 1,
                    duration: motion.duration('base'),
                    ease: motion.ease.enter,
                    clearProps: 'opacity,transform',
                },
            );
        } else if (!motion.enabled) {
            setMounted(false);
        } else {
            openAnim.current = gsap.to(el, {
                opacity: 0,
                y: 18,
                scale: 0.992,
                duration: motion.duration('fast'),
                ease: motion.ease.exit,
                onComplete: () => setMounted(false),
            });
        }
    }, [state.open, mounted, motion.enabled]);
    useEffect(() => () => void openAnim.current?.kill(), []);

    // 最大化：布局先铺满，整块从原来的顶边往上推到位；还原：整块往下推回原来的顶边，再换回小布局。
    // 标题行跟着顶边走，推到窗口外的那截被外层裁掉；动画期间终端尺寸不变，只在落定时重排一次行列
    const prevMax = useRef(state.maximized);
    useLayoutEffect(() => {
        const el = dockRef.current;
        const was = prevMax.current;
        prevMax.current = state.maximized;
        // 最大化着直接收起：保持铺满的样子一起淡出，摘掉以后再按 store 重置
        if (was === state.maximized || !state.open) return;
        maxAnim.current?.kill();
        if (!el || !motion.enabled) {
            if (el) gsap.set(el, { clearProps: 'transform' });
            if (!state.maximized) setShownMax(false);
            return;
        }
        const edge = Math.max(0, el.offsetHeight - terminalLayout.get().height);
        if (state.maximized) {
            maxAnim.current = gsap.fromTo(
                el,
                { y: edge },
                {
                    y: 0,
                    duration: motion.duration('base'),
                    ease: motion.ease.enter,
                    clearProps: 'transform',
                },
            );
        } else {
            maxAnim.current = gsap.fromTo(
                el,
                { y: 0 },
                {
                    y: edge,
                    duration: motion.duration('base'),
                    ease: motion.ease.enter,
                    // 位移留到换完布局那一刻再清（下面的 effect），不然会先闪一帧铺满的样子
                    onComplete: () => setShownMax(false),
                },
            );
        }
    }, [state.maximized, state.open, motion.enabled]);
    useLayoutEffect(() => {
        const el = dockRef.current;
        if (el && !shownMax && !maxAnim.current?.isActive())
            gsap.set(el, { clearProps: 'transform' });
    }, [shownMax]);

    // 页面上贴底悬浮的按钮（index.css 的 .float-above-terminal）靠这几样避开面板。
    // 面板贴着窗口底边、离底有一截空隙，抬的高度按「面板顶边到窗口底」算
    useLayoutEffect(() => {
        const root = document.documentElement;
        const dock = dockRef.current;
        if (!dock || !state.open) return;
        const publish = () => {
            // offsetTop 不算 transform：打开时的上浮动画还没走完也能量到落定的位置
            const top = (dock.offsetParent?.getBoundingClientRect().top ?? 0) + dock.offsetTop;
            const inset = Math.max(0, Math.round(window.innerHeight - top));
            root.style.setProperty('--terminal-dock-inset', `${inset}px`);
        };
        publish();
        const observer = new ResizeObserver(publish);
        observer.observe(dock);
        window.addEventListener('resize', publish);
        return () => {
            observer.disconnect();
            window.removeEventListener('resize', publish);
            root.style.removeProperty('--terminal-dock-inset');
        };
    }, [state.open, mounted, shownMax]);
    const covers = state.open && shownMax;
    useLayoutEffect(() => {
        document.documentElement.toggleAttribute('data-terminal-covers', covers);
        return () => document.documentElement.removeAttribute('data-terminal-covers');
    }, [covers]);

    const tokens = useThemeTokens({
        background: { name: '--surface-inset', fallback: '#f4efe7' },
        foreground: { name: '--text-primary', fallback: '#2c1f18' },
        accent: { name: '--accent-500', fallback: '#f58fb6' },
    });
    const palette = useMemo(
        () => buildPalette(tokens, prefs.colorScheme),
        [tokens.background, tokens.foreground, tokens.accent, prefs.colorScheme],
    );
    useEffect(() => setTerminalTheme(palette), [palette]);

    // Ctrl+` 开关面板、Ctrl+Shift+` 新开本机终端；焦点在终端里时由终端自己处理
    useEffect(() => {
        const onKey = (e: KeyboardEvent) => {
            if (!(e.ctrlKey || e.metaKey) || e.code !== 'Backquote') return;
            if (e.target instanceof Element && e.target.closest('.xterm')) return;
            e.preventDefault();
            if (e.shiftKey) void terminalStore.open({ kind: 'local' }, { forceNew: true });
            else terminalStore.toggle();
        };
        window.addEventListener('keydown', onKey);
        return () => window.removeEventListener('keydown', onKey);
    }, []);

    const resize = useDrag(
        (e) => {
            const dock = dockRef.current;
            const parent = dock?.parentElement;
            if (!dock || !parent) return;
            const bottom = dock.getBoundingClientRect().bottom;
            const max = bottom - parent.getBoundingClientRect().top - DOCK_TOP_KEEP;
            const next = Math.round(
                Math.min(
                    Math.max(bottom - e.clientY, DOCK_HEIGHT_MIN),
                    Math.max(DOCK_HEIGHT_MIN, max),
                ),
            );
            liveHeightRef.current = next;
            setLiveHeight(next);
        },
        () => {
            // 拖的时候只改本地高度，松手才落盘
            if (liveHeightRef.current !== null)
                terminalLayout.patch({ height: liveHeightRef.current });
            liveHeightRef.current = null;
            setLiveHeight(null);
        },
    );

    const onDrop = useCallback((target: TerminalDropTarget, paths: string[]) => {
        const view = terminalStore.getSnapshot().sessions[target.sessionId];
        if (!view) return;
        const { info } = view;
        if (target.dir) {
            void uploadToSession(target.sessionId, paths, target.dir);
        } else if (info.host_id === 'local') {
            const syntax = shellSyntaxOf(info.host_os, info.shell);
            getRuntime(target.sessionId)?.fillInput(
                `${paths.map((p) => quotePath(p, syntax)).join(' ')} `,
            );
        } else if (info.features.files) {
            void uploadToSession(target.sessionId, paths, view.cwd ?? '~');
        } else {
            pushInfoBar({
                tone: 'warning',
                title: '容器里的终端没法直接传文件',
                content: '改用「宿主机部署目录」那个终端传',
            });
        }
    }, []);
    const drop = useTerminalFileDrop(state.open && state.groups.length > 0, onDrop);

    // 悬浮按钮跟着面板滑上滑下；拖高度时逐帧跟手，不带过渡
    const glide = motion.enabled && !resize.dragging;
    useLayoutEffect(() => {
        document.documentElement.toggleAttribute('data-terminal-glide', glide);
        return () => document.documentElement.removeAttribute('data-terminal-glide');
    }, [glide]);

    if (!mounted) return null;
    const activeGroup = state.groups.find((g) => g.id === state.activeGroup) ?? null;
    const height = liveHeight ?? layout.height;
    const focusedId = activeGroup?.focused;
    const focusedView = focusedId ? state.sessions[focusedId] : undefined;

    return (
        <section
            ref={dockRef}
            aria-label="终端"
            data-motion={motion.enabled ? 'on' : 'off'}
            className={cn(
                'ncd-term absolute inset-x-2 bottom-2 z-20 flex min-h-0 flex-col overflow-hidden rounded-md',
                'border border-border-subtle bg-surface shadow-popover',
                // 最大化时顶到标题栏下面（标题栏 h-11）
                shownMax && 'top-11',
            )}
            style={{
                height: shownMax ? undefined : height,
                ['--ncd-term-bg' as string]: palette.background,
            }}
        >
            {!shownMax && (
                <div
                    className="ncd-term-resize"
                    data-dragging={resize.dragging}
                    onPointerDown={resize.start}
                    onDoubleClick={() => terminalStore.setMaximized(true)}
                    title="拖动改高度，双击最大化"
                />
            )}
            <header className="flex h-9 shrink-0 items-center gap-1 border-b border-border-subtle px-2">
                <SquareTerminal size={14} className="mx-1 shrink-0 text-text-tertiary" />
                <TerminalTabs state={state} />
                <TerminalNewMenu busy={state.busy} />
                {focusedView && activeGroup && (
                    <SplitButton group={activeGroup} focusedId={focusedId} />
                )}
                <button
                    type="button"
                    title={state.maximized ? '还原终端面板' : '最大化终端面板'}
                    aria-label={state.maximized ? '还原终端面板' : '最大化终端面板'}
                    onClick={() => terminalStore.setMaximized(!state.maximized)}
                    className="flex h-7 w-7 items-center justify-center rounded-sm text-text-tertiary hover:bg-inset hover:text-text"
                >
                    {state.maximized ? <Minimize2 size={14} /> : <Maximize2 size={14} />}
                </button>
                <button
                    type="button"
                    title="收起（Ctrl+`）；终端都还开着"
                    aria-label="收起终端面板"
                    onClick={() => terminalStore.setOpen(false)}
                    className="flex h-7 w-7 items-center justify-center rounded-sm text-text-tertiary hover:bg-inset hover:text-text"
                >
                    <ChevronDown size={15} />
                </button>
            </header>
            {activeGroup ? (
                <GroupView
                    key={activeGroup.id}
                    group={activeGroup}
                    visible={state.open}
                    drop={drop}
                />
            ) : (
                <div className="flex flex-1 flex-col items-center justify-center gap-3 text-[13px] text-text-secondary">
                    <p>没有开着的终端</p>
                    <Button
                        size="sm"
                        disabled={state.busy}
                        onClick={() => void terminalStore.open({ kind: 'local' })}
                    >
                        <SquareTerminal size={14} />
                        开一个本机终端
                    </Button>
                </div>
            )}
        </section>
    );
}
