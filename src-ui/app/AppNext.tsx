// 新 UI 树根 = AppShell。
// 布局:TitleBar(透明) ─ [Sidebar | main]
// overview 首屏同步加载；其余业务路由 lazy，降低主包解析成本。
// spotlight 等锚点仍靠 waitForTourTarget，lazy 挂载延迟可接受。
// 侧栏 hover/focus 预取对应 chunk；点击导航直接 setRoute（不走 startTransition）。

import React, {
    Suspense,
    lazy,
    memo,
    startTransition,
    useCallback,
    useEffect,
    useMemo,
    useRef,
    useState,
} from 'react';

import { CustomTitleBar } from '../shared/components/next/CustomTitleBar';
import { Sidebar, type AppRoute } from '../shared/components/next/Sidebar';
import { GlobalTitleTooltip, InfoBarStack, TooltipProvider } from '../shared/ui';
// fallback 直引，避免只为 Spinner 再钉死整个 shared/ui barrel 图。
import { PagePlaceholder } from '../shared/ui/PagePlaceholder';
import { Spinner } from '../shared/ui/Spinner';
import { BootstrapPanelNext } from '../modules/bootstrap/BootstrapPanel.next';
import { useServerManager } from '../hooks/remote/useServerManager';
import { useComponentActionEventBridge } from '../hooks/components/useComponentActionBridge';
import { useDockerDeployProgressBridge } from '../hooks/docker/useDockerDeployProgressBridge';
import { useDockerInstallProgressBridge } from '../hooks/docker/useDockerInstallProgressBridge';
import { useDockerStatusByHost } from '../hooks/docker/useDockerStatusByHost';
import { useDeploymentTaskBridge } from '../hooks/task-queue/useDeploymentTaskBridge';
import { useAppInstanceEventsBridge } from '../hooks/apps/useAppInstanceEventsBridge';
import { useComponentsWarmup } from '../hooks/components/useComponents';
import { useHostConnectionEvents } from '../hooks/remote/useHostConnectionEvents';
import { useHostHealthAlerts } from '../hooks/remote/useHostHealthAlerts';
import { useGlobalInfoBars } from '../hooks/ui/useGlobalInfoBars';
import { useAppUiPreferencesBootstrap } from '../hooks/preferences/useAppUiPreferencesBootstrap';
import { useMotion } from '../hooks/preferences/useMotion';
import { usePreferences } from '../hooks/preferences/usePreferences';
import { useTaskQueue, useTaskQueueActiveCount } from '../hooks/task-queue/useTaskQueue';
import { terminalStore, useTerminalCoversPage } from '../hooks/terminal/terminalStore';
import { featureTogglesStore, useFeatures } from '../hooks/preferences/featureTogglesStore';
import { useDebugConsoleEnabled } from '../hooks/debug/useDebugConsoleEnabled';
import { registerDebugNavigator } from '../hooks/debug/debugNav';
import { dockerStatusSummary } from '../core/domain/docker/status';
import { PageTransition } from '../shared/ui/motion';
import { DesktopExitGate } from './DesktopExitGate';
import { useBootstrap } from '../hooks/bootstrap/useBootstrap';
import { useDataLayoutConsolidateAlert } from '../hooks/bootstrap/useDataLayoutConsolidateAlert';
import { useDesktopConsentGate } from '../hooks/desktop/useDesktopConsentGate';
import { useOnboardingGate } from '../hooks/desktop/useOnboardingGate';
import { registerOnboardingHost } from '../hooks/desktop/onboardingHost';
import { registerDesktopConsentHost } from '../hooks/desktop/desktopConsentHost';
import { useFrameworkTour } from '../hooks/desktop/useFrameworkTour';
import { useAppWindowBridge } from '../hooks/desktop/useAppWindowBridge';
import { useDesktopUpdateStartupNotice } from '../hooks/desktop/useDesktopUpdate';
import { DesktopConsentDialog } from '../shared/components/next/DesktopConsentDialog';
import {
    ONBOARDING_GUIDE_STEP_IDS,
    OnboardingDialog,
} from '../shared/components/next/OnboardingDialog';
import { OnboardingContinueDialog } from '../shared/components/next/OnboardingContinueDialog';
import { SpotlightTour } from '../shared/components/next/SpotlightTour';
import { perfMark } from '../core/domain/performance/perfMarks';

