// 面板顶上的标签条和「+」菜单。
// 标签上的小点：后台来了输出是灰点，后台跑完命令成功绿点、失败红点；程序退出 / 断线时标签名变淡。

import { useLayoutEffect, useRef, useState } from 'react';
import { ChevronDown, Columns2, Copy, FileDown, Pencil, Plus, Rows2, SquareArrowOutUpRight, X } from 'lucide-react';
import {
    ContextMenu,
    ContextMenuContent,
    ContextMenuItem,
    ContextMenuSeparator,
    ContextMenuTrigger,
    Popover,
    PopoverContent,
    PopoverTrigger,
} from '../../shared/ui';
import { cn } from '../../shared/utils/cn';
import {
    isLive,
    sessionTitle,
    terminalStore,
    type TerminalActivity,
    type TerminalGroup,
    type TerminalState,
} from '../../hooks/terminal/terminalStore';
import { terminalIo } from '../../hooks/terminal/terminalIo';
import { useTerminalExternal } from '../../hooks/terminal/useTerminalExternal';
import { useTerminalLaunchOptions } from '../../hooks/terminal/useTerminalLaunchOptions';
import { getRuntime } from './registry';
import { TargetIcon } from './TerminalPane';

const ACTIVITY_RANK: Record<TerminalActivity, number> = { none: 0, output: 1, ok: 2, fail: 3 };

function groupActivity(state: TerminalState, group: TerminalGroup): TerminalActivity {
    let best: TerminalActivity = 'none';
    for (const id of group.panes) {
        const a = state.sessions[id]?.activity ?? 'none';
        if (ACTIVITY_RANK[a] > ACTIVITY_RANK[best]) best = a;
    }
    return best;
}

