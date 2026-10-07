// 调试台弹出窗（label = debug-console）的根组件：只装调试台一页，不带主窗壳
// （侧栏 / 路由切换 / 终端面板 / 协议与引导闸门）。
//
// 页面没有 onNavigate：去别的页（Bot 管理、通道「去哪解决」）的入口自动不画。
// 偏好（主题 / 动画 / 圆角）在渲染前已由 main.tsx 从后端水合好。

import React, { Suspense, lazy, useEffect, useRef } from 'react';
import './index.css';
import { TitleBarChrome } from '../shared/components/next/TitleBarChrome';
import { GlobalTitleTooltip, InfoBarStack, TooltipProvider } from '../shared/ui';
// fallback 直引，避免只为 Spinner 再钉死整个 shared/ui barrel 图。
import { PagePlaceholder } from '../shared/ui/PagePlaceholder';
import { Spinner } from '../shared/ui/Spinner';
import { RouteErrorBoundary } from '../shared/ui/RouteErrorBoundary';
import { useGlobalInfoBars } from '../hooks/ui/useGlobalInfoBars';
import { debugWindowService } from '../core/services/debug-window.service';

const DebugConsolePage = lazy(() =>
    import('../modules/debug/DebugConsolePage').then((m) => ({ default: m.DebugConsolePage })),
);

function PopoutFallback() {
    return (
        <PagePlaceholder>
            <Spinner size="md" tone="brand" label="页面加载中" />
            <p className="text-[13px] text-text-secondary">正在加载页面…</p>
        </PagePlaceholder>
    );
}

export const DebugPopoutApp: React.FC = () => {
    const { bars, dismiss, remove } = useGlobalInfoBars();

    // 窗口起步隐藏（克隆主窗配置），首帧上屏后再叫后端显示，防透明壳闪白。
    // 对齐主窗 AppBootGate 的双 rAF 时序
    const revealedRef = useRef(false);
    useEffect(() => {
        if (revealedRef.current) return;
        revealedRef.current = true;
        let inner = 0;
        const outer = requestAnimationFrame(() => {
            inner = requestAnimationFrame(() => {
                void debugWindowService.reveal().catch((err) => {
                    console.error('[DebugPopout] 显示窗口失败:', err);
                });
            });
        });
        return () => {
            cancelAnimationFrame(outer);
            cancelAnimationFrame(inner);
        };
    }, []);

    return (
        <TooltipProvider>
            <div className="flex h-screen w-screen flex-col overflow-hidden bg-canvas">
                <div className="relative flex flex-1 flex-col overflow-hidden">
                    <div className="ndf-canvas-glow" />

                    <TitleBarChrome tool />

                    <main className="relative z-10 flex min-w-0 flex-1 overflow-hidden">
                        <div className="flex min-w-0 w-full max-w-full flex-col px-4 pb-6 pt-2 sm:px-6 lg:px-8">
                            <div className="flex min-h-0 min-w-0 flex-1 flex-col">
                                <RouteErrorBoundary title="调试台渲染失败">
                                    <Suspense fallback={<PopoutFallback />}>
                                        <DebugConsolePage />
                                    </Suspense>
                                </RouteErrorBoundary>
                            </div>
                        </div>
                    </main>
                </div>

                <InfoBarStack items={bars} onDismiss={dismiss} onAutoDismiss={remove} />
                <GlobalTitleTooltip />
            </div>
        </TooltipProvider>
    );
};

export default DebugPopoutApp;
