// BotListPage 主壳（next）。
//
// 职责：把 6 个 hook 串起来 + 拼装子组件。视觉走暖粉桃色调，沿用 Components
// 页的设计语言（顶部小工具栏 + 自适应卡片网格 + 浮动菜单 + InfoBar 全局队列）。
//
// 跟旧 Fluent BotListPage 的差异：
//   - 错误反馈走全局 InfoBar（pushInfoBar），不再用本地 actionMessage state +
//     行内 MessageBar，跟 Components 页一致。
//   - 批量删除确认改成 shared/ui Dialog，不再行内 Fluent Dialog。
//   - 浮动菜单拆出 FloatingActions / BatchBottomBar 两个组件，互斥显示。
//   - 卡片网格用 Tailwind grid auto-fit，按窗口宽度自动 1/2/3 列。

import { useEffect, useMemo, useRef, useState, useCallback } from 'react';
import { Bot } from 'lucide-react';
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
import { useMotion } from '../../../hooks/preferences/useMotion';
import { useBotSnapshots } from '../../../hooks/bot/useBotSnapshots';
import { useBotMutations, type ActionMessage } from '../../../hooks/bot/useBotMutations';
import { useBotBatchSelection } from '../../../hooks/bot/useBotBatchSelection';
import { useBotFlavorMap } from '../../../hooks/bot/useBotFlavorMap';
import { useBotConfigsMap } from '../../../hooks/bot/useBotConfigsMap';
import { useBotDockerStartGate } from '../../../hooks/bot/useBotDockerStartGate';
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
    const { data: botSnapshots = [], isLoading, error, refetch } = useBotSnapshots();
    const flavorByBot = useBotFlavorMap(botSnapshots);
    const configByBot = useBotConfigsMap(botSnapshots);
    const { startBlock: dockerStartGate } = useBotDockerStartGate(configByBot);
    const napcat = useNapcatLogin();
    const snowluma = useSnowlumaState();
    const batch = useBotBatchSelection();
    const openWebui = useOpenWebui();
    const openSnowlumaNovnc = useOpenSnowlumaNovnc();
    // Desktop 协议门禁统一走 App 级 host，避免本页再挂一份 gate/Dialog 打架
    // 批量删除二次确认
    const [confirmDeleteOpen, setConfirmDeleteOpen] = useState(false);

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
    }, [clearConsentErrorSuppression, dockerStartGate, startBotDirect]);

    const handleDriftConfirm = useCallback(async (decisions: DriftDecision[]) => {
        if (!driftBotId) return;
        clearConsentErrorSuppression(driftBotId);
        setStartingBotId(driftBotId);
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
    }, [clearConsentErrorSuppression, driftBotId, dockerStartGate, openSnowLumaConsent, prepareSnowLumaConsentOrOpen]);

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
                <div className="text-xs text-text-tertiary tabular-nums">
                    共 <Counter value={botSnapshots.length} className="font-semibold text-text" /> 个实例
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
                    <EmptyState onCreate={onCreateBot} />
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

function EmptyState({ onCreate }: { onCreate: () => void }) {
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
                    创建第一个配置后，这里就会显示出来。
                </p>
            </div>
            <Button
                size="sm"
                variant="primary"
                onClick={onCreate}
                data-tour-id="bot-create-empty"
            >
                创建第一个实例
            </Button>
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
    startingBotId: string | null;
};

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
    startingBotId,
}: GridProps) {
    const m = useMotion();
    const containerRef = useRef<HTMLDivElement>(null);

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

    // 列表 stagger: 只对新出现的子节点播放动画。
    // 路由切换时组件 mount，所有子节点都是"新的"，会自动播放。
    useGSAP(
        () => {
            const root = containerRef.current;
            if (!root || !m.enabled) return;
            return animateListChildrenEnterAfterPaint(root, bots.length, m);
        },
        { scope: containerRef, dependencies: [bots.length, m.enabled, m.level, m.speed, m.stagger] },
    );

    return (
        <div ref={containerRef} className={gridStyles.botCardGrid}>
            {bots.map((bot) => {
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
                return (
                    <ListItem key={bot.bot_id} hoverable>
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
                            snowlumaUin={snowlumaBot?.uin ?? null}
                            snowlumaLoginState={snowlumaBot?.loginState ?? null}
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
                        />
                    </ListItem>
                );
            })}
        </div>
    );
}
