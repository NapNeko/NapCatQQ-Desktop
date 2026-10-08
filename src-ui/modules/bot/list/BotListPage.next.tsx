import { useEffect, useState, useCallback } from 'react';
import { Counter } from '../../../shared/ui/motion';
import { useBotSnapshots } from '../../../hooks/bot/useBotSnapshots';
import { useSortedBots } from '../../../hooks/bot/useBotSort';
import { useSyncRemoteRuntimes } from '../../../hooks/bot/useSyncRemoteRuntimes';
import { useBotMutations, type ActionMessage } from '../../../hooks/bot/useBotMutations';
import { useBotStartFlow } from '../../../hooks/bot/useBotStartFlow';
import { useBotBatchSelection } from '../../../hooks/bot/useBotBatchSelection';
import { useBotFlavorMap } from '../../../hooks/bot/useBotFlavorMap';
import { useBotConfigsMap } from '../../../hooks/bot/useBotConfigsMap';
import { useBotDockerStartGate } from '../../../hooks/bot/useBotDockerStartGate';
import { useBotRuntimeStartGate } from '../../../hooks/bot/useBotRuntimeStartGate';
import { useNapcatLogin } from '../../../hooks/webui/useNapcatLogin';
import { useSnowlumaState } from '../../../hooks/webui/useSnowlumaState';
import { useOpenWebui } from '../../../hooks/webui/useOpenWebui';
import { useOpenSnowlumaNovnc } from '../../../hooks/webui/useOpenSnowlumaNovnc';
import { pushInfoBar } from '../../../hooks/ui/globalInfoBarStore';
import { pushErrorBar } from '../../../hooks/ui/pushErrorBar';
import { errorText } from '../../../core/domain/errors';
import { isSnowLumaFlavor } from '../../../core/domain/bot/flavor';
import { botService } from '../../../core/services/bot.service';
import { FloatingActions } from './next/FloatingActions';
import { BatchBottomBar } from './next/BatchBottomBar';
import { BotListGrid } from './next/BotListGrid';
import { LoadingState, ErrorState, EmptyState } from './next/botListStateParts';
import { BatchDeleteConfirmDialog } from './next/BatchDeleteConfirmDialog';
import { ConfigDriftDialog } from '../dialogs/ConfigDriftDialog';
import { ImportRemoteBotsDialog } from '../dialogs/ImportRemoteBotsDialog';
import { SnowLumaConsentDialog } from '../dialogs/SnowLumaConsentDialog';
import { requestDesktopConsent } from '../../../hooks/desktop/desktopConsentHost';
import { SystemQqWarningDialog } from '../dialogs/SystemQqWarningDialog';
import type { AppRoute } from '../../../shared/components/next/Sidebar';

interface BotListPageNextProps {
    onConfigureBot: (botId: string | null) => void;
    onViewLogs: (botId: string) => void;
    onViewMetrics: (botId: string) => void;
    onNavigate?: (route: AppRoute) => void;
}