// 路由顺序,跟 Sidebar PRIMARY_NAV 对齐。PageTransition 用此判断切换方向。
const ROUTE_ORDER: ReadonlyArray<AppRoute> = [
    'overview',
    'bots',
    'chat',
    'apps',
    'debug',
    'components',
    'docker',
    'remote',
    'tasks',
    'settings',
];

// 这些页面不受 1280px 的宽度上限：调试台是三栏工作台，宽屏上越宽越好用
const WIDE_ROUTES: ReadonlySet<AppRoute> = new Set(['debug', 'chat']);
// 聊天自己贴着窗口边排版，不吃页面内边距。
const FLUSH_ROUTES: ReadonlySet<AppRoute> = new Set(['chat']);

// 与 lazy 共用同一 import 工厂，侧栏预取与首点加载同一 chunk。
const loadBotPage = () =>
    import('../modules/bot/BotPage.next').then((m) => ({ default: m.BotPageNext }));
const loadAppsPage = () =>
    import('../modules/apps/AppsPage.next').then((m) => ({ default: m.AppsPageNext }));
const loadChatPage = () =>
    import('../modules/chat/ChatPage').then((m) => ({ default: m.ChatPage }));
const loadDebugPage = () =>
    import('../modules/debug/DebugConsolePage').then((m) => ({ default: m.DebugConsolePage }));
const loadComponentsPage = () =>
    import('../modules/components/ComponentsPage.next').then((m) => ({
        default: m.ComponentsPageNext,
    }));
const loadDockerPage = () =>
    import('../modules/docker/DockerPage.next').then((m) => ({ default: m.DockerPageNext }));
const loadRemotePage = () =>
    import('../modules/remote/RemoteHostPanel.next').then((m) => ({
        default: m.RemoteHostPanelNext,
    }));
const loadSettingsPage = () =>
    import('../modules/settings/SettingsPage.next').then((m) => ({
        default: m.SettingsPageNext,
    }));
const loadTaskQueuePage = () =>
    import('../modules/task-queue/TaskQueuePage.next').then((m) => ({
        default: m.TaskQueuePageNext,
    }));

const BotPageNext = lazy(loadBotPage);
const AppsPageNext = lazy(loadAppsPage);
const ChatPage = lazy(loadChatPage);
const DebugConsolePage = lazy(loadDebugPage);
const ComponentsPageNext = lazy(loadComponentsPage);
const DockerPageNext = lazy(loadDockerPage);
const RemoteHostPanelNext = lazy(loadRemotePage);
const SettingsPageNext = lazy(loadSettingsPage);
const TaskQueuePageNext = lazy(loadTaskQueuePage);
// 终端面板带着 xterm，单独一块懒加载，不压首屏
const TerminalDock = lazy(() =>
    import('../modules/terminal/TerminalDock').then((m) => ({ default: m.TerminalDock })),
);

const ROUTE_PRELOAD: Partial<Record<AppRoute, () => Promise<unknown>>> = {
    bots: loadBotPage,
    apps: loadAppsPage,
    chat: loadChatPage,
    debug: loadDebugPage,
    components: loadComponentsPage,
    docker: loadDockerPage,
    remote: loadRemotePage,
    settings: loadSettingsPage,
    tasks: loadTaskQueuePage,
};

function preloadRoute(route: AppRoute): void {
    const load = ROUTE_PRELOAD[route];
    if (load) void load();
}

const NO_HOSTS: string[] = [];

function RouteFallback() {
    return (
        <PagePlaceholder>
            <Spinner size="md" tone="brand" label="页面加载中" />
            <p className="text-[13px] text-text-secondary">正在加载页面…</p>
        </PagePlaceholder>
    );
}

