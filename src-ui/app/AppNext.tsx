// 新 UI 树根 = AppShell。
// 布局:TitleBar(透明) ─ [Sidebar | main]

import React, { useCallback, useEffect, useMemo, useState } from 'react';

import { CustomTitleBar } from '../shared/components/next/CustomTitleBar';
import { Sidebar, type AppRoute } from '../shared/components/next/Sidebar';
import { InfoBarStack, TooltipProvider } from '../shared/ui';
import { BootstrapPanelNext } from '../modules/bootstrap/BootstrapPanel.next';
import { BotPageNext } from '../modules/bot/BotPage.next';
import { ComponentsPageNext } from '../modules/components/ComponentsPage.next';
import { DockerPageNext } from '../modules/docker/DockerPage.next';
import { RemoteHostPanelNext } from '../modules/remote/RemoteHostPanel.next';
import { SettingsPageNext } from '../modules/settings/SettingsPage.next';
import { TaskQueuePageNext } from '../modules/task-queue/TaskQueuePage.next';
import { useServerManager } from '../hooks/remote/useServerManager';
import { useComponentActionEventBridge } from '../hooks/components/useComponentActionBridge';
import { useDockerDeployProgressBridge } from '../hooks/docker/useDockerDeployProgressBridge';
import { useDockerInstallProgressBridge } from '../hooks/docker/useDockerInstallProgressBridge';
import { useDockerStatusByHost } from '../hooks/docker/useDockerStatusByHost';
import { useDeploymentTaskBridge } from '../hooks/task-queue/useDeploymentTaskBridge';
import { useComponentsWarmup } from '../hooks/components/useComponents';
import { useHostConnectionEvents } from '../hooks/remote/useHostConnectionEvents';
import { useHostHealthAlerts } from '../hooks/remote/useHostHealthAlerts';
import { useGlobalInfoBars } from '../hooks/ui/useGlobalInfoBars';
import { useAppUiPreferencesBootstrap } from '../hooks/preferences/useAppUiPreferencesBootstrap';
import { useMotion } from '../hooks/preferences/useMotion';
import { useTaskQueue } from '../hooks/task-queue/useTaskQueue';
import type { TaskQueueSnapshot } from '../core/domain/task-queue/types';
import { dockerStatusSummary } from '../core/domain/docker/status';
import { PageTransition } from '../shared/ui/motion';
import { DesktopExitGate } from './DesktopExitGate';

/// 路由顺序,跟 Sidebar PRIMARY_NAV 对齐。PageTransition 用此判断切换方向。
const ROUTE_ORDER: ReadonlyArray<AppRoute> = [
    'overview',
    'bots',
    'components',
    'docker',
    'remote',
    'tasks',
    'settings',
];

