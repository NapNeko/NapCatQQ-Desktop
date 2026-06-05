// 新 UI 树根 = AppShell。
// 布局:TitleBar(透明) ─ [Sidebar | main] ─ StatusBar(极弱)

import React, { useEffect, useState } from 'react';

import { CustomTitleBar } from '../shared/components/next/CustomTitleBar';
import { Sidebar, type AppRoute } from '../shared/components/next/Sidebar';
import { StatusBar } from '../shared/components/next/StatusBar';
import { InfoBarStack, TooltipProvider } from '../shared/ui';
import { BootstrapPanelNext } from '../modules/bootstrap/BootstrapPanel.next';
import { BotPageNext } from '../modules/bot/BotPage.next';
import { ComponentsPageNext } from '../modules/components/ComponentsPage.next';
import { DockerPageNext } from '../modules/docker/DockerPage.next';
import { RemoteHostPanelNext } from '../modules/remote/RemoteHostPanel.next';
import { SettingsPageNext } from '../modules/settings/SettingsPage.next';
import { useServerManager } from '../hooks/remote/useServerManager';
import { useComponentActionEventBridge } from '../hooks/components/useComponentActionBridge';
import { useDockerDeployProgressBridge } from '../hooks/docker/useDockerDeployProgressBridge';
import { useComponentsWarmup } from '../hooks/components/useComponents';
import { useGlobalInfoBars } from '../hooks/ui/useGlobalInfoBars';
import { useTrayCloseActionSync } from '../hooks/desktop/useTrayCloseActionSync';
import { applySideEffects } from '../hooks/preferences/preferencesStore';
import { useMotion } from '../hooks/preferences/useMotion';
import { APP_VERSION_LABEL } from '../core/domain/app-meta';
import { PageTransition } from '../shared/ui/motion';

/// 路由顺序,跟 Sidebar PRIMARY_NAV 对齐。PageTransition 用此判断切换方向。
const ROUTE_ORDER: ReadonlyArray<AppRoute> = [
    'overview',
    'bots',
    'components',
    'docker',
    'remote',
    'settings',
];

export const AppNext: React.FC = () => {
    const [route, setRoute] = useState<AppRoute>('overview');
    const [collapsed, setCollapsed] = useState(false);

    useComponentActionEventBridge();
    useDockerDeployProgressBridge();
    useComponentsWarmup();

    useEffect(() => {
        applySideEffects();
    }, []);

    useTrayCloseActionSync();

    const { bars, dismiss } = useGlobalInfoBars();

    const { servers } = useServerManager();
    const showDocker = servers.length > 0;
    useEffect(() => {
        if (!showDocker && route === 'docker') {
            setRoute('overview');
        }
    }, [showDocker, route]);

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
                        onChange={setRoute}
                        collapsed={collapsed}
                        onToggleCollapse={() => setCollapsed((v) => !v)}
                        showDocker={showDocker}
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

                        <main className="relative z-10 flex flex-1 overflow-hidden">
                            <div className="flex w-full flex-col px-4 pb-6 pt-2 sm:px-6 lg:px-8 xl:mx-auto xl:max-w-[1280px]">
                                <div className="flex min-h-0 flex-1 flex-col">
                                    <PageTransition
                                        visible={pageVisible}
                                        onExited={handlePageExited}
                                        direction={direction}
                                        className="flex min-h-0 flex-1 flex-col"
                                    >
                                        <RouteContent route={displayedRoute} />
                                    </PageTransition>
                                </div>
                            </div>
                        </main>
                    </div>
                </div>

                <StatusBar appVersion={APP_VERSION_LABEL} />

                <InfoBarStack items={bars} onDismiss={dismiss} />
            </div>
        </TooltipProvider>
    );
};

const RouteContent: React.FC<{ route: AppRoute }> = ({ route }) => {
    switch (route) {
        case 'overview':
            return <BootstrapPanelNext />;
        case 'bots':
            return <BotPageNext />;
        case 'components':
            return <ComponentsPageNext />;
        case 'docker':
            return <DockerPageNext />;
        case 'remote':
            return <RemoteHostPanelNext />;
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