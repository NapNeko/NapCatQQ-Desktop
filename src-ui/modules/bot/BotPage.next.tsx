// Bot 业务区路由壳：list / config / log / metrics 四个视图之间的浅路由切换。
//
// 回到 list 时在过渡结束后再清 selectedBotId；退场动画期间仍要带着 botId，
// 否则配置页会瞬间变成「新建 Bot」标题。
//
// botTourBridge：入门引导可强制打开演示新建（不落盘）。

import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { Button } from '../../shared/ui';
import { PageTransition } from '../../shared/ui/motion';
import { RouteErrorBoundary } from '../../shared/ui/RouteErrorBoundary';
import {
    getBotTourBridge,
    setBotTourBridge,
    subscribeBotTourBridge,
} from '../../hooks/desktop/botTourBridge';
import { requestDesktopConsent } from '../../hooks/desktop/desktopConsentHost';
import {
    consumePendingBotCreateRequest,
    createDockerBotConfig,
    dockerBotRuntimeTarget,
    getPendingBotCreateRequest,
    usePendingBotCreateRequest,
} from '../../hooks/bot/botCreateNavigation';
import { useServerManager } from '../../hooks/remote/useServerManager';
import { featureTogglesStore } from '../../hooks/preferences/featureTogglesStore';
import { pushInfoBar } from '../../hooks/ui/globalInfoBarStore';
import { pushErrorBar } from '../../hooks/ui/pushErrorBar';
import { errorText } from '../../core/domain/errors';
import type { BotConfig } from '../../core/ipc/generated/domain/BotConfig';
import { BotListPageNext } from './list/BotListPage.next';
import { BotConfigPageNext } from './config/BotConfigPage.next';
import { BotLogPageNext } from './log/BotLogPage.next';
import { BotRuntimeMetricsPageNext } from './metrics/BotRuntimeMetricsPage.next';
import type { AppRoute } from '../../shared/components/next/Sidebar';

type View = 'list' | 'config' | 'log' | 'metrics';

const VIEW_ORDER: ReadonlyArray<View> = ['list', 'config', 'log', 'metrics'];

export function BotPageNext({ onNavigate }: { onNavigate?: (route: AppRoute) => void } = {}) {
    const [view, setView] = useState<View>('list');
    const [selectedBotId, setSelectedBotId] = useState<string | null>(null);
    const [initialConfig, setInitialConfig] = useState<BotConfig | null>(null);
    const pendingCreate = usePendingBotCreateRequest();
    const { servers, isLoading: serversLoading } = useServerManager();
    const latestServers = useRef(servers);
    latestServers.current = servers;
    const handledCreate = useRef<number | null>(null);
    const mounted = useRef(false);

    const [displayedView, setDisplayedView] = useState<View>('list');
    const [subVisible, setSubVisible] = useState(true);
    const [subDirection, setSubDirection] = useState<-1 | 0 | 1>(0);

    const botTour = useSyncExternalStore(
        subscribeBotTourBridge,
        getBotTourBridge,
        getBotTourBridge,
    );

    useEffect(() => {
        mounted.current = true;
        return () => {
            mounted.current = false;
        };
    }, []);

    useEffect(() => {
        if (!pendingCreate || serversLoading || botTour.demoMode) return;
        if (handledCreate.current === pendingCreate.requestId) return;
        handledCreate.current = pendingCreate.requestId;

        const rejectUnavailable = (): boolean => {
            const serverId = dockerBotRuntimeTarget(pendingCreate.hostId);
            const framework = pendingCreate.flavor === 'napcat' ? 'NapCat' : 'SnowLuma';
            const reason = !serverId
                ? 'Docker Bot 只能部署到具体的远程主机。'
                : !latestServers.current.some((server) => server.id === serverId)
                  ? '目标远程主机已不存在，请重新选择主机。'
                  : !featureTogglesStore.getSnapshot()[pendingCreate.flavor]
                    ? `请先在设置中启用 ${framework}。`
                    : null;
            if (!reason) return false;
            if (consumePendingBotCreateRequest(pendingCreate.requestId)) {
                pushInfoBar({ tone: 'warning', title: '无法新建 Docker Bot', content: reason });
            }
            return true;
        };
        if (rejectUnavailable()) return;

        void requestDesktopConsent(() => {
            if (!mounted.current) return;
            if (getPendingBotCreateRequest()?.requestId !== pendingCreate.requestId) return;
            if (getBotTourBridge().demoMode || rejectUnavailable()) return;
            const config = createDockerBotConfig(pendingCreate);
            if (!consumePendingBotCreateRequest(pendingCreate.requestId)) return;
            setInitialConfig(config);
            setSelectedBotId(null);
            setView('config');
        }).catch((error: unknown) => {
            if (!mounted.current) return;
            if (!consumePendingBotCreateRequest(pendingCreate.requestId)) return;
            pushErrorBar({ title: '无法新建 Docker Bot', raw: errorText(error) });
        });
    }, [pendingCreate, serversLoading, botTour.demoMode]);

    // 入门引导：打开演示新建 / 回列表。
    // 同步切 displayedView，跳过 PageTransition 空窗，否则遮罩量不到配置页锚点。
    useEffect(() => {
        if (botTour.openCreate) {
            setInitialConfig(null);
            setSelectedBotId(null);
            setView('config');
            setDisplayedView('config');
            setSubVisible(true);
            setBotTourBridge({ openCreate: false });
        }
    }, [botTour.openCreate]);

    useEffect(() => {
        if (botTour.requestList) {
            setInitialConfig(null);
            setView('list');
            setDisplayedView('list');
            setSubVisible(true);
            setSelectedBotId(null);
            setBotTourBridge({ requestList: false });
        }
    }, [botTour.requestList]);

    useEffect(() => {
        if (view === displayedView) {
            if (!subVisible) setSubVisible(true);
            return;
        }
        const oldIdx = VIEW_ORDER.indexOf(displayedView);
        const newIdx = VIEW_ORDER.indexOf(view);
        const dir: -1 | 0 | 1 =
            oldIdx < 0 || newIdx < 0 ? 0 : newIdx > oldIdx ? 1 : newIdx < oldIdx ? -1 : 0;
        setSubDirection(dir);
        setSubVisible(false);
    }, [view, displayedView, subVisible]);

    const handleSubExited = () => {
        setDisplayedView(view);
        if (view === 'list') {
            setSelectedBotId(null);
            setInitialConfig(null);
        }
        setSubVisible(true);
    };

    const goList = () => {
        const pending = getPendingBotCreateRequest();
        if (pending) consumePendingBotCreateRequest(pending.requestId);
        setView('list');
    };

    return (
        <div className="flex h-full w-full flex-col">
            <PageTransition
                visible={subVisible}
                onExited={handleSubExited}
                direction={subDirection}
                className="flex min-h-0 flex-1 flex-col"
            >
                <BotViewContent
                    view={displayedView}
                    selectedBotId={selectedBotId}
                    initialConfig={initialConfig}
                    demoMode={
                        botTour.demoMode &&
                        (view === 'config' || displayedView === 'config') &&
                        selectedBotId === null
                    }
                    forceTab={botTour.forceTab}
                    onConfigureBot={(botId) => {
                        const pending = getPendingBotCreateRequest();
                        if (pending) consumePendingBotCreateRequest(pending.requestId);
                        setInitialConfig(null);
                        setSelectedBotId(botId);
                        setView('config');
                    }}
                    onViewLogs={(botId) => {
                        setSelectedBotId(botId);
                        setView('log');
                    }}
                    onViewMetrics={(botId) => {
                        setSelectedBotId(botId);
                        setView('metrics');
                    }}
                    onBack={goList}
                    onSavedStay={(savedBotId) => {
                        setSelectedBotId(savedBotId);
                        setInitialConfig(null);
                    }}
                    onNavigate={onNavigate}
                />
            </PageTransition>
        </div>
    );
}

