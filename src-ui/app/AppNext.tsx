// 新 UI 树根 = AppShell。
// 布局:TitleBar(透明) ─ [Sidebar | main]

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';

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
import { useBootstrap } from '../hooks/bootstrap/useBootstrap';
import { useDataLayoutConsolidateAlert } from '../hooks/bootstrap/useDataLayoutConsolidateAlert';
import { useDesktopConsentGate } from '../hooks/desktop/useDesktopConsentGate';
import { useOnboardingGate } from '../hooks/desktop/useOnboardingGate';
import { registerOnboardingHost } from '../hooks/desktop/onboardingHost';
import { useFrameworkTour } from '../hooks/desktop/useFrameworkTour';
import { DesktopConsentDialog } from '../shared/components/next/DesktopConsentDialog';
import {
    ONBOARDING_GUIDE_STEP_IDS,
    OnboardingDialog,
} from '../shared/components/next/OnboardingDialog';
import { OnboardingContinueDialog } from '../shared/components/next/OnboardingContinueDialog';
import { SpotlightTour } from '../shared/components/next/SpotlightTour';

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

    const { bootstrap } = useBootstrap();
    useDataLayoutConsolidateAlert(bootstrap);

    const desktopConsent = useDesktopConsentGate();
    const onboarding = useOnboardingGate();
    const consentWasBlockingRef = useRef(false);

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

    useEffect(() => {
        void (async () => {
            const ok = await desktopConsent.promptAtStartup();
            // 已同意：直接尝试入门选择；未同意则等 accept 后由 blocking 回落触发
            if (ok) await onboarding.promptAfterConsent();
        })();
        // 仅挂载时检查一次
        // eslint-disable-next-line react-hooks/exhaustive-deps -- startup once
    }, []);

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
                            await onboarding.finishGuide([
                                ...ONBOARDING_GUIDE_STEP_IDS,
                            ]);
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
