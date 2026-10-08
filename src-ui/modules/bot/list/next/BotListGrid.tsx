// Bot 列表网格：卡片渲染 + 拖拽交互装配。拖拽 / FLIP / ghost 的交互逻辑在
// useBotCardDrag，本文件只保留渲染结构与 alert 上报、进场 stagger。
// retrySnowlumaUi 的隧道重建需要直连 botService，modules 层禁 services，
// 回调由页面（白名单文件）注入。

import { useMemo } from 'react';
import { GripVertical } from 'lucide-react';
import { useGSAP } from '@gsap/react';
import { animateListChildrenEnterAfterPaint } from '../../../../shared/ui/motion/listEnter';
import { ListItem } from '../../../../shared/ui/motion';
import { cn } from '../../../../shared/utils/cn';
import { useMotion } from '../../../../hooks/preferences/useMotion';
import { pushErrorBar } from '../../../../hooks/ui/pushErrorBar';
import { errorText } from '../../../../core/domain/errors';
import { useBotSnapshotAlerts } from '../../../../hooks/bot/useBotSnapshotAlerts';
import { isSnowLumaFlavor } from '../../../../core/domain/bot/flavor';
import {
    snowlumaDaemonStateForConfig,
    isSnowlumaRemoteDockerConfig,
    isSnowlumaRemoteNativeConfig,
} from '../../../../core/domain/bot/snowluma-remote-ui';
import type { useBotSnapshots } from '../../../../hooks/bot/useBotSnapshots';
import type { useBotFlavorMap } from '../../../../hooks/bot/useBotFlavorMap';
import type { useBotConfigsMap } from '../../../../hooks/bot/useBotConfigsMap';
import type { useNapcatLogin } from '../../../../hooks/webui/useNapcatLogin';
import type { useSnowlumaState } from '../../../../hooks/webui/useSnowlumaState';
import type { useBotBatchSelection } from '../../../../hooks/bot/useBotBatchSelection';
import type { useBotMutations } from '../../../../hooks/bot/useBotMutations';
import type { useOpenWebui } from '../../../../hooks/webui/useOpenWebui';
import type { useOpenSnowlumaNovnc } from '../../../../hooks/webui/useOpenSnowlumaNovnc';
import { BotCard } from './BotCard';
import { useBotCardDrag } from './useBotCardDrag';
import gridStyles from './botCardGrid.module.css';

/// Bot 列表带 stagger + 进退场动画。从主组件抽出来,避免在 render 中写一大段
/// motion 逻辑。stagger 由档位 preset 提供;优雅档 stagger=0 退化为同步进场。
type GridProps = {
    bots: ReturnType<typeof useBotSnapshots>['data'] extends infer T
        ? T extends readonly (infer U)[]
            ? U[]
            : never
        : never;
    flavorByBot: ReturnType<typeof useBotFlavorMap>;
    configByBot: ReturnType<typeof useBotConfigsMap>;
    napcat: ReturnType<typeof useNapcatLogin>;
    snowluma: ReturnType<typeof useSnowlumaState>;
    batch: ReturnType<typeof useBotBatchSelection>;
    mutations: ReturnType<typeof useBotMutations>;
    openWebui: ReturnType<typeof useOpenWebui>;
    openSnowlumaNovnc: ReturnType<typeof useOpenSnowlumaNovnc>;
    onConfigureBot: (botId: string | null) => void;
    onViewLogs: (botId: string) => void;
    onViewMetrics: (botId: string) => void;
    onStartBot: (botId: string) => void;
    onReorderBots: (sourceBotId: string, targetBotId: string) => void;
    onRetrySnowlumaUi: (botId: string) => Promise<void>;
    startingBotId: string | null;
};

