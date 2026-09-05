import { useEffect, useLayoutEffect, useMemo, useRef, useState, useCallback } from 'react';
import gsap from 'gsap';
import { Bot, GripVertical } from 'lucide-react';
import { useGSAP } from '@gsap/react';
import { animateListChildrenEnterAfterPaint } from '../../../shared/ui/motion/listEnter';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogTitle,
    Spinner,
} from '../../../shared/ui';
import { ListItem, Counter, MotionIcon } from '../../../shared/ui/motion';
import { PagePlaceholder } from '../../../shared/ui/PagePlaceholder';
import { cn } from '../../../shared/utils/cn';
import { useMotion } from '../../../hooks/preferences/useMotion';
import { useBotSnapshots } from '../../../hooks/bot/useBotSnapshots';
import { useSortedBots } from '../../../hooks/bot/useBotSort';
import { useSyncRemoteRuntimes } from '../../../hooks/bot/useSyncRemoteRuntimes';
import { useBotMutations, type ActionMessage } from '../../../hooks/bot/useBotMutations';
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
import { useBotSnapshotAlerts } from '../../../hooks/bot/useBotSnapshotAlerts';
import { isSnowLumaFlavor } from '../../../core/domain/bot/flavor';
import {
    snowlumaDaemonStateForConfig,
    isSnowlumaRemoteDockerConfig,
    isSnowlumaRemoteNativeConfig,
} from '../../../core/domain/bot/snowluma-remote-ui';
import { botService } from '../../../core/services/bot.service';
import type { SnowLumaAgreementsPayload } from '../../../core/services/bot.service';
import type { ConfigDrift } from '../../../core/ipc/generated/ConfigDrift';
import type { DriftDecision } from '../../../core/ipc/generated/DriftDecision';
import { BotCard } from './next/BotCard';
import { FloatingActions } from './next/FloatingActions';
import { BatchBottomBar } from './next/BatchBottomBar';
import { ConfigDriftDialog } from '../dialogs/ConfigDriftDialog';
import { ImportRemoteBotsDialog } from '../dialogs/ImportRemoteBotsDialog';
import { SnowLumaConsentDialog } from '../dialogs/SnowLumaConsentDialog';
import { requestDesktopConsent } from '../../../hooks/desktop/desktopConsentHost';
import gridStyles from './next/botCardGrid.module.css';

interface BotListPageNextProps {
    onConfigureBot: (botId: string | null) => void;
    onViewLogs: (botId: string) => void;
    onViewMetrics: (botId: string) => void;
}

function isSnowLumaConsentError(err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
    return message.includes('SNOWLUMA_CONSENT_REQUIRED') || message.includes('"consentRequired":true');
}

function isDesktopConsentError(err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
    return message.includes('DESKTOP_CONSENT_REQUIRED');
}