export const AppNext: React.FC = () => {
    const [route, setRoute] = useState<AppRoute>('overview');
    const [collapsed, setCollapsed] = useState(true);

    useComponentActionEventBridge();
    useDockerDeployProgressBridge();
    useDockerInstallProgressBridge();
    useDeploymentTaskBridge();
    useComponentsWarmup();
    useHostConnectionEvents();
    useHostHealthAlerts();

    const { servers } = useServerManager();
    const dockerHostIds = useMemo(
        () => servers.map((p) => `remote:${p.id}`),
        [servers],
    );
    const dockerStatusByHost = useDockerStatusByHost(dockerHostIds);
    const showDocker = useMemo(
        () =>
            dockerHostIds.some((hostId) => {
                const status = dockerStatusByHost[hostId];
                return status ? dockerStatusSummary(status).ready : false;
            }),
        [dockerHostIds, dockerStatusByHost],
    );
    const hostLabels = useMemo(() => {
        const map: Record<string, string> = { local: '本机' };
        for (const p of servers) {
            map[`remote:${p.id}`] = p.name?.trim() || p.host?.trim() || p.id;
        }
        return map;
    }, [servers]);

    const taskQueue = useTaskQueue({ hostLabels });

    useAppUiPreferencesBootstrap();

    const { bars, dismiss, remove } = useGlobalInfoBars();

    useEffect(() => {
        if (!showDocker && route === 'docker') {
            setRoute('overview');
        }
    }, [showDocker, route]);

    const navigate = useCallback(
        (nextRoute: AppRoute) => {
            setRoute(nextRoute === 'docker' && !showDocker ? 'overview' : nextRoute);
        },
        [showDocker],
    );

    const [displayedRoute, setDisplayedRoute] = useState<AppRoute>(route);
    const [pageVisible, setPageVisible] = useState<boolean>(true);
    const [direction, setDirection] = useState<-1 | 0 | 1>(0);

    useEffect(() => {
        if (route === displayedRoute) {
            if (!pageVisible) setPageVisible(true);
            return;
        }
        const oldIdx = ROUTE_ORDER.indexOf(displayedRoute);
        const newIdx = ROUTE_ORDER.indexOf(route);
        const dir: -1 | 0 | 1 =
            oldIdx < 0 || newIdx < 0 ? 0 : newIdx > oldIdx ? 1 : newIdx < oldIdx ? -1 : 0;
        setDirection(dir);
        setPageVisible(false);
    }, [route, displayedRoute, pageVisible]);

    const handlePageExited = () => {
        setDisplayedRoute(route);
        setPageVisible(true);
    };

    const motion = useMotion();

    return (
        <TooltipProvider>
            <div className="flex h-screen w-screen flex-col overflow-hidden bg-canvas">
                <div className="relative flex flex-1 overflow-hidden">
                    <Sidebar
                        active={route}
                        onChange={navigate}
                        collapsed={collapsed}
                        onToggleCollapse={() => setCollapsed((v) => !v)}
                        showDocker={showDocker}
                        taskQueueActiveCount={taskQueue.activeCount}
                    />

                    <div className="relative flex flex-1 flex-col overflow-hidden">
                        <div
                            className={
                                'ndf-canvas-glow' +
                                (motion.preset.feel.overshoot &&
                                motion.enabled &&
                                route === 'overview'
                                    ? ' is-breathing'
                                    : '')
                            }
                        />

                        <CustomTitleBar />

                        <main className="relative z-10 flex min-w-0 flex-1 overflow-hidden">
                            <div className="flex min-w-0 w-full max-w-full flex-col px-4 pb-6 pt-2 sm:px-6 lg:px-8 xl:mx-auto xl:max-w-[1280px]">
                                <div className="flex min-h-0 min-w-0 flex-1 flex-col">
                                    <PageTransition
                                        visible={pageVisible}
                                        onExited={handlePageExited}
                                        direction={direction}
                                        className="flex min-h-0 min-w-0 flex-1 flex-col"
                                    >
                                        <RouteContent
                                            route={displayedRoute}
                                            onNavigate={navigate}
                                            taskQueue={taskQueue}
                                            showDocker={showDocker}
                                        />
                                    </PageTransition>
                                </div>
                            </div>
                        </main>
                    </div>
                </div>

                <InfoBarStack items={bars} onDismiss={dismiss} onAutoDismiss={remove} />
                <DesktopExitGate />
            </div>
        </TooltipProvider>
    );
};

const RouteContent: React.FC<{
    route: AppRoute;
    onNavigate: (route: AppRoute) => void;
    taskQueue: TaskQueueSnapshot;
    showDocker: boolean;
}> = ({ route, onNavigate, taskQueue, showDocker }) => {
    switch (route) {
        case 'overview':
            return <BootstrapPanelNext onNavigate={onNavigate} />;
        case 'bots':
            return <BotPageNext />;
        case 'components':
            return <ComponentsPageNext />;
        case 'docker':
            return <DockerPageNext />;
        case 'remote':
            return <RemoteHostPanelNext />;
        case 'tasks':
            return (
                <TaskQueuePageNext
                    items={taskQueue.items}
                    activeCount={taskQueue.activeCount}
                    onNavigate={onNavigate}
                    showDocker={showDocker}
                />
            );
        case 'settings':
            return <SettingsPageNext />;
        default: {
            const _exhaustive: never = route;
            void _exhaustive;
            return null;
        }
    }
};

export default AppNext;
