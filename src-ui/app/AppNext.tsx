// 新 UI 树根 = AppShell。
// 布局：TitleBar(透明) ─ [Sidebar | main] ─ StatusBar(极弱)
//
// 简单 useState 路由（5 主路由 + 1 dev showcase）。

import React, { useEffect, useState } from 'react';
import { Activity } from 'lucide-react';
import './index.css';

import { CustomTitleBar } from '../shared/components/next/CustomTitleBar';
import { Sidebar, type AppRoute } from '../shared/components/next/Sidebar';
import { StatusBar } from '../shared/components/next/StatusBar';
import { InfoBarStack, TooltipProvider } from '../shared/ui';
import { PagePlaceholder } from '../shared/components/next/PagePlaceholder';
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
import { useComponentsWarmup } from '../hooks/components/useComponents';
import { useGlobalInfoBars } from '../hooks/ui/useGlobalInfoBars';
import { applySideEffects as applyPreferences } from '../hooks/preferences/preferencesStore';

const SHOW_SHOWCASE = true;

export const AppNext: React.FC = () => {
    const [route, setRoute] = useState<AppRoute>('overview');
    const [collapsed, setCollapsed] = useState(false);

    // useBootstrap 提供 data_root 与连接状态推导：
    //   - isLoading → 'connecting'
    //   - 有数据 → 'connected'
    //   - 出错 → 'disconnected'
    const { bootstrap, isLoading, error } = useBootstrap();
    const connectionState: 'connected' | 'connecting' | 'disconnected' = error
        ? 'disconnected'
        : isLoading
            ? 'connecting'
            : 'connected';

    // 顶层挂一次 component-action 事件桥。路由切换不会断订阅，进度状态留在
    // 模块级 store；切走 Components 页再切回来不会丢已经在跑的安装进度。
    useComponentActionEventBridge();

    // 启动即后台预热组件探测：拉服务器列表 + 自动连接远端 + catalog + 逐主机
    // detect，全在 App 根节点常驻跑。用户切到组件页时数据已在 react-query 缓存，
    // 秒开，不用再从零等一轮 SSH 探测。
    useComponentsWarmup();

    // 启动时 apply 一次客户端偏好（主题 / 窗口透明度等）。preferencesStore
    // 已经从 localStorage 加载初始值，这里只是把它落到 DOM。后续通过
    // store update 会自动触发 applySideEffects，不需要重复挂监听。
    useEffect(() => {
        applyPreferences();
    }, []);

    // 全局 InfoBar：所有页面 / hook / service 通过 useGlobalInfoBars().push 或
    // pushInfoBar() 推条目，这里是整个 App 唯一的渲染处。模块级 store 跟组件
    // 树解耦，路由切换不会丢 banner。
    const { bars, dismiss } = useGlobalInfoBars();

    // Docker 项门控：只有添加了远端服务器才显示。Docker 只用于管理远端 Linux
    // 容器，本机（Windows）用不上。当前停在 docker 页时若远端被删光，回落到
    // overview，避免停在一个已隐藏的空页。
    const { servers } = useServerManager();
    const showDocker = servers.length > 0;
    useEffect(() => {
        if (!showDocker && route === 'docker') {
            setRoute('overview');
        }
    }, [showDocker, route]);

    return (
        <TooltipProvider>
            <div className="flex h-screen w-screen flex-col overflow-hidden bg-canvas">
                {/* 主体行：Sidebar 从 y=0 开始顶到头，
                    TitleBar 浮在右侧顶端，和 sidebar header 同一水平面 */}
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
                        {/* 角落柔光只覆盖右侧主内容区，不污染 sidebar */}
                        <div className="ndf-canvas-glow" />

                        <CustomTitleBar />

                        <main className="relative z-10 flex flex-1 overflow-hidden">
                            <div className="flex w-full flex-col px-4 pb-6 pt-2 sm:px-6 lg:px-8 xl:mx-auto xl:max-w-[1280px]">
                                <div className="flex min-h-0 flex-1 flex-col">
                                    <RouteOutlet route={route} />
                                </div>
                            </div>
                        </main>
                    </div>
                </div>

                <StatusBar
                    connectionState={connectionState}
                    dataRoot={bootstrap?.data_root || undefined}
                />

                {/* 全局 InfoBar 队列：portal 到 body，固定右上角；任何页面调
                    pushInfoBar / useGlobalInfoBars().push 都汇集到这一处渲染。 */}
                <InfoBarStack items={bars} onDismiss={dismiss} />
            </div>
        </TooltipProvider>
    );
};

const RouteOutlet: React.FC<{ route: AppRoute }> = ({ route }) => {
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
        case 'events':
            return (
                <PagePlaceholder
                    title="Events"
                    icon={Activity}
                    description="17 种 Domain 事件的实时流 + payload 调试器。"
                    pendingItems={[
                        '事件流时间线（最近 100 条）',
                        'kind 过滤下拉',
                        '单事件 payload JSON viewer',
                        '可选：常驻右侧抽屉模式',
                    ]}
                />
            );
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
