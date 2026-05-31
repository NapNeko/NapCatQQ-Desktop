// 新 UI 树根 = AppShell。
// 布局:TitleBar(透明) ─ [Sidebar | main] ─ StatusBar(极弱)
//
// 简单 useState 路由(6 主路由 + 1 dev showcase)。
// 路由切换走 GsapPresence + PageTransition,旧页跑完 exit 才真 unmount。

import React, { useEffect, useState } from 'react';
import './index.css';

import { CustomTitleBar } from '../shared/components/next/CustomTitleBar';
import { Sidebar, type AppRoute } from '../shared/components/next/Sidebar';
import { StatusBar } from '../shared/components/next/StatusBar';
import { InfoBarStack, TooltipProvider } from '../shared/ui';
import { BootstrapPanelNext } from '../modules/bootstrap/BootstrapPanel.next';
import { ComponentsPageNext } from '../modules/components/ComponentsPage.next';
import { DockerPageNext } from '../modules/docker/DockerPage.next';
import { BotPageNext } from '../modules/bot/BotPage.next';
import { RemoteHostPanelNext } from '../modules/remote/RemoteHostPanel.next';
import { SettingsPageNext } from '../modules/settings/SettingsPage.next';
import { Showcase } from './Showcase';
import { useBootstrap } from '../hooks/bootstrap/useBootstrap';
import { useServerManager } from '../hooks/remote/useServerManager';
import { useComponentActionEventBridge } from '../hooks/components/useComponentActionBridge';
import { useDockerDeployProgressBridge } from '../hooks/docker/useDockerDeployProgressBridge';
import { useComponentsWarmup } from '../hooks/components/useComponents';
import { useGlobalInfoBars } from '../hooks/ui/useGlobalInfoBars';
import { applySideEffects as applyPreferences } from '../hooks/preferences/preferencesStore';
import { useMotion } from '../hooks/preferences/useMotion';
import { PageTransition } from '../shared/ui/motion';

// Showcase 是 dev-only 的原子件预览页。
const SHOW_SHOWCASE = import.meta.env.DEV;

export const AppNext: React.FC = () => {
    const [route, setRoute] = useState<AppRoute>('overview');
    const [collapsed, setCollapsed] = useState(false);

    const { bootstrap, isLoading, error } = useBootstrap();
    const connectionState: 'connected' | 'connecting' | 'disconnected' = error
        ? 'disconnected'
        : isLoading
            ? 'connecting'
            : 'connected';

    useComponentActionEventBridge();
    useDockerDeployProgressBridge();
    useComponentsWarmup();

    useEffect(() => {
        applyPreferences();
    }, []);

    const { bars, dismiss } = useGlobalInfoBars();

    const { servers } = useServerManager();
    const showDocker = servers.length > 0;
    useEffect(() => {
        if (!showDocker && route === 'docker') {
            setRoute('overview');
        }
    }, [showDocker, route]);

    // 用于 PageTransition 的 visible 控制:每个路由有自己的 PageTransition,
    // visible=(route===self) 决定 enter/exit。但若同时挂多个 PageTransition,
    // 切换时旧 visible=false 跑 exit + 新 visible=true 跑 enter,可能短暂
    // 重叠在同一容器位置。我们用 currentRoute + transitioningRoute 双 state
    // 实现"严格 wait":先让旧 route 跑完 exit,再切到新 route。
    // 简化做法:只挂一个 PageTransition,key=route,GsapPresence 会先 exit
    // 旧节点(visible=false 的瞬间)再 enter 新节点。但 GsapPresence 设计是
    // 单实例 visible 切换,不是 key 切换 → 需要外层用 sequence pattern。
    // 直接做:用 displayedRoute 跟踪当前在 DOM 里的路由,route 改变时先把
    // displayedRoute 对应的 PageTransition 设 visible=false 跑 exit,完成后
    // 把 displayedRoute 设为新 route + visible=true 跑 enter。
    const [displayedRoute, setDisplayedRoute] = useState<AppRoute>(route);
    const [pageVisible, setPageVisible] = useState<boolean>(true);

    useEffect(() => {
        if (route === displayedRoute) {
            // 已经在这个路由,确保 visible=true(初次或回切)
            if (!pageVisible) setPageVisible(true);
            return;
        }
        // route 变了:先把当前页跑 exit
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
                        showShowcase={SHOW_SHOWCASE}
                        showDocker={showDocker}
                    />

                    <div className="relative flex flex-1 flex-col overflow-hidden">
                        {/* 角落柔光只覆盖右侧主内容区,不污染 sidebar。
                            rich 档 + 当前在 overview 时叠 is-breathing 8s 呼吸。 */}
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
                                        className="flex min-h-0 flex-1 flex-col"
                                    >
                                        <RouteContent route={displayedRoute} />
                                    </PageTransition>
                                </div>
                            </div>
                        </main>
                    </div>
                </div>

                <StatusBar
                    connectionState={connectionState}
                    dataRoot={bootstrap?.data_root || undefined}
                />

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
        case 'showcase':
            return <Showcase />;
        default: {
            const _exhaustive: never = route;
            void _exhaustive;
            return null;
        }
    }
};

export default AppNext;