export function BotListGrid({
    bots,
    flavorByBot,
    configByBot,
    napcat,
    snowluma,
    batch,
    mutations,
    openWebui,
    openSnowlumaNovnc,
    onConfigureBot,
    onViewLogs,
    onViewMetrics,
    onStartBot,
    onReorderBots,
    onRetrySnowlumaUi,
    startingBotId,
}: GridProps) {
    const m = useMotion();
    const { containerRef, ghostRef, draggedId, displayBots, handlePointerDown } = useBotCardDrag({
        bots,
        isBatchMode: batch.isBatchMode,
        onReorderBots,
    });

    const alertRows = useMemo(
        () =>
            bots.map((bot) => {
                const config = configByBot[bot.bot_id] ?? null;
                const flavor = flavorByBot[bot.bot_id] ?? null;
                const name = config?.bot.name?.trim();
                return {
                    bot,
                    displayName: name && name.length > 0 ? name : bot.bot_id,
                    invalidationReason: napcat.byBot[bot.bot_id]?.invalidationReason ?? null,
                    isSnowLuma: isSnowLumaFlavor(flavor),
                    snowlumaDaemonState: snowlumaDaemonStateForConfig(
                        config,
                        snowluma.daemonStates,
                    ),
                    offlineAutoRestart: !!config?.bot.offlineAutoRestart,
                };
            }),
        [bots, configByBot, flavorByBot, napcat.byBot, snowluma.daemonStates],
    );
    useBotSnapshotAlerts(alertRows);

    // 列表 stagger: 只对初次加载播放动画
    useGSAP(
        () => {
            const root = containerRef.current;
            if (!root || !m.enabled) return;
            return animateListChildrenEnterAfterPaint(root, bots.length, m);
        },
        {
            scope: containerRef,
            dependencies: [bots.length, m.enabled, m.level, m.speed, m.stagger],
        },
    );

    const draggedConfig = draggedId ? configByBot[draggedId] : null;
    const draggedDisplayName =
        draggedConfig?.bot.name?.trim() || (draggedId ? `Bot ${draggedId}` : '');

    return (
        <>
            <div ref={containerRef} className={gridStyles.botCardGrid}>
                {displayBots.map((bot) => {
                    const flavor = flavorByBot[bot.bot_id] ?? null;
                    const config = configByBot[bot.bot_id] ?? null;
                    const napcatBot = napcat.byBot[bot.bot_id];
                    const snowlumaBot = snowluma.byBot[bot.bot_id];
                    const snowlumaDaemonState = snowlumaDaemonStateForConfig(
                        config,
                        snowluma.daemonStates,
                    );
                    const isSnowlumaRemoteTunnelUi =
                        isSnowLumaFlavor(flavor) &&
                        (isSnowlumaRemoteDockerConfig(config) ||
                            isSnowlumaRemoteNativeConfig(config));

                    // 当前拖拽项在网格中呈现高质感的虚线落位槽
                    if (draggedId === bot.bot_id) {
                        return (
                            <ListItem key={bot.bot_id}>
                                <div
                                    data-bot-card-id={bot.bot_id}
                                    className="flex h-[148px] min-h-[148px] w-full items-center justify-center rounded-xl border-2 border-dashed border-brand/50 bg-brand-soft/20 text-xs font-medium text-brand select-none"
                                >
                                    <span className="flex items-center gap-1.5 animate-pulse font-medium">
                                        <GripVertical size={15} /> 放置于此处
                                    </span>
                                </div>
                            </ListItem>
                        );
                    }

                    return (
                        <ListItem key={bot.bot_id} hoverable={!draggedId}>
                            <div
                                data-bot-card-id={bot.bot_id}
                                onPointerDown={(e) => handlePointerDown(e, bot.bot_id)}
                                className={cn(
                                    'h-[148px] min-h-[148px] select-none rounded-xl',
                                    !batch.isBatchMode && 'cursor-grab active:cursor-grabbing',
                                )}
                            >
                                <BotCard
                                    bot={bot}
                                    config={config}
                                    flavor={flavor}
                                    qrcodeUrl={napcatBot?.qrcodeUrl ?? null}
                                    isOnline={napcatBot?.online ?? null}
                                    invalidationReason={napcatBot?.invalidationReason ?? null}
                                    napcatBinding={napcatBot?.webui ?? null}
                                    snowlumaDaemonState={snowlumaDaemonState}
                                    snowlumaDockerEndpointsReady={
                                        snowlumaBot?.dockerEndpointsReady ?? false
                                    }
                                    snowlumaLoginState={snowlumaBot?.loginState ?? null}
                                    snowlumaProbeUnavailable={
                                        snowlumaBot?.probeUnavailable ?? false
                                    }
                                    isBatchMode={batch.isBatchMode}
                                    isSelected={batch.selectedIds.has(bot.bot_id)}
                                    actionPending={startingBotId === bot.bot_id}
                                    onStart={onStartBot}
                                    onStop={mutations.stopBot}
                                    onConfigure={onConfigureBot}
                                    onViewLogs={onViewLogs}
                                    onViewMetrics={onViewMetrics}
                                    onToggleSelect={batch.toggleSelect}
                                    onOpenWebui={(params) => {
                                        openWebui(params).catch((err: unknown) => {
                                            pushErrorBar({
                                                key: `webui-open:${params.botId}`,
                                                title: '打开 WebUI 失败',
                                                raw: errorText(err),
                                            });
                                        });
                                    }}
                                    isSnowlumaRemoteTunnelUi={isSnowlumaRemoteTunnelUi}
                                    onOpenNovnc={(id) => {
                                        openSnowlumaNovnc(id).catch((err: unknown) => {
                                            pushErrorBar({
                                                key: `novnc-open:${id}`,
                                                title: '打开 noVNC 失败',
                                                raw: errorText(err),
                                            });
                                        });
                                    }}
                                    onRetrySnowlumaUi={onRetrySnowlumaUi}
                                />
                            </div>
                        </ListItem>
                    );
                })}
            </div>

            {/* 拖拽时的全局跟随浮动卡片（脱离 React 渲染树，纯 GPU 120Hz 变换） */}
            <div
                ref={ghostRef}
                style={{ display: 'none' }}
                className={cn(
                    'pointer-events-none fixed top-0 left-0 z-[9999] will-change-transform flex items-center gap-2.5 rounded-xl border border-brand/60 bg-surface/95 px-4 py-3 backdrop-blur-md select-none text-text',
                    !m.enabled
                        ? 'shadow-none ring-0'
                        : m.level === 'rich'
                          ? 'shadow-[0_24px_48px_-12px_rgba(0,0,0,0.45)] ring-2 ring-brand/60'
                          : 'shadow-2xl ring-1 ring-border-subtle',
                )}
            >
                <GripVertical size={16} className="text-brand shrink-0" />
                <div className="flex flex-col min-w-0">
                    <span className="text-xs font-semibold text-text truncate max-w-[160px]">
                        {draggedDisplayName}
                    </span>
                    <span className="font-mono text-2xs text-text-tertiary">QQ {draggedId}</span>
                </div>
            </div>
        </>
    );
}