function BotViewContent({
    view,
    selectedBotId,
    initialConfig,
    demoMode,
    forceTab,
    onConfigureBot,
    onViewLogs,
    onViewMetrics,
    onBack,
    onSavedStay,
    onNavigate,
}: {
    view: View;
    selectedBotId: string | null;
    initialConfig: BotConfig | null;
    demoMode: boolean;
    forceTab: 'identity' | 'connections' | 'advanced' | null;
    onConfigureBot: (botId: string | null) => void;
    onViewLogs: (botId: string) => void;
    onViewMetrics: (botId: string) => void;
    onBack: () => void;
    onSavedStay: (savedBotId: string) => void;
    onNavigate?: (route: AppRoute) => void;
}) {
    switch (view) {
        case 'list':
            return (
                <RouteErrorBoundary title="Bot 列表加载失败">
                    <BotListPageNext
                        onConfigureBot={onConfigureBot}
                        onViewLogs={onViewLogs}
                        onViewMetrics={onViewMetrics}
                        onNavigate={onNavigate}
                    />
                </RouteErrorBoundary>
            );
        case 'config':
            return (
                <BotConfigPageNext
                    botId={selectedBotId}
                    onBack={onBack}
                    onSavedStay={onSavedStay}
                    tourDemoMode={demoMode}
                    tourForceTab={forceTab}
                    initialConfig={initialConfig}
                />
            );
        case 'log':
            if (selectedBotId) {
                return <BotLogPageNext botId={selectedBotId} onBack={onBack} />;
            }
            return (
                <div className="flex flex-col items-center gap-3 rounded-md bg-elevated p-6 ring-1 ring-border-subtle">
                    <p className="text-sm text-text-secondary">未选择要查看日志的 Bot 实例</p>
                    <Button size="sm" variant="ghost" onClick={onBack}>
                        返回实例列表
                    </Button>
                </div>
            );
        case 'metrics':
            if (selectedBotId) {
                return (
                    <RouteErrorBoundary title="运行时指标加载失败">
                        <BotRuntimeMetricsPageNext botId={selectedBotId} onBack={onBack} />
                    </RouteErrorBoundary>
                );
            }
            return (
                <div className="flex flex-col items-center gap-3 rounded-md bg-elevated p-6 ring-1 ring-border-subtle">
                    <p className="text-sm text-text-secondary">未选择要查看指标的 Bot 实例</p>
                    <Button size="sm" variant="ghost" onClick={onBack}>
                        返回实例列表
                    </Button>
                </div>
            );
        default: {
            const _exhaustive: never = view;
            void _exhaustive;
            return null;
        }
    }
}

export default BotPageNext;
