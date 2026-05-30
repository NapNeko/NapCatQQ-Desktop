// Components 页（next）：单机视图 + 主机切换。
//
// 交互：页面一次只展示一台机器的组件。顶部一排主机切换标签（本机 / 各远端），
// 点哪台就在下方铺哪台的组件，按框架 / 运行时依赖 / 桌面端分组成网格。装不了
// 的组件（平台不支持）不出现。docker 就绪的机器在末尾带 Docker 部署区。
//
// 只有一台机器时不显示切换条。这样"组件 × 各主机"被翻成"先选机器、再看这台
// 机器能装啥"，扫描成本远低于把每台机器堆成一张大卡上下排。
//
// 严守 frontend-layering：仅 import hooks / shared/ui / 自身组件 + domain
// 纯函数，不直接调 service / @tauri-apps。

import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { Box, Loader2, RefreshCw } from 'lucide-react';
import { Button } from '../../shared/ui';
import { useComponents } from '../../hooks/components/useComponents';
import { useComponentAction } from '../../hooks/components/useComponentAction';
import { useComponentActionErrors } from '../../hooks/components/useComponentActionErrors';
import { useReleases } from '../../hooks/diagnostics/useReleases';
import { useDockerHosts } from '../../hooks/docker/useDockerHosts';
import { HostSwitcher } from './HostSwitcher';
import { HostComponentsView } from './HostComponentsView';
import { groupByHost, type ComponentRow, type MachineView } from '../../core/domain/components/types';
import type { ComponentId } from '../../core/ipc/types';

export const ComponentsPageNext: React.FC = () => {
    const { view, hosts, isLoading, error, refetch } = useComponents();
    const { startAction, cancelAction, getProgressFor, onTaskTerminal } = useComponentAction();
    const { snapshot: releases } = useReleases();

    const hostIds = useMemo(() => hosts.map((h) => h.host_id), [hosts]);
    const dockerHosts = useDockerHosts(hostIds);

    // 组件主导矩阵 → 主机主导，再剔掉这台机器一个组件都装不了的空机器。
    const allRows = useMemo<ComponentRow[]>(
        () => [...view.framework, ...view.runtimeDep, ...view.selfApp],
        [view],
    );
    const machines = useMemo<MachineView[]>(() => {
        const grouped = groupByHost(allRows, hosts);
        return grouped.filter(
            (m) => m.framework.length + m.runtimeDep.length + m.selfApp.length > 0,
        );
    }, [allRows, hosts]);

    // 终态错误 push 进全局 InfoBar（顶层 InfoBarStack 渲染）。
    useComponentActionErrors(allRows);

    // 选中的主机：默认停在第一台（本机）。机器列表变动后若当前选中项消失，
    // 回落到第一台，避免选中一台已被移除的远端导致空白。
    const [activeHostId, setActiveHostId] = useState<string | null>(null);
    useEffect(() => {
        if (machines.length === 0) {
            if (activeHostId !== null) setActiveHostId(null);
            return;
        }
        const stillThere = machines.some((m) => m.host.host_id === activeHostId);
        if (!stillThere) setActiveHostId(machines[0].host.host_id);
    }, [machines, activeHostId]);

    const activeMachine = useMemo(
        () => machines.find((m) => m.host.host_id === activeHostId) ?? machines[0] ?? null,
        [machines, activeHostId],
    );

    const latestVersionFor = useCallback(
        (id: ComponentId): string | null => {
            switch (id) {
                case 'napcat':
                    return releases.napcat?.version ?? null;
                case 'snowluma':
                    return releases.snowluma?.version ?? null;
                case 'desktop_self':
                    return releases.desktop?.version ?? null;
                default:
                    return null;
            }
        },
        [releases],
    );

    const handleAction = useCallback(
        async (
            componentId: ComponentId,
            hostId: string,
            payload: { stepKind: import('../../core/ipc/types').StepKind } | { cancelTaskId: string },
        ) => {
            try {
                if ('cancelTaskId' in payload) {
                    await cancelAction(payload.cancelTaskId);
                    return;
                }
                const taskId = await startAction(componentId, hostId, payload.stepKind);
                onTaskTerminal(taskId, () => refetch());
            } catch (err) {
                console.error('[ComponentsPage] action failed:', err);
            }
        },
        [startAction, cancelAction, onTaskTerminal, refetch],
    );

    const allEmpty = machines.length === 0;

    return (
        <div className="flex min-h-0 flex-1 flex-col">
            <header className="flex shrink-0 items-end justify-between pb-4 pt-2">
                <div>
                    <p className="text-2xs uppercase tracking-widest text-text-tertiary">
                        components
                    </p>
                    <h1 className="font-display text-xl font-semibold text-text">组件管理</h1>
                    <p className="mt-1 text-sm text-text-secondary">
                        选一台机器，管理它上面的 Bot 框架与运行时依赖：安装、更新、卸载、容器部署。
                    </p>
                </div>
                <Button size="sm" variant="secondary" onClick={refetch} disabled={isLoading}>
                    <RefreshCw size={14} className={isLoading ? 'animate-spin' : undefined} />
                    刷新
                </Button>
            </header>

            {error && <ErrorBanner message={error.message} onRetry={refetch} />}

            {machines.length > 1 && activeHostId && (
                <HostSwitcher
                    machines={machines}
                    activeHostId={activeHostId}
                    onSelect={setActiveHostId}
                />
            )}

            <div className="-mx-2 mt-3 flex min-h-0 flex-1 flex-col overflow-y-auto px-2 pt-1 pb-6">
                {isLoading && allEmpty ? (
                    <SectionLoading />
                ) : activeMachine ? (
                    <HostComponentsView
                        machine={activeMachine}
                        latestVersionFor={latestVersionFor}
                        getProgress={getProgressFor}
                        onAction={handleAction}
                        onRetryDetect={() => refetch()}
                        dockerStatus={dockerHosts.statusByHost[activeMachine.host.host_id]}
                        isDockerProbing={dockerHosts.probingByHost[activeMachine.host.host_id] ?? false}
                        isInstallingDocker={dockerHosts.isInstalling}
                        onInstallDocker={(hostId) => {
                            void dockerHosts.install(hostId).catch((err) => {
                                console.error('[ComponentsPage] docker install failed:', err);
                            });
                        }}
                        onOpenDockerDownload={() => {
                            void dockerHosts.openDownloadPage().catch(() => undefined);
                        }}
                        isDeploying={dockerHosts.isDeploying}
                        onDeploy={dockerHosts.deploy}
                    />
                ) : null}
            </div>
        </div>
    );
};

const SectionLoading: React.FC = () => (
    <div className="flex items-center gap-2 rounded-md bg-inset/40 p-6 text-text-tertiary">
        <Loader2 size={16} className="animate-spin" />
        <span className="text-sm">加载中…</span>
    </div>
);

const ErrorBanner: React.FC<{ message: string; onRetry: () => void }> = ({ message, onRetry }) => (
    <div className="mb-3 flex shrink-0 items-center justify-between gap-3 rounded-md border border-danger/30 bg-danger-soft px-4 py-3">
        <div className="flex items-center gap-2">
            <Box size={16} className="text-danger" />
            <span className="text-sm text-text">加载组件清单失败：{message}</span>
        </div>
        <Button size="sm" variant="ghost" onClick={onRetry}>
            重试
        </Button>
    </div>
);

export default ComponentsPageNext;