export function BotListPageNext({
    onConfigureBot,
    onViewLogs,
    onViewMetrics,
    onNavigate,
}: BotListPageNextProps) {
    const { data: rawBotSnapshots = [], isLoading, error, refetch } = useBotSnapshots();
    const flavorByBot = useBotFlavorMap(rawBotSnapshots);
    const configByBot = useBotConfigsMap(rawBotSnapshots);
    const { sortedBots: botSnapshots, reorderBots } = useSortedBots(rawBotSnapshots);
    useSyncRemoteRuntimes(rawBotSnapshots, configByBot);
    const { startBlock: dockerStartGate } = useBotDockerStartGate(configByBot);
    const { startBlock: runtimeStartGate } = useBotRuntimeStartGate(configByBot);
    const napcat = useNapcatLogin();
    const snowluma = useSnowlumaState();
    const batch = useBotBatchSelection();
    const openWebui = useOpenWebui();
    const openSnowlumaNovnc = useOpenSnowlumaNovnc();
    // Desktop 协议门禁统一走 App 级 host，避免本页再挂一份 gate/Dialog 打架
    // 批量删除二次确认
    const [confirmDeleteOpen, setConfirmDeleteOpen] = useState(false);
    const [importOpen, setImportOpen] = useState(false);
    const [batchStartPreparing, setBatchStartPreparing] = useState(false);

    // 把 mutation 的 success / error 消息桥接到全局 InfoBar 队列。
    const handleMessage = (msg: ActionMessage) => {
        if (msg.type === 'success') {
            pushInfoBar({
                tone: 'success',
                title: '操作完成',
                content: msg.text,
            });
        } else {
            pushErrorBar({ title: '操作失败', raw: msg.text });
        }
        if (msg.text.startsWith('批量')) batch.exitBatch();
    };

    const mutations = useBotMutations({ onMessage: handleMessage });

    const {
        pendingDrift,
        consentBotId,
        consentPayload,
        consentSubmitting,
        startingBotId,
        systemQqBotId,
        setSystemQqBotId,
        handleStartBot,
        handleSystemQqContinue,
        handleSystemQqInstall,
        handleDriftConfirm,
        handleDriftCancel,
        handleConsentConfirm,
        handleConsentCancel,
        clearConsentErrorSuppression,
        prepareSnowLumaConsentOrOpen,
    } = useBotStartFlow({
        botSnapshots,
        configByBot,
        dockerStartGate,
        runtimeStartGate,
        mutations,
        onNavigate,
    });

    // 加载 / 错误状态也接 InfoBar，让顶部状态信息统一。但只在错误首次出现时
    // 推一次，避免 react-query 重试反复推。
    useEffect(() => {
        if (!error) return;
        pushErrorBar({
            key: 'bot-list-fetch-error',
            title: 'Bot 列表加载失败',
            raw: error.message,
        });
    }, [error]);

    const onBatchStart = () => {
        if (batch.selectedIds.size === 0) return;
        const ids = Array.from(batch.selectedIds);
        void requestDesktopConsent(() => {
            setBatchStartPreparing(true);
            void (async () => {
                for (const botId of ids) {
                    clearConsentErrorSuppression(botId);
                    const config = configByBot[botId] ?? null;
                    const flavor = flavorByBot[botId] ?? config?.bot.backend_type ?? null;
                    if (!isSnowLumaFlavor(flavor)) continue;
                    const ready = await prepareSnowLumaConsentOrOpen(botId);
                    if (!ready) return;
                }
                mutations.batchStart(ids);
            })().finally(() => setBatchStartPreparing(false));
        });
    };

    const onCreateBot = useCallback(() => {
        void requestDesktopConsent(() => {
            onConfigureBot(null);
        });
    }, [onConfigureBot]);
    const onImportBots = useCallback(() => {
        void requestDesktopConsent(() => {
            setImportOpen(true);
        });
    }, []);
    const onBatchStop = () => {
        if (batch.selectedIds.size === 0) return;
        mutations.batchStop(Array.from(batch.selectedIds));
    };
    const onBatchDeleteConfirm = () => {
        if (batch.selectedIds.size === 0) return;
        mutations.batchDelete(Array.from(batch.selectedIds));
        setConfirmDeleteOpen(false);
    };

    const allSelected = botSnapshots.length > 0 && batch.selectedIds.size === botSnapshots.length;
    const selectAll = () => {
        for (const bot of botSnapshots) {
            if (!batch.selectedIds.has(bot.bot_id)) batch.toggleSelect(bot.bot_id);
        }
    };
    const selectNone = () => {
        for (const id of Array.from(batch.selectedIds)) batch.toggleSelect(id);
    };

    return (
        <div className="flex min-h-0 flex-1 flex-col">
            {/* 头部 */}
            <header
                className="flex shrink-0 items-end justify-between pb-4 pt-2"
                data-tour-id="bot-list-header"
            >
                <div>
                    <p className="text-2xs uppercase tracking-widest text-text-tertiary">bots</p>
                    <h1 className="font-display text-xl font-semibold text-text">Bot 实例</h1>
                    <p className="mt-1 text-sm text-text-secondary">
                        管理本机和远端 Bot 配置、生命周期与登录态。
                    </p>
                </div>
                <div className="flex items-baseline gap-1 text-xs text-text-tertiary tabular-nums">
                    <span>共</span>
                    <Counter
                        value={botSnapshots.length}
                        className="font-medium text-text-secondary"
                    />
                    <span>个实例</span>
                </div>
            </header>

            {/* 主体：单卡按内容自然高度（min-h-24），多张时流式滚动。
                之前试过 grid-rows-4 强制每张 1/4 视口，stopped 卡内容少时被撑得
                稀疏，上下留白严重。退回流式 + 卡内紧凑，视觉密度更稳。
                overflow-y-auto 会隐式触发 overflow-x:auto 把卡片的 ring/shadow
                切掉，所以容器四周加 px-2 py-1 让阴影有呼吸位。 */}
            <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-2 pb-24 pt-1">
                {isLoading ? (
                    <LoadingState />
                ) : error ? (
                    <ErrorState onRetry={() => refetch()} />
                ) : botSnapshots.length === 0 ? (
                    <EmptyState onCreate={onCreateBot} onImport={onImportBots} />
                ) : (
                    <BotListGrid
                        bots={botSnapshots}
                        flavorByBot={flavorByBot}
                        configByBot={configByBot}
                        napcat={napcat}
                        snowluma={snowluma}
                        batch={batch}
                        mutations={mutations}
                        openWebui={openWebui}
                        openSnowlumaNovnc={openSnowlumaNovnc}
                        onConfigureBot={onConfigureBot}
                        onViewLogs={onViewLogs}
                        onViewMetrics={onViewMetrics}
                        onStartBot={handleStartBot}
                        onReorderBots={reorderBots}
                        onRetrySnowlumaUi={async (id) => {
                            try {
                                await botService.retrySnowlumaUi(id);
                                pushInfoBar({
                                    key: `snowluma-ui-retry:${id}`,
                                    tone: 'success',
                                    title: '连接已重建',
                                    content: 'SnowLuma WebUI 与 noVNC 隧道已重新建立。',
                                });
                            } catch (err: unknown) {
                                pushErrorBar({
                                    key: `snowluma-ui-retry:${id}`,
                                    title: '连接重建失败',
                                    raw: errorText(err),
                                });
                            }
                        }}
                        startingBotId={startingBotId}
                    />
                )}
            </div>

            {/* 浮动操作组(互斥):FloatingActions / BatchBottomBar 切换走 GsapPresence,
                各自跑 enter/exit 不打架。 */}
            <FloatingActions
                visible={!batch.isBatchMode}
                busy={mutations.isPending}
                onCreate={onCreateBot}
                onImport={onImportBots}
                onRefresh={() => refetch()}
                onEnterBatch={batch.toggleBatch}
            />
            <BatchBottomBar
                visible={batch.isBatchMode}
                selectedCount={batch.selectedIds.size}
                totalCount={botSnapshots.length}
                allSelected={allSelected}
                onSelectAll={selectAll}
                onSelectNone={selectNone}
                onBatchStart={onBatchStart}
                onBatchStop={onBatchStop}
                onBatchDelete={() => setConfirmDeleteOpen(true)}
                onExitBatch={batch.toggleBatch}
                busy={mutations.isPending || batchStartPreparing}
            />

            {/* 批量删除确认 */}
            <BatchDeleteConfirmDialog
                open={confirmDeleteOpen}
                onOpenChange={setConfirmDeleteOpen}
                selectedCount={batch.selectedIds.size}
                busy={mutations.isPending}
                onConfirm={onBatchDeleteConfirm}
            />

            <ImportRemoteBotsDialog open={importOpen} onOpenChange={setImportOpen} />

            {/* Config drift 确认 */}
            {pendingDrift && (
                <ConfigDriftDialog
                    open={!!pendingDrift}
                    drift={pendingDrift}
                    intent="start"
                    onConfirm={handleDriftConfirm}
                    onCancel={handleDriftCancel}
                />
            )}

            <SystemQqWarningDialog
                open={!!systemQqBotId}
                onContinue={handleSystemQqContinue}
                onInstall={handleSystemQqInstall}
                onCancel={() => setSystemQqBotId(null)}
            />

            <SnowLumaConsentDialog
                open={!!consentBotId}
                botId={consentBotId}
                payload={consentPayload}
                submitting={consentSubmitting}
                onConfirm={handleConsentConfirm}
                onCancel={handleConsentCancel}
            />
        </div>
    );
}