function Tab({ state, group, active }: { state: TerminalState; group: TerminalGroup; active: boolean }) {
    const [renaming, setRenaming] = useState(false);
    const [draft, setDraft] = useState('');
    const openExternal = useTerminalExternal();
    const view = state.sessions[group.focused] ?? state.sessions[group.panes[0] ?? ''];
    if (!view) return null;
    const title = sessionTitle(view);
    const activity = groupActivity(state, group);
    const live = group.panes.every((id) => {
        const s = state.sessions[id];
        return s ? isLive(s.info.status) : false;
    });
    const focusedId = view.info.id as string;

    const commit = () => {
        terminalStore.rename(focusedId, draft);
        setRenaming(false);
    };

    return (
        <ContextMenu>
            <ContextMenuTrigger asChild>
                <div
                    role="tab"
                    aria-selected={active}
                    tabIndex={0}
                    onClick={() => terminalStore.focusGroup(group.id)}
                    onAuxClick={(e) => {
                        if (e.button === 1) void terminalStore.closeGroup(group.id);
                    }}
                    onDoubleClick={() => {
                        setDraft(title);
                        setRenaming(true);
                    }}
                    className={cn(
                        'ncd-term-tab group relative z-[1] flex h-7 max-w-[220px] shrink-0 cursor-pointer select-none items-center gap-1.5 rounded-sm pl-2.5 pr-1 text-[12px] transition-colors',
                        // 当前标签的底色是标签条里那块会滑的指示块
                        active ? 'text-text' : 'text-text-secondary hover:bg-inset/60 hover:text-text',
                        !live && 'italic opacity-70',
                    )}
                    title={`${title}${view.cwd ? `\n${view.cwd}` : ''}`}
                >
                    <TargetIcon target={view.info.target} />
                    {renaming ? (
                        <input
                            autoFocus
                            value={draft}
                            onChange={(e) => setDraft(e.target.value)}
                            onBlur={commit}
                            onKeyDown={(e) => {
                                if (e.key === 'Enter') commit();
                                if (e.key === 'Escape') setRenaming(false);
                            }}
                            onClick={(e) => e.stopPropagation()}
                            className="w-28 rounded-xs bg-field px-1 text-[12px] text-text outline-none ring-1 ring-brand"
                        />
                    ) : (
                        <span className="min-w-0 truncate">{title}</span>
                    )}
                    {group.panes.length > 1 && <span className="shrink-0 text-[10px] text-text-tertiary">+1</span>}
                    {activity !== 'none' && !active && (
                        <span
                            key={activity}
                            className={cn(
                                'ncd-term-dot h-1.5 w-1.5 shrink-0 rounded-full',
                                activity === 'fail' ? 'bg-danger' : activity === 'ok' ? 'bg-success' : 'bg-text-tertiary',
                            )}
                        />
                    )}
                    <button
                        type="button"
                        title="关掉（中键也行）"
                        aria-label="关掉"
                        onClick={(e) => {
                            e.stopPropagation();
                            void terminalStore.closeGroup(group.id);
                        }}
                        className={cn(
                            'flex h-5 w-5 shrink-0 items-center justify-center rounded-xs text-text-tertiary hover:bg-surface hover:text-text',
                            active ? 'visible' : 'invisible group-hover:visible',
                        )}
                    >
                        <X size={12} />
                    </button>
                </div>
            </ContextMenuTrigger>
            <ContextMenuContent className="min-w-[190px]">
                <ContextMenuItem
                    onClick={() => {
                        setDraft(title);
                        setRenaming(true);
                    }}
                >
                    <Pencil size={13} />
                    <span>改名</span>
                </ContextMenuItem>
                <ContextMenuItem onClick={() => void terminalStore.open(view.info.target, { shell: view.info.shell, forceNew: true })}>
                    <Copy size={13} />
                    <span>再开一个同样的</span>
                </ContextMenuItem>
                {group.panes.length < 2 && (
                    <>
                        <ContextMenuItem
                            onClick={() =>
                                void terminalStore.open(view.info.target, { shell: view.info.shell, splitFrom: focusedId, split: 'row' })
                            }
                        >
                            <Columns2 size={13} />
                            <span>左右分屏</span>
                        </ContextMenuItem>
                        <ContextMenuItem
                            onClick={() =>
                                void terminalStore.open(view.info.target, { shell: view.info.shell, splitFrom: focusedId, split: 'column' })
                            }
                        >
                            <Rows2 size={13} />
                            <span>上下分屏</span>
                        </ContextMenuItem>
                    </>
                )}
                {group.panes.length > 1 && (
                    <ContextMenuItem onClick={() => terminalStore.setSplit(group.id, group.split === 'row' ? 'column' : 'row')}>
                        {group.split === 'row' ? <Rows2 size={13} /> : <Columns2 size={13} />}
                        <span>{group.split === 'row' ? '改成上下' : '改成左右'}</span>
                    </ContextMenuItem>
                )}
                {view.info.features.external && (
                    <ContextMenuItem onClick={() => openExternal(view.info.target, view.info.shell)}>
                        <SquareArrowOutUpRight size={13} />
                        <span>在系统终端里打开</span>
                    </ContextMenuItem>
                )}
                <ContextMenuItem
                    onClick={() => {
                        const runtime = getRuntime(focusedId);
                        if (runtime) void terminalIo.exportText(`${title.replace(/[\\/:*?"<>|\s·]+/g, '-')}.log`, runtime.bufferText());
                    }}
                >
                    <FileDown size={13} />
                    <span>导出输出</span>
                </ContextMenuItem>
                <ContextMenuSeparator />
                <ContextMenuItem onClick={() => void terminalStore.closeOthers(group.id)}>
                    <X size={13} />
                    <span>关掉其它标签</span>
                </ContextMenuItem>
                <ContextMenuItem tone="danger" onClick={() => void terminalStore.closeGroup(group.id)}>
                    <X size={13} />
                    <span>关掉</span>
                </ContextMenuItem>
            </ContextMenuContent>
        </ContextMenu>
    );
}

/// 当前标签底下那块底色：切标签时滑过去、跟着标签宽度伸缩（只动 transform 和宽度，元素很小）。
/// 第一次落位不带过渡，免得面板一打开它从最左边滑进来
function useActiveIndicator(activeGroup: string | null, groupCount: number) {
    const listRef = useRef<HTMLDivElement>(null);
    const barRef = useRef<HTMLDivElement>(null);
    const placed = useRef(false);

    useLayoutEffect(() => {
        const list = listRef.current;
        const bar = barRef.current;
        if (!list || !bar) return;
        const place = () => {
            const tab = list.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]');
            if (!tab) {
                bar.style.opacity = '0';
                placed.current = false;
                return;
            }
            bar.dataset.instant = placed.current ? 'false' : 'true';
            bar.style.opacity = '1';
            bar.style.width = `${tab.offsetWidth}px`;
            bar.style.transform = `translateX(${tab.offsetLeft}px)`;
            placed.current = true;
        };
        place();
        const tab = list.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]');
        // 改名、标题变了标签会变宽，跟着量
        const observer = new ResizeObserver(place);
        if (tab) observer.observe(tab);
        observer.observe(list);
        return () => observer.disconnect();
    }, [activeGroup, groupCount]);

    return { listRef, barRef };
}

