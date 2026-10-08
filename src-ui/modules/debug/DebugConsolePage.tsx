// 调试台页面（一级路由 'debug'）：顶栏选 Bot 和通道，下面三栏工作台（consoleParts/Workbench）——
// 左栏接口 / 收藏 / 历史，中栏请求与响应，右栏聊天。
//
// 页面本身不滚：高度来自路由容器（一路 flex-1 + min-h-0 撑下来），三栏各自在内部滚。
// 这里管的是跨栏的事：工作区载入 / 写盘、选中哪个 Bot（含从 Bot 卡片跳进来时带的那个）、
// 选中且在跑的 Bot 自动开始收事件、命令面板开关、标签页快捷键。

import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
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
import { channelIdKey } from '../../hooks/debug/keys';
import { AUTO_CHANNEL } from '../../core/domain/debug/channelPick';
import { defaultTargetId } from '../../core/domain/debug/targetGroups';
import { nextTabId } from '../../core/domain/debug/tabCycling';
import type { DebugChannelChoice } from '../../core/ipc/generated/debug/DebugChannelChoice';
import { CommandPalette } from './CommandPalette';
import { TopBar } from './TopBar';
import { useDebugShortcuts } from './debugShortcuts';
import { StorageNotices } from './consoleParts/StorageNotices';
import { Workbench } from './consoleParts/Workbench';
import { ConsoleLoading, ConsoleNoTargets, ConsoleTargetsError } from './consoleParts/PageStates';

const DEFAULT_CHOICE: DebugChannelChoice = { call: AUTO_CHANNEL, events: AUTO_CHANNEL };

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
            <ConsoleTargetsError
                message={targetsQuery.error?.message}
                onRetry={() => void targetsQuery.refetch()}
            />
        );
    } else if (targets && targets.length === 0) {
        body = <ConsoleNoTargets onNavigate={onNavigate} />;
    } else if (!ready) {
        body = <ConsoleLoading />;
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

/** 快捷键切标签：算 next 的纯逻辑在 core/domain/debug/tabCycling，这里只落 store */
function cycleTab(dir: 1 | -1): void {
    const { tabs, active_tab } = debugWorkspaceStore.getSnapshot().ws;
    const next = nextTabId(tabs, active_tab, dir);
    if (next) debugWorkspaceStore.setActive(next);
}

export default DebugConsolePage;
