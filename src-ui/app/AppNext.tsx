// 新 UI 树根 = AppShell。
// 布局：TitleBar(透明) ─ [Sidebar | main] ─ StatusBar(极弱)
//
// 简单 useState 路由（5 主路由 + 1 dev showcase）。

import React, { useState } from 'react';
import { Activity, Server, Settings as SettingsIcon } from 'lucide-react';
import './index.css';

import { CustomTitleBar } from '../shared/components/next/CustomTitleBar';
import { Sidebar, type AppRoute } from '../shared/components/next/Sidebar';
import { StatusBar } from '../shared/components/next/StatusBar';
import { InfoBarStack, TooltipProvider } from '../shared/ui';
import { PagePlaceholder } from '../shared/components/next/PagePlaceholder';
import { BootstrapPanelNext } from '../modules/bootstrap/BootstrapPanel.next';
import { ComponentsPageNext } from '../modules/components/ComponentsPage.next';
import { BotPageNext } from '../modules/bot/BotPage.next';
import { Showcase } from './Showcase';
import { useBootstrap } from '../hooks/bootstrap/useBootstrap';
import { useComponentActionEventBridge } from '../hooks/components/useComponentActionBridge';
import { useGlobalInfoBars } from '../hooks/ui/useGlobalInfoBars';

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

    // 全局 InfoBar：所有页面 / hook / service 通过 useGlobalInfoBars().push 或
    // pushInfoBar() 推条目，这里是整个 App 唯一的渲染处。模块级 store 跟组件
    // 树解耦，路由切换不会丢 banner。
    const { bars, dismiss } = useGlobalInfoBars();

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
        case 'remote':
            return (
                <PagePlaceholder
                    title="Remote Hosts"
                    icon={Server}
                    description="通过 SSH 接管远端 Linux 主机上的 NapCat 部署。"
                    pendingItems={[
                        'SSH 连接表单 + 已连接信息卡',
                        '远端 runtime 监控（PID / RSS / 活跃连接数）',
                        'SFTP 只读浏览',
                        '远端 WebUI 跳转 / 重启容器',
                    ]}
                />
            );
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
            return (
                <PagePlaceholder
                    title="Settings"
                    icon={SettingsIcon}
                    description="客户端偏好 / 系统环境 / 数据目录。"
                    pendingItems={[
                        '主题切换（暖粉浅色 / 暖夜暗色）',
                        '资源监视采样间隔',
                        'mascot 显示开关',
                        '关于 / 版本信息',
                    ]}
                />
            );
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
