// 应用壳只组合导航、页面与桌面设施；各子系统自己拥有订阅和生命周期。
import { Suspense, lazy, useCallback, useEffect, useState } from 'react';
import { CustomTitleBar } from '../shared/components/next/CustomTitleBar';
import { Sidebar } from '../shared/components/next/Sidebar';
import { GlobalTitleTooltip, TooltipProvider } from '../shared/ui';
import { FLOAT_LAYER_ID } from '../shared/ui/BodyPortal';
import { PageTransition } from '../shared/ui/motion';
import { useMotion } from '../hooks/preferences/useMotion';
import { usePreferences } from '../hooks/preferences/usePreferences';
import { useFeatureEnabled } from '../hooks/preferences/featureTogglesStore';
import { useTaskQueueActiveCount } from '../hooks/task-queue/useTaskQueue';
import { useTerminalCoversPage } from '../hooks/terminal/terminalStore';
import { DesktopExitGate } from './DesktopExitGate';
import { AppRoutes } from './routing/AppRoutes';
import { useAppNavigation } from './routing/useAppNavigation';
import { AppBackgroundEffects } from './desktop/AppBackgroundEffects';
import { AppDesktopOverlays, AppNotifications } from './desktop/AppDesktopOverlays';
import type { AppRoute } from '../core/domain/ui/route';
import { perfMark } from '../core/domain/performance/perfMarks';

const WIDE_ROUTES: ReadonlySet<AppRoute> = new Set(['debug', 'chat']);
const FLUSH_ROUTES: ReadonlySet<AppRoute> = new Set(['chat']);
const TerminalDock = lazy(() =>
    import('../modules/terminal/TerminalDock').then((m) => ({ default: m.TerminalDock })),
);

export function AppNext() {
    useEffect(() => {
        perfMark('app_mounted', { once: true });
    }, []);
    const {
        route,
        displayedRoute,
        pageVisible,
        direction,
        navigate,
        prefetchRoute,
        hiddenRoutes,
        showDocker,
        onPageExited,
    } = useAppNavigation();
    const [collapsed, setCollapsed] = useState(true);
    const toggleCollapsed = useCallback(() => setCollapsed((value) => !value), []);
    const { sidebarStyle } = usePreferences();
    const taskQueueActiveCount = useTaskQueueActiveCount();
    const terminalCoversPage = useTerminalCoversPage();
    const terminalEnabled = useFeatureEnabled('terminal');
    const motion = useMotion();
    return (
        <TooltipProvider>
            <AppBackgroundEffects />
            <div className="flex h-screen w-screen flex-col overflow-hidden bg-canvas">
                <div className="relative flex flex-1 overflow-hidden">
                    <div
                        className={
                            'ndf-canvas-glow' +
                            (motion.preset.feel.overshoot && motion.enabled && route === 'overview'
                                ? ' is-breathing'
                                : '')
                        }
                    />
                    <div
                        className={
                            motion.enabled ? 'ndf-shell-enter-sidebar flex h-full' : 'flex h-full'
                        }
                    >
                        <Sidebar
                            active={route}
                            onChange={navigate}
                            onPrefetch={prefetchRoute}
                            collapsed={collapsed}
                            sidebarStyle={sidebarStyle}
                            onToggleCollapse={toggleCollapsed}
                            hiddenRoutes={hiddenRoutes}
                            taskQueueActiveCount={taskQueueActiveCount}
                        />
                    </div>

                    <div className="relative z-10 flex flex-1 flex-col overflow-hidden">
                        <div className={motion.enabled ? 'ndf-shell-enter-titlebar' : ''}>
                            <CustomTitleBar />
                        </div>

                        <main
                            className={
                                'relative isolate z-10 flex min-h-0 min-w-0 flex-1 overflow-hidden [contain:paint] ' +
                                (motion.enabled ? 'ndf-shell-enter-main' : '') +
                                (terminalCoversPage ? ' hidden' : '')
                            }
                        >
                            <div
                                className={
                                    'flex min-w-0 w-full max-w-full flex-col xl:mx-auto' +
                                    (FLUSH_ROUTES.has(displayedRoute)
                                        ? ''
                                        : ' px-4 pb-6 pt-2 sm:px-6 lg:px-8') +
                                    (WIDE_ROUTES.has(displayedRoute) ? '' : ' xl:max-w-[1280px]')
                                }
                            >
                                <div className="flex min-h-0 min-w-0 flex-1 flex-col">
                                    <PageTransition
                                        visible={pageVisible}
                                        onExited={onPageExited}
                                        direction={direction}
                                        className="flex min-h-0 min-w-0 flex-1 flex-col"
                                    >
                                        <AppRoutes
                                            route={displayedRoute}
                                            onNavigate={navigate}
                                            showDocker={showDocker}
                                        />
                                    </PageTransition>
                                </div>
                            </div>
                        </main>

                        {/* 贴底悬浮按钮的挂载槽：在 main（z-10）之上、终端面板之下 */}
                        <div id={FLOAT_LAYER_ID} className="contents" />

                        {terminalEnabled && (
                            <Suspense fallback={null}>
                                <TerminalDock />
                            </Suspense>
                        )}
                    </div>
                </div>

                <AppNotifications />
                <DesktopExitGate />

                <AppDesktopOverlays navigate={navigate} setSidebarCollapsed={setCollapsed} />

                <GlobalTitleTooltip />
            </div>
        </TooltipProvider>
    );
}

export default AppNext;