export const AppNext: React.FC = () => {
    const [route, setRoute] = useState<AppRoute>('overview');
    const [displayedRoute, setDisplayedRoute] = useState<AppRoute>(route);
    const [pageVisible, setPageVisible] = useState<boolean>(true);
    // 弹出窗事件要做的路由切换：每组动作与原内联 effect 里的 setState 批次一一对应
    const showChatPage = useCallback(() => {
        if (!featureTogglesStore.getSnapshot().chat) return;
        setRoute('chat');
        setDisplayedRoute('chat');
        setPageVisible(true);
    }, []);
    const showChatPageQuiet = useCallback(() => {
        if (!featureTogglesStore.getSnapshot().chat) return;
        setRoute('chat');
        setDisplayedRoute('chat');
    }, []);
    const showOverviewPage = useCallback(() => {
        setRoute('overview');
        setDisplayedRoute('overview');
        setPageVisible(true);
    }, []);
    const { focusChatIfOpen, focusDebugIfOpen } = useAppWindowBridge({
        route,
        showChat: showChatPage,
        showChatQuiet: showChatPageQuiet,
        showOverview: showOverviewPage,
    });
    const [collapsed, setCollapsed] = useState(true);
    const { sidebarStyle } = usePreferences();
    const debugEnabled = useDebugConsoleEnabled();

    useEffect(() => {
        perfMark('app_mounted', { once: true });
    }, []);

    useComponentActionEventBridge();
    useDockerDeployProgressBridge();
    useDockerInstallProgressBridge();
    useDeploymentTaskBridge();
    useAppInstanceEventsBridge();
    useComponentsWarmup();
    useHostConnectionEvents();
    useHostHealthAlerts();

    const { servers } = useServerManager();
    const dockerHostIds = useMemo(() => servers.map((p) => `remote:${p.id}`), [servers]);
    const features = useFeatures();
    // 容器页关了就不去每台远端探 Docker，这份探测只为决定侧栏要不要显示容器页
    const dockerProbeHostIds = features.dockerPage ? dockerHostIds : NO_HOSTS;
    const dockerStatusByHost = useDockerStatusByHost(dockerProbeHostIds);
    const showDocker = useMemo(
        () =>
            dockerProbeHostIds.some((hostId) => {
                const status = dockerStatusByHost[hostId];
                return status ? dockerStatusSummary(status).ready : false;
            }),
        [dockerProbeHostIds, dockerStatusByHost],
    );
    // 侧栏不显示、也不许切过去的页
    const hiddenRoutes = useMemo(() => {
        const hidden = new Set<AppRoute>();
        if (!showDocker) hidden.add('docker');
        if (!features.apps) hidden.add('apps');
        if (!features.chat) hidden.add('chat');
        if (!debugEnabled) hidden.add('debug');
        return hidden;
    }, [showDocker, features.apps, features.chat, debugEnabled]);
    const hostLabels = useMemo(() => {
        const map: Record<string, string> = { local: '本机' };
        for (const p of servers) {
            map[`remote:${p.id}`] = p.name?.trim() || p.host?.trim() || p.id;
        }
        return map;
    }, [servers]);

    // 根组件只订阅任务数：整份队列每条进度事件都换新快照，订阅它会让整页跟着重渲
    const taskQueueActiveCount = useTaskQueueActiveCount();
    const terminalCoversPage = useTerminalCoversPage();

    useAppUiPreferencesBootstrap();

    const { bootstrap } = useBootstrap();
    useDataLayoutConsolidateAlert(bootstrap);

    const desktopConsent = useDesktopConsentGate();
    const onboarding = useOnboardingGate();
    const consentWasBlockingRef = useRef(false);

    const { bars, dismiss, remove } = useGlobalInfoBars();

    useEffect(() => {
        if (hiddenRoutes.has(route)) {
            startTransition(() => setRoute('overview'));
        }
    }, [hiddenRoutes, route]);

    // 关了内嵌终端就把开着的都关掉，面板不挂以后它们没处看也没处关
    useEffect(() => {
        if (!features.terminal) void terminalStore.closeAll();
    }, [features.terminal]);

    const navigate = useCallback(
        (nextRoute: AppRoute) => {
            const target = hiddenRoutes.has(nextRoute) ? 'overview' : nextRoute;
            if (target === 'chat') {
                void focusChatIfOpen().then((focused) => {
                    if (!focused && featureTogglesStore.getSnapshot().chat) setRoute('chat');
                });
                return;
            }
            // 调试台弹出窗开着时主窗不进调试页（工作区 / 收藏落盘 JSON 是两窗同一份文件，
            // 两边同时写会互相盖），入口一律把弹出窗叫到前面；没开着才正常切路由
            if (target === 'debug') {
                void focusDebugIfOpen().then((focused) => {
                    if (!focused) setRoute('debug');
                });
                return;
            }
            // 侧栏点击必须是紧急更新：详情页一旦有持续 setState，startTransition 会一直交不出去。
            setRoute(target);
        },
        [hiddenRoutes, focusChatIfOpen, focusDebugIfOpen],
    );

    const prefetchRoute = useCallback(
        (nextRoute: AppRoute) => {
            if (hiddenRoutes.has(nextRoute)) return;
            preloadRoute(nextRoute);
        },
        [hiddenRoutes],
    );

    // Bot 卡片「调试」按钮 / 右键「在调试台打开」经 debugNav 跳过来
    useEffect(() => registerDebugNavigator(() => navigate('debug')), [navigate]);

    const toggleCollapsed = useCallback(() => setCollapsed((v) => !v), []);

    useEffect(() => {
        void (async () => {
            const ok = await desktopConsent.promptAtStartup();
            // 已同意：直接尝试入门选择；未同意则等 accept 后由 blocking 回落触发
            if (ok) await onboarding.promptAfterConsent();
        })();
        // 仅挂载时检查一次
        // eslint-disable-next-line react-hooks/exhaustive-deps -- startup once
    }, []);

    useDesktopUpdateStartupNotice();

    useEffect(() => {
        if (desktopConsent.blocking) {
            consentWasBlockingRef.current = true;
            return;
        }
        if (!consentWasBlockingRef.current) return;
        if (desktopConsent.open) return;
        consentWasBlockingRef.current = false;
        void onboarding.promptAfterConsent();
        // eslint-disable-next-line react-hooks/exhaustive-deps -- only react to consent gate
    }, [desktopConsent.blocking, desktopConsent.open]);

    useEffect(() => {
        onboarding.setNavigate(navigate);
        return () => onboarding.setNavigate(null);
        // eslint-disable-next-line react-hooks/exhaustive-deps -- setNavigate stable
    }, [navigate]);

    const frameworkTour = useFrameworkTour({
        navigate,
        setSidebarCollapsed: setCollapsed,
    });

    /** 组件遮罩结束后的「接下来」介绍层（风格对齐第一步） */
    const [continueOpen, setContinueOpen] = useState(false);

    /** 整条引导：Dialog 认路结束后接组件页遮罩（NC/SL + 演示远端） */
    const continueOnboardingFlow = useCallback(async () => {
        await onboarding.finishGuide([...ONBOARDING_GUIDE_STEP_IDS]);
        setContinueOpen(false);
        await frameworkTour.start({ mode: 'full' });
    }, [onboarding, frameworkTour]);

    const handleFrameworkTourClose = useCallback(
        (reason: 'skip' | 'done') => {
            const { shouldOfferContinue } = frameworkTour.close(reason);
            if (shouldOfferContinue) {
                setContinueOpen(true);
            }
        },
        [frameworkTour],
    );

    const handleContinueToBots = useCallback(async () => {
        setContinueOpen(false);
        await frameworkTour.start({ mode: 'bots' });
    }, [frameworkTour]);

    const handleContinueFinish = useCallback(() => {
        setContinueOpen(false);
        navigate('components');
    }, [navigate]);

    useEffect(() => {
        // 设置「重新查看入门」= 从 Dialog 再走一遍，走完仍接遮罩
        registerOnboardingHost(() => onboarding.openFromSettings());
        return () => registerOnboardingHost(null);
        // eslint-disable-next-line react-hooks/exhaustive-deps -- host once per mount identity
    }, [onboarding.openFromSettings]);

    useEffect(() => {
        // Bot 列表 / 批量启动等复用 App 级协议门禁，禁止页面内再挂一份 gate
        registerDesktopConsentHost((action) => desktopConsent.ensureConsent(action));
        return () => registerDesktopConsentHost(null);
    }, [desktopConsent.ensureConsent]);

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
                                'relative z-10 flex min-w-0 flex-1 overflow-hidden ' +
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
                                        onExited={handlePageExited}
                                        direction={direction}
                                        className="flex min-h-0 min-w-0 flex-1 flex-col"
                                    >
                                        <RouteContent
                                            route={displayedRoute}
                                            onNavigate={navigate}
                                            hostLabels={hostLabels}
                                            showDocker={showDocker}
                                        />
                                    </PageTransition>
                                </div>
                            </div>
                        </main>

                        {features.terminal && (
                            <Suspense fallback={null}>
                                <TerminalDock />
                            </Suspense>
                        )}
                    </div>
                </div>

                <InfoBarStack items={bars} onDismiss={dismiss} onAutoDismiss={remove} />
                <DesktopExitGate />

                <DesktopConsentDialog
                    open={desktopConsent.open}
                    mode={desktopConsent.mode}
                    payload={desktopConsent.payload}
                    submitting={desktopConsent.submitting}
                    onAccept={() => void desktopConsent.accept()}
                    onClose={desktopConsent.close}
                />

                <OnboardingDialog
                    open={onboarding.open}
                    mode={onboarding.mode}
                    submitting={onboarding.submitting || frameworkTour.open}
                    onExplore={() => void onboarding.chooseExplore()}
                    onSkip={() => void onboarding.chooseSkip()}
                    onCloseGuide={() => void onboarding.closeGuide()}
                    onGoComponents={() => void continueOnboardingFlow()}
                    onGoBots={() => void continueOnboardingFlow()}
                    onFinish={() => void continueOnboardingFlow()}
                    onDismissToApp={() => {
                        void (async () => {
                            await onboarding.finishGuide([...ONBOARDING_GUIDE_STEP_IDS]);
                            navigate('overview');
                        })();
                    }}
                />

                <SpotlightTour
                    open={frameworkTour.open}
                    steps={frameworkTour.spotlightSteps}
                    stepIndex={frameworkTour.stepIndex}
                    onStepIndexChange={frameworkTour.setStepIndex}
                    onClose={handleFrameworkTourClose}
                    onBeforeStep={frameworkTour.onBeforeStep}
                />

                <OnboardingContinueDialog
                    open={continueOpen}
                    submitting={frameworkTour.open}
                    onContinueBots={() => void handleContinueToBots()}
                    onFinish={handleContinueFinish}
                />

                <GlobalTitleTooltip />
            </div>
        </TooltipProvider>
    );
};