export function TerminalTabs({ state }: { state: TerminalState }) {
    const { listRef, barRef } = useActiveIndicator(state.activeGroup, state.groups.length);
    return (
        <div
            ref={listRef}
            role="tablist"
            className="scrollbar-hide relative flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto"
        >
            <div ref={barRef} aria-hidden className="ncd-term-tab-indicator pointer-events-none absolute left-0 top-0 h-7 rounded-sm bg-inset" />
            {state.groups.map((group) => (
                <Tab key={group.id} state={state} group={group} active={group.id === state.activeGroup} />
            ))}
        </div>
    );
}

export function TerminalNewMenu({ busy }: { busy: boolean }) {
    const [open, setOpen] = useState(false);
    const { shells, servers } = useTerminalLaunchOptions(open);
    const openExternal = useTerminalExternal();

    const item = (key: string, onClick: () => void, children: React.ReactNode) => (
        <button
            key={key}
            type="button"
            onClick={() => {
                setOpen(false);
                onClick();
            }}
            className="flex w-full items-center gap-2 rounded-xs px-2 py-1.5 text-left text-[12px] text-text hover:bg-inset"
        >
            {children}
        </button>
    );

    return (
        <div className="flex shrink-0 items-center">
            <button
                type="button"
                title="新开本机终端（Ctrl+Shift+`）"
                aria-label="新开终端"
                disabled={busy}
                onClick={() => void terminalStore.open({ kind: 'local' }, { forceNew: true })}
                className="flex h-7 w-7 items-center justify-center rounded-l-sm text-text-tertiary hover:bg-inset hover:text-text disabled:opacity-50"
            >
                <Plus size={14} />
            </button>
            <Popover open={open} onOpenChange={setOpen}>
                <PopoverTrigger asChild>
                    <button
                        type="button"
                        title="选择要开的终端"
                        aria-label="选择要开的终端"
                        className="flex h-7 w-4 items-center justify-center rounded-r-sm text-text-tertiary hover:bg-inset hover:text-text"
                    >
                        <ChevronDown size={12} />
                    </button>
                </PopoverTrigger>
                <PopoverContent align="start" className="w-60 p-1">
                    <p className="px-2 pb-1 pt-1.5 text-[11px] text-text-tertiary">本机</p>
                    {shells.map((shell) =>
                        item(
                            shell.kind,
                            () => void terminalStore.open({ kind: 'local' }, { shell: shell.kind, forceNew: true }),
                            <>
                                <TargetIcon target={{ kind: 'local' }} />
                                {shell.label}
                            </>,
                        ),
                    )}
                    {shells.length === 0 && <p className="px-2 py-1 text-[11px] text-text-tertiary">正在找本机的 shell…</p>}
                    {item(
                        'external',
                        () => openExternal({ kind: 'local' }),
                        <>
                            <SquareArrowOutUpRight size={12} />
                            在系统终端里打开
                        </>,
                    )}
                    <p className="px-2 pb-1 pt-2.5 text-[11px] text-text-tertiary">远端主机</p>
                    {servers.map((server) =>
                        item(
                            server.id,
                            () => void terminalStore.open({ kind: 'server', server_id: server.id }, { forceNew: true }),
                            <>
                                <TargetIcon target={{ kind: 'server', server_id: server.id }} />
                                <span className="truncate">{server.label}</span>
                            </>,
                        ),
                    )}
                    {servers.length === 0 && (
                        <p className="px-2 py-1 text-[11px] text-text-tertiary">还没添加远端主机，到「远端」页加一台</p>
                    )}
                </PopoverContent>
            </Popover>
        </div>
    );
}