export function BotListPageNext({
    onConfigureBot,
    onViewLogs,
    onViewMetrics,
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

    // 把 mutation 的 success / error 消息桥接到全局 InfoBar 队列。
    const handleMessage = (msg: ActionMessage) => {
        pushInfoBar({
            // 不传 key：每条 mutation 反馈都独立显示。批量动作的 partial-success
            // 通常想全部留住看清楚。
            tone: msg.type === 'success' ? 'success' : 'danger',
            title: msg.type === 'success' ? '操作完成' : '操作失败',
            content: msg.text,
            autoDismissMs: msg.type === 'success' ? undefined : 0,
        });
        // 批量动作完成后退出批量模式（对齐旧版交互）。
        if (msg.text.startsWith('批量')) batch.exitBatch();
    };

    const mutations = useBotMutations({ onMessage: handleMessage });

    // ── Config drift detection before start ──────────────────────────────
    const [pendingDrift, setPendingDrift] = useState<ConfigDrift | null>(null);
    const [driftBotId, setDriftBotId] = useState<string | null>(null);
    const [consentBotId, setConsentBotId] = useState<string | null>(null);
    const [consentPayload, setConsentPayload] = useState<SnowLumaAgreementsPayload | null>(null);
    const [consentSubmitting, setConsentSubmitting] = useState(false);
    const [consentRetryDecisions, setConsentRetryDecisions] = useState<DriftDecision[] | null>(null);
    const [startingBotId, setStartingBotId] = useState<string | null>(null);
    const [batchStartPreparing, setBatchStartPreparing] = useState(false);
    const activeConsentBotRef = useRef<string | null>(null);
    const openingConsentBotRef = useRef<string | null>(null);
    const suppressedConsentErrorsRef = useRef<Map<string, string>>(new Map());

    useEffect(() => {
        activeConsentBotRef.current = consentBotId;
    }, [consentBotId]);

    const suppressCurrentConsentError = useCallback((botId: string) => {
        const currentError = botSnapshots
            .find((bot) => bot.bot_id === botId)
            ?.last_error
            ?.trim();
        if (currentError && isSnowLumaConsentError(currentError)) {
            suppressedConsentErrorsRef.current.set(botId, currentError);
        }
    }, [botSnapshots]);

    const clearConsentErrorSuppression = useCallback((botId: string) => {
        suppressedConsentErrorsRef.current.delete(botId);
    }, []);

    const openSnowLumaConsent = useCallback(async (
        botId: string,
        decisions: DriftDecision[] | null = null,
        preparedPayload?: SnowLumaAgreementsPayload,
    ) => {
        const activeBot = activeConsentBotRef.current;
        if (activeBot || openingConsentBotRef.current) return;
        activeConsentBotRef.current = botId;
        openingConsentBotRef.current = botId;
        setConsentBotId(botId);
        setConsentRetryDecisions(decisions);
        if (preparedPayload) {
            setConsentPayload(preparedPayload);
            openingConsentBotRef.current = null;
            return;
        }
        setConsentPayload(null);
        try {
            const payload = await botService.getSnowLumaAgreements(botId);
            setConsentPayload(payload);
        } catch (err: unknown) {
            if (activeConsentBotRef.current === botId) {
                activeConsentBotRef.current = null;
            }
            setConsentBotId(null);
            setConsentRetryDecisions(null);
            pushInfoBar({
                tone: 'danger',
                title: '读取 SnowLuma 协议失败',
                content: err instanceof Error ? err.message : String(err),
            });
        } finally {
            if (openingConsentBotRef.current === botId) {
                openingConsentBotRef.current = null;
            }
        }
    }, []);

    const prepareSnowLumaConsentOrOpen = useCallback(async (
        botId: string,
        decisions: DriftDecision[] | null = null,
    ) => {
        try {
            const payload = await botService.prepareSnowLumaAgreements(botId);
            if (payload?.consent_required) {
                openSnowLumaConsent(botId, decisions, payload);
                return false;
            }
            return true;
        } catch (err: unknown) {
            if (isSnowLumaConsentError(err)) {
                await openSnowLumaConsent(botId, decisions);
                return false;
            }
            pushInfoBar({
                tone: 'danger',
                title: 'SnowLuma 启动前检查失败',
                content: err instanceof Error ? err.message : String(err),
            });
            return false;
        }
    }, [openSnowLumaConsent]);

    const startBotDirect = useCallback(async (botId: string) => {
        setStartingBotId(botId);
        const ready = await prepareSnowLumaConsentOrOpen(botId);
        if (!ready) {
            setStartingBotId(null);
            return;
        }
        try {
            await mutations.startBotAsync(botId);
        } catch (err: unknown) {
            if (isDesktopConsentError(err)) {
                await requestDesktopConsent(async () => {
                    const again = await prepareSnowLumaConsentOrOpen(botId);
                    if (!again) return;
                    await mutations.startBotAsync(botId);
                });
                return;
            }
            if (isSnowLumaConsentError(err)) {
                await openSnowLumaConsent(botId);
                return;
            }
            throw err;
        } finally {
            setStartingBotId(null);
        }
    }, [mutations, openSnowLumaConsent, prepareSnowLumaConsentOrOpen]);

    const handleStartBot = useCallback(async (botId: string) => {
        // 顺序：Desktop 协议 → Docker 门禁 → 配置漂移 → SnowLuma 协议 → start
        const allowed = await requestDesktopConsent(async () => {
            clearConsentErrorSuppression(botId);
            setStartingBotId(botId);
            const runtimeGate = runtimeStartGate(botId);
            if (runtimeGate) {
                pushInfoBar({
                    tone: 'danger',
                    title: '无法启动',
                    content: runtimeGate,
                    key: `bot-start-gate:${botId}`,
                });
                setStartingBotId(null);
                return;
            }
            const gate = dockerStartGate(botId);
            if (gate) {
                pushInfoBar({
                    tone: 'danger',
                    title: '无法启动',
                    content: gate,
                    key: `bot-start-gate:${botId}`,
                });
                setStartingBotId(null);
                return;
            }
            let drift: ConfigDrift | null = null;
            try {
                drift = await botService.detectConfigDrift(botId);
            } catch {
                drift = null;
            }
            if (drift && (drift.added.length > 0 || drift.modified.length > 0)) {
                setDriftBotId(botId);
                setPendingDrift(drift);
                setStartingBotId(null);
                return;
            }
            startBotDirect(botId).catch(() => undefined);
        });
        if (!allowed) return;
    }, [clearConsentErrorSuppression, dockerStartGate, runtimeStartGate, startBotDirect]);

    const handleDriftConfirm = useCallback(async (decisions: DriftDecision[]) => {
        if (!driftBotId) return;
        clearConsentErrorSuppression(driftBotId);
        setStartingBotId(driftBotId);
        const runtimeGate = runtimeStartGate(driftBotId);
        if (runtimeGate) {
            setPendingDrift(null);
            pushInfoBar({
                tone: 'danger',
                title: '无法启动',
                content: runtimeGate,
                key: `bot-start-gate:${driftBotId}`,
            });
            setDriftBotId(null);
            setStartingBotId(null);
            return;
        }
        const gate = dockerStartGate(driftBotId);
        if (gate) {
            setPendingDrift(null);
            pushInfoBar({
                tone: 'danger',
                title: '无法启动',
                content: gate,
                key: `bot-start-gate:${driftBotId}`,
            });
            setDriftBotId(null);
            setStartingBotId(null);
            return;
        }
        setPendingDrift(null);
        try {
            const ready = await prepareSnowLumaConsentOrOpen(driftBotId, decisions);
            if (!ready) {
                setDriftBotId(null);
                return;
            }
            const snap = await botService.startWithDecisions(driftBotId, decisions);
            pushInfoBar({
                tone: 'success',
                title: '操作完成',
                content: `已发送启动指令给 Bot: ${snap.bot_id}`,
                autoDismissMs: 4000,
            });
        } catch (err: unknown) {
            if (isSnowLumaConsentError(err)) {
                await openSnowLumaConsent(driftBotId, decisions);
                setDriftBotId(null);
                return;
            }
            pushInfoBar({
                tone: 'danger',
                title: '启动失败',
                content: String(err),
            });
        }
        setDriftBotId(null);
        setStartingBotId(null);
    }, [clearConsentErrorSuppression, driftBotId, dockerStartGate, runtimeStartGate, openSnowLumaConsent, prepareSnowLumaConsentOrOpen]);

    const handleConsentConfirm = useCallback(async () => {
        if (!consentBotId || !consentPayload) return;
        setConsentSubmitting(true);
        const retryBotId = consentBotId;
        const retryDecisions = consentRetryDecisions;
        try {
            await botService.acceptSnowLumaAgreements(retryBotId, consentPayload.version);
            suppressCurrentConsentError(retryBotId);
        } catch (err: unknown) {
            pushInfoBar({
                tone: 'danger',
                title: 'SnowLuma 协议确认失败',
                content: err instanceof Error ? err.message : String(err),
            });
            setConsentSubmitting(false);
            return;
        }

        setConsentBotId(null);
        setConsentPayload(null);
        setConsentRetryDecisions(null);
        activeConsentBotRef.current = null;
        setConsentSubmitting(false);

        if (retryDecisions) {
            try {
                setStartingBotId(retryBotId);
                const ready = await prepareSnowLumaConsentOrOpen(retryBotId, retryDecisions);
                if (!ready) {
                    setStartingBotId(null);
                    return;
                }
                const snap = await botService.startWithDecisions(retryBotId, retryDecisions);
                pushInfoBar({
                    tone: 'success',
                    title: '操作完成',
                    content: `已发送启动指令给 Bot: ${snap.bot_id}`,
                    autoDismissMs: 4000,
                });
            } catch (err: unknown) {
                pushInfoBar({
                    tone: 'danger',
                    title: '启动失败',
                    content: err instanceof Error ? err.message : String(err),
                });
            } finally {
                setStartingBotId(null);
            }
        } else {
            startBotDirect(retryBotId).catch(() => undefined);
        }
    }, [consentBotId, consentPayload, consentRetryDecisions, prepareSnowLumaConsentOrOpen, startBotDirect, suppressCurrentConsentError]);

    const handleConsentCancel = useCallback(() => {
        if (consentSubmitting) return;
        const botId = consentBotId;
        if (botId) {
            suppressCurrentConsentError(botId);
        }
        setConsentBotId(null);
        setConsentPayload(null);
        setConsentRetryDecisions(null);
        activeConsentBotRef.current = null;
        if (botId) {
            botService.releaseSnowLumaAgreementSession(botId).catch(() => undefined);
        }
        setStartingBotId(null);
    }, [consentBotId, consentSubmitting, suppressCurrentConsentError]);

    const handleDriftCancel = useCallback(() => {
        setPendingDrift(null);
        setDriftBotId(null);
    }, []);

    useEffect(() => {
        if (activeConsentBotRef.current || openingConsentBotRef.current) return;
        const pending = botSnapshots.find((bot) => {
            const lastError = bot.last_error?.trim();
            if (!lastError || !isSnowLumaConsentError(lastError)) {
                suppressedConsentErrorsRef.current.delete(bot.bot_id);
                return false;
            }
            return suppressedConsentErrorsRef.current.get(bot.bot_id) !== lastError;
        });
        if (!pending) return;
        void openSnowLumaConsent(pending.bot_id);
    }, [botSnapshots, openSnowLumaConsent]);

    // 加载 / 错误状态也接 InfoBar，让顶部状态信息统一。但只在错误首次出现时
    // 推一次，避免 react-query 重试反复推。
    useEffect(() => {
        if (!error) return;
        pushInfoBar({
            key: 'bot-list-fetch-error',
            tone: 'danger',
            title: 'Bot 列表加载失败',
            content: error.message,
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

    const allSelected =
        botSnapshots.length > 0 && batch.selectedIds.size === botSnapshots.length;
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
                    <p className="text-2xs uppercase tracking-widest text-text-tertiary">
                        bots
                    </p>
                    <h1 className="font-display text-xl font-semibold text-text">
                        Bot 实例
                    </h1>
                    <p className="mt-1 text-sm text-text-secondary">
                        管理本机和远端 Bot 配置、生命周期与登录态。
                    </p>
                </div>
                <div className="flex items-baseline gap-1 text-xs text-text-tertiary tabular-nums">
                    <span>共</span>
                    <Counter value={botSnapshots.length} className="font-medium text-text-secondary" />
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
            <Dialog open={confirmDeleteOpen} onOpenChange={setConfirmDeleteOpen}>
                <DialogContent size="md">
                    <DialogTitle>确认批量删除选中实例？</DialogTitle>
                    <DialogDescription>
                        即将删除选中的 {batch.selectedIds.size} 个 Bot 实例的配置文件与数据项。
                        若有正在运行的 Bot，会先自动停止再删除。此操作不可撤销。
                    </DialogDescription>
                    <DialogFooter>
                        <Button
                            variant="ghost"
                            onClick={() => setConfirmDeleteOpen(false)}
                            disabled={mutations.isPending}
                        >
                            取消
                        </Button>
                        <Button
                            variant="primary"
                            onClick={onBatchDeleteConfirm}
                            disabled={mutations.isPending}
                            className="bg-danger hover:bg-danger/90"
                        >
                            彻底删除
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>

            <ImportRemoteBotsDialog open={importOpen} onOpenChange={setImportOpen} />

            {/* Config drift 确认 */}
            {pendingDrift && (
                <ConfigDriftDialog
                    open={!!pendingDrift}
                    drift={pendingDrift}
                    onConfirm={handleDriftConfirm}
                    onCancel={handleDriftCancel}
                />
            )}

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

function LoadingState() {
    return (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 py-20 text-text-tertiary">
            <Spinner size="lg" />
            <p className="text-sm">正在加载 Bot 实例…</p>
        </div>
    );
}

function ErrorState({ onRetry }: { onRetry: () => void }) {
    return (
        <PagePlaceholder className="gap-3">
            <p className="text-sm text-text-secondary">
                Bot 列表加载失败，详情见顶部提示条。
            </p>
            <Button size="sm" variant="primary" onClick={onRetry}>
                重试
            </Button>
        </PagePlaceholder>
    );
}

function EmptyState({
    onCreate,
    onImport,
}: {
    onCreate: () => void;
    onImport: () => void;
}) {
    return (
        <PagePlaceholder className="gap-4">
            <MotionIcon
                icon={Bot}
                motion="bob"
                playEnter
                enterKey="empty-bot"
                size={32}
                strokeWidth={1.6}
                className="text-text-tertiary"
            />
            <div>
                <p className="font-display text-md font-semibold text-text">
                    还没有 Bot 实例
                </p>
                <p className="mt-1 text-xs text-text-secondary">
                    可以新建，也可以把远端已有的 NapCat / SnowLuma 账号导进来。
                </p>
            </div>
            <div className="flex flex-wrap items-center justify-center gap-2">
                <Button
                    size="sm"
                    variant="secondary"
                    onClick={onImport}
                >
                    导入已有 Bot
                </Button>
                <Button
                    size="sm"
                    variant="primary"
                    onClick={onCreate}
                    data-tour-id="bot-create-empty"
                >
                    创建第一个实例
                </Button>
            </div>
        </PagePlaceholder>
    );
}

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
    startingBotId: string | null;
};

interface CardRect {
    id: string;
    left: number;
    right: number;
    top: number;
    bottom: number;
    centerX: number;
    centerY: number;
}

function BotListGrid({
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
    startingBotId,
}: GridProps) {
    const m = useMotion();
    const containerRef = useRef<HTMLDivElement>(null);
    const ghostRef = useRef<HTMLDivElement>(null);
    const [draggedId, setDraggedId] = useState<string | null>(null);
    const [hoverTargetId, setHoverTargetId] = useState<string | null>(null);

    const isDraggingRef = useRef(false);
    const startPosRef = useRef<{ x: number; y: number; id: string } | null>(null);
    const cardRectsRef = useRef<CardRect[]>([]);
    const rafIdRef = useRef<number | null>(null);
    const lastHoverTargetRef = useRef<string | null>(null);

    const ghostTiltDeg = !m.enabled
        ? 0
        : m.level === 'rich'
            ? 3.5
            : m.level === 'standard'
                ? 2.0
                : 0.8;

    const ghostScale = !m.enabled
        ? 1.0
        : m.level === 'rich'
            ? 1.06
            : m.level === 'standard'
                ? 1.03
                : 1.01;

    const measureCards = useCallback(() => {
        if (!containerRef.current) return [];
        const elements = containerRef.current.querySelectorAll<HTMLElement>('[data-bot-card-id]');
        const rects: CardRect[] = [];
        elements.forEach((el) => {
            const id = el.getAttribute('data-bot-card-id');
            if (id) {
                const r = el.getBoundingClientRect();
                rects.push({
                    id,
                    left: r.left,
                    right: r.right,
                    top: r.top,
                    bottom: r.bottom,
                    centerX: r.left + r.width / 2,
                    centerY: r.top + r.height / 2,
                });
            }
        });
        return rects;
    }, []);

    const handlePointerDown = (e: React.PointerEvent, botId: string) => {
        if (batch.isBatchMode || e.button !== 0) return;
        const target = e.target as HTMLElement;
        const isHandle = !!target.closest('[data-drag-handle]');
        if (!isHandle && target.closest('button, input, [role="button"], a, select, [tabindex], [data-no-drag]')) {
            return;
        }

        const startX = e.clientX;
        const startY = e.clientY;
        startPosRef.current = { x: startX, y: startY, id: botId };

        if (isHandle) {
            // 点击手柄：0 毫秒立即启动拖拽
            isDraggingRef.current = true;
            cardRectsRef.current = measureCards();
            lastHoverTargetRef.current = botId;
            setDraggedId(botId);
            setHoverTargetId(botId);
            if (ghostRef.current) {
                ghostRef.current.style.display = 'flex';
                ghostRef.current.style.transform = `translate3d(${startX + 14}px, ${startY + 14}px, 0) rotate(${ghostTiltDeg}deg) scale(${ghostScale})`;
            }
        }
    };

    useEffect(() => {
        const onPointerMove = (e: PointerEvent) => {
            const start = startPosRef.current;
            if (!start) return;

            if (!isDraggingRef.current) {
                const dist = Math.hypot(e.clientX - start.x, e.clientY - start.y);
                if (dist > 4) {
                    isDraggingRef.current = true;
                    cardRectsRef.current = measureCards();
                    lastHoverTargetRef.current = start.id;
                    setDraggedId(start.id);
                    setHoverTargetId(start.id);
                    if (ghostRef.current) {
                        ghostRef.current.style.display = 'flex';
                    }
                } else {
                    return;
                }
            }

            // 零 React 开销：GPU 直接位移跟随 + 动态姿态微倾角
            if (rafIdRef.current) cancelAnimationFrame(rafIdRef.current);
            rafIdRef.current = requestAnimationFrame(() => {
                if (ghostRef.current) {
                    ghostRef.current.style.transform = `translate3d(${e.clientX + 14}px, ${e.clientY + 14}px, 0) rotate(${ghostTiltDeg}deg) scale(${ghostScale})`;
                }
            });

            // 智能边界盒命中判定（解决对角线穿行时相邻卡片反复震荡乱跳）：
            // 只有当光标明确进入某个卡片区域时才切换插槽，缝隙过渡时平稳保持。
            let targetId = lastHoverTargetRef.current || start.id;
            for (const rect of cardRectsRef.current) {
                if (
                    e.clientX >= rect.left &&
                    e.clientX <= rect.right &&
                    e.clientY >= rect.top &&
                    e.clientY <= rect.bottom
                ) {
                    targetId = rect.id;
                    break;
                }
            }

            if (targetId !== lastHoverTargetRef.current) {
                lastHoverTargetRef.current = targetId;
                setHoverTargetId(targetId);
            }
        };

        const onPointerUp = () => {
            if (rafIdRef.current) {
                cancelAnimationFrame(rafIdRef.current);
                rafIdRef.current = null;
            }

            const currentDragged = startPosRef.current?.id;
            const currentTarget = lastHoverTargetRef.current;

            if (isDraggingRef.current && currentDragged && currentTarget && currentDragged !== currentTarget) {
                onReorderBots(currentDragged, currentTarget);
                // 放置成功时播放符合当前动效档位的微回弹反馈
                if (m.enabled && containerRef.current) {
                    const droppedEl = containerRef.current.querySelector<HTMLElement>(
                        `[data-bot-card-id="${currentDragged}"]`,
                    );
                    if (droppedEl) {
                        m.pop(droppedEl, { ease: 'release' });
                    }
                }
            }

            isDraggingRef.current = false;
            startPosRef.current = null;
            lastHoverTargetRef.current = null;
            setDraggedId(null);
            setHoverTargetId(null);
            if (ghostRef.current) {
                ghostRef.current.style.display = 'none';
            }
        };

        window.addEventListener('pointermove', onPointerMove, { passive: true });
        window.addEventListener('pointerup', onPointerUp);
        window.addEventListener('pointercancel', onPointerUp);
        return () => {
            window.removeEventListener('pointermove', onPointerMove);
            window.removeEventListener('pointerup', onPointerUp);
            window.removeEventListener('pointercancel', onPointerUp);
        };
    }, [ghostScale, ghostTiltDeg, m, measureCards, onReorderBots]);

    // 计算拖拽中的实时预览（两两对调模式：对角线移动时仅对调目标卡片，其余卡片保持静止）
    const displayBots = useMemo(() => {
        if (!draggedId || !hoverTargetId || draggedId === hoverTargetId) {
            return bots;
        }
        const srcIdx = bots.findIndex((b) => b.bot_id === draggedId);
        const dstIdx = bots.findIndex((b) => b.bot_id === hoverTargetId);
        if (srcIdx === -1 || dstIdx === -1) return bots;

        const next = [...bots];
        const temp = next[srcIdx];
        next[srcIdx] = next[dstIdx];
        next[dstIdx] = temp;
        return next;
    }, [bots, draggedId, hoverTargetId]);

    // FLIP (First-Last-Invert-Play) 实时滑动让位动效：
    // 用户移动卡片时，其他被挤压/替换的卡片会丝滑滑向新位置，视觉极其直观。
    const prevRectsRef = useRef<Map<string, DOMRect>>(new Map());

    useLayoutEffect(() => {
        if (!containerRef.current) return;
        const elements = containerRef.current.querySelectorAll<HTMLElement>('[data-bot-card-id]');
        const currentRects = new Map<string, DOMRect>();

        elements.forEach((el) => {
            const id = el.getAttribute('data-bot-card-id');
            if (id) {
                currentRects.set(id, el.getBoundingClientRect());
            }
        });

        elements.forEach((el) => {
            const id = el.getAttribute('data-bot-card-id');
            if (!id || id === draggedId) return;

            const prev = prevRectsRef.current.get(id);
            const current = currentRects.get(id);

            if (prev && current) {
                const deltaX = prev.left - current.left;
                const deltaY = prev.top - current.top;

                if (deltaX !== 0 || deltaY !== 0) {
                    if (m.enabled) {
                        gsap.fromTo(
                            el,
                            { x: deltaX, y: deltaY },
                            {
                                x: 0,
                                y: 0,
                                duration: Math.max(0.2, m.duration('fast') * 1.1),
                                ease: 'power2.out',
                                overwrite: 'auto',
                            },
                        );
                    }
                }
            }
        });

        prevRectsRef.current = currentRects;
    }, [displayBots, draggedId, m]);

    const alertRows = useMemo(
        () =>
            bots.map((bot) => {
                const config = configByBot[bot.bot_id] ?? null;
                const flavor = flavorByBot[bot.bot_id] ?? null;
                const name = config?.bot.name?.trim();
                return {
                    bot,
                    displayName: name && name.length > 0 ? name : bot.bot_id,
                    invalidationReason:
                        napcat.byBot[bot.bot_id]?.invalidationReason ?? null,
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
        { scope: containerRef, dependencies: [bots.length, m.enabled, m.level, m.speed, m.stagger] },
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
                                            pushInfoBar({
                                                key: `webui-open:${params.botId}`,
                                                tone: 'danger',
                                                title: '打开 WebUI 失败',
                                                content: String(err),
                                            });
                                        });
                                    }}
                                    isSnowlumaRemoteTunnelUi={isSnowlumaRemoteTunnelUi}
                                    onOpenNovnc={(id) => {
                                        openSnowlumaNovnc(id).catch((err: unknown) => {
                                            pushInfoBar({
                                                key: `novnc-open:${id}`,
                                                tone: 'danger',
                                                title: '打开 noVNC 失败',
                                                content: String(err),
                                            });
                                        });
                                    }}
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
                                            pushInfoBar({
                                                key: `snowluma-ui-retry:${id}`,
                                                tone: 'danger',
                                                title: '连接重建失败',
                                                content: String(err),
                                            });
                                        }
                                    }}
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
                    <span className="font-mono text-2xs text-text-tertiary">
                        QQ {draggedId}
                    </span>
                </div>
            </div>
        </>
    );
}