/**
 * 页面本体。memo 住，props 只放稳定值：根组件因提示条、主机状态、引导等重渲时，
 * 当前页不跟着从头渲一遍。
 */
const RouteContent = memo(function RouteContent({
    route,
    onNavigate,
    hostLabels,
    showDocker,
}: {
    route: AppRoute;
    onNavigate: (route: AppRoute) => void;
    hostLabels: Record<string, string>;
    showDocker: boolean;
}) {
    let body: React.ReactNode;
    switch (route) {
        case 'overview':
            body = <BootstrapPanelNext onNavigate={onNavigate} />;
            break;
        case 'bots':
            body = <BotPageNext onNavigate={onNavigate} />;
            break;
        case 'apps':
            body = <AppsPageNext onNavigate={onNavigate} />;
            break;
        case 'debug':
            body = <DebugConsolePage onNavigate={onNavigate} />;
            break;
        case 'chat':
            body = <ChatPage onNavigate={onNavigate} />;
            break;
        case 'components':
            body = <ComponentsPageNext />;
            break;
        case 'docker':
            body = <DockerPageNext />;
            break;
        case 'remote':
            body = <RemoteHostPanelNext />;
            break;
        case 'tasks':
            body = (
                <TasksRoute
                    hostLabels={hostLabels}
                    onNavigate={onNavigate}
                    showDocker={showDocker}
                />
            );
            break;
        case 'settings':
            body = <SettingsPageNext />;
            break;
        default: {
            const _exhaustive: never = route;
            void _exhaustive;
            body = null;
        }
    }

    // overview 同步；其余 lazy 包一层 Suspense，避免切页白屏。
    if (route === 'overview') {
        return body;
    }
    return <Suspense fallback={<RouteFallback />}>{body}</Suspense>;
});

/** 只有任务页要整份队列：订阅放在这里，进度事件只让任务页重渲。 */
function TasksRoute({
    hostLabels,
    onNavigate,
    showDocker,
}: {
    hostLabels: Record<string, string>;
    onNavigate: (route: AppRoute) => void;
    showDocker: boolean;
}) {
    const taskQueue = useTaskQueue({ hostLabels });
    return (
        <TaskQueuePageNext
            items={taskQueue.items}
            activeCount={taskQueue.activeCount}
            onNavigate={onNavigate}
            showDocker={showDocker}
        />
    );
}

export default AppNext;
