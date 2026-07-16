// Bot 业务区路由壳（next）。
//
// 跟旧 BotPage.tsx 对应：3 视图 (list / config / log) 的浅路由切换。
// Step 7 完成 list；Step 8 (本次) 完成 config 推倒重写。log 仍走旧 Fluent 树等 Step 9。
//
// 回到 list 时在过渡结束后再清 selectedBotId；退场动画期间仍要带着 botId，
// 否则配置页会瞬间变成「新建 Bot」标题。
//
// botTourBridge：入门引导可强制打开演示新建（不落盘）。

import { useEffect, useState, useSyncExternalStore } from 'react';
import { Button } from '../../shared/ui';
import { PageTransition } from '../../shared/ui/motion';
import { RouteErrorBoundary } from '../../shared/ui/RouteErrorBoundary';
import {
    getBotTourBridge,
    setBotTourBridge,
    subscribeBotTourBridge,
} from '../../hooks/desktop/botTourBridge';
import { BotListPageNext } from './list/BotListPage.next';
import { BotConfigPageNext } from './config/BotConfigPage.next';
import { BotLogPageNext } from './log/BotLogPage.next';
import { BotRuntimeMetricsPageNext } from './metrics/BotRuntimeMetricsPage.next';

type View = 'list' | 'config' | 'log' | 'metrics';

const VIEW_ORDER: ReadonlyArray<View> = ['list', 'config', 'log', 'metrics'];

export function BotPageNext() {
    const [view, setView] = useState<View>('list');
    const [selectedBotId, setSelectedBotId] = useState<string | null>(null);

    const [displayedView, setDisplayedView] = useState<View>('list');
    const [subVisible, setSubVisible] = useState(true);
    const [subDirection, setSubDirection] = useState<-1 | 0 | 1>(0);

    const botTour = useSyncExternalStore(
        subscribeBotTourBridge,
        getBotTourBridge,
        getBotTourBridge,
    );

    // 入门引导：打开演示新建 / 回列表。
    // 同步切 displayedView，跳过 PageTransition 空窗，否则遮罩量不到配置页锚点。
    useEffect(() => {
        if (botTour.openCreate) {
            setSelectedBotId(null);
            setView('config');
            setDisplayedView('config');
            setSubVisible(true);
            setBotTourBridge({ openCreate: false });
        }
    }, [botTour.openCreate]);

    useEffect(() => {
        if (botTour.requestList) {
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
        }
        setSubVisible(true);
    };

    const goList = () => {
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
                    demoMode={
                        botTour.demoMode &&
                        (view === 'config' || displayedView === 'config') &&
                        selectedBotId === null
                    }
                    forceTab={botTour.forceTab}
                    onConfigureBot={(botId) => {
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
                    onSavedStay={(savedBotId) => setSelectedBotId(savedBotId)}
                />
            </PageTransition>
        </div>
    );
}

function BotViewContent({
    view,
    selectedBotId,
    demoMode,
    forceTab,
    onConfigureBot,
    onViewLogs,
    onViewMetrics,
    onBack,
    onSavedStay,
}: {
    view: View;
    selectedBotId: string | null;
    demoMode: boolean;
    forceTab: 'identity' | 'connections' | 'advanced' | null;
    onConfigureBot: (botId: string | null) => void;
    onViewLogs: (botId: string) => void;
    onViewMetrics: (botId: string) => void;
    onBack: () => void;
    onSavedStay: (savedBotId: string) => void;
}) {
    switch (view) {
        case 'list':
            return (
                <RouteErrorBoundary title="Bot 列表加载失败">
                    <BotListPageNext
                        onConfigureBot={onConfigureBot}
                        onViewLogs={onViewLogs}
                        onViewMetrics={onViewMetrics}
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
