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

import { useEffect, useState, useCallback } from 'react';
import { Bot } from 'lucide-react';
import { AnimatePresence, motion } from 'framer-motion';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogTitle,
    Spinner,
} from '../../../shared/ui';
import { ListItem } from '../../../shared/ui/motion';
import { useMotion } from '../../../hooks/preferences/useMotion';
import { listContainerVariants } from '../../../core/design/motion';
import { useBotSnapshots } from '../../../hooks/bot/useBotSnapshots';
import { useBotMutations, type ActionMessage } from '../../../hooks/bot/useBotMutations';
import { useBotBatchSelection } from '../../../hooks/bot/useBotBatchSelection';
import { useBotFlavorMap } from '../../../hooks/bot/useBotFlavorMap';
import { useBotConfigsMap } from '../../../hooks/bot/useBotConfigsMap';
import { useNapcatLogin } from '../../../hooks/webui/useNapcatLogin';
import { useSnowlumaState } from '../../../hooks/webui/useSnowlumaState';
import { useOpenWebui } from '../../../hooks/webui/useOpenWebui';
import { pushInfoBar } from '../../../hooks/ui/globalInfoBarStore';
import { botService } from '../../../core/services/bot.service';
import type { ConfigDrift } from '../../../core/ipc/generated/ConfigDrift';
import type { DriftDecision } from '../../../core/ipc/generated/DriftDecision';
import { BotCard } from './next/BotCard';
import { FloatingActions } from './next/FloatingActions';
import { BatchBottomBar } from './next/BatchBottomBar';
import { ConfigDriftDialog } from '../dialogs/ConfigDriftDialog';

interface BotListPageNextProps {
    onConfigureBot: (botId: string | null) => void;
    onViewLogs: (botId: string) => void;
}

export function BotListPageNext({
    onConfigureBot,
    onViewLogs,
}: BotListPageNextProps) {
    const { data: botSnapshots = [], isLoading, error, refetch } = useBotSnapshots();
    const flavorByBot = useBotFlavorMap(botSnapshots);
    const configByBot = useBotConfigsMap(botSnapshots);
    const napcat = useNapcatLogin();
    const snowluma = useSnowlumaState();
    const batch = useBotBatchSelection();
    const openWebui = useOpenWebui();

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
            autoDismissMs: msg.type === 'success' ? 4000 : 0,
        });
        // 批量动作完成后退出批量模式（对齐旧版交互）。
        if (msg.text.startsWith('批量')) batch.exitBatch();
    };

    const mutations = useBotMutations({ onMessage: handleMessage });

    // ── Config drift detection before start ──────────────────────────────
    const [pendingDrift, setPendingDrift] = useState<ConfigDrift | null>(null);
    const [driftBotId, setDriftBotId] = useState<string | null>(null);

    const handleStartBot = useCallback(async (botId: string) => {
        try {
            const drift = await botService.detectConfigDrift(botId);
            if (drift && !drift.added.length && !drift.modified.length) {
                // clean, start directly
                mutations.startBot(botId);
                return;
            }
            if (drift) {
                // has drift, show dialog
                setDriftBotId(botId);
                setPendingDrift(drift);
            } else {
                // null = no drift or files don't exist
                mutations.startBot(botId);
            }
        } catch (err: unknown) {
            // detection failed, fallback to direct start
            mutations.startBot(botId);
        }
    }, [mutations]);

    const handleDriftConfirm = useCallback(async (decisions: DriftDecision[]) => {
        if (!driftBotId) return;
        setPendingDrift(null);
        try {
            const snap = await botService.startWithDecisions(driftBotId, decisions);
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
                content: String(err),
            });
        }
        setDriftBotId(null);
    }, [driftBotId]);

    const handleDriftCancel = useCallback(() => {
        setPendingDrift(null);
        setDriftBotId(null);
    }, []);

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
        mutations.batchStart(Array.from(batch.selectedIds));
    };
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
            <header className="flex shrink-0 items-end justify-between pb-4 pt-2">
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
                    共 <span className="font-semibold text-text">{botSnapshots.length}</span> 个实例
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
                    <ErrorState message={error.message} onRetry={() => refetch()} />
                ) : botSnapshots.length === 0 ? (
                    <EmptyState onCreate={() => onConfigureBot(null)} />
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
                        onConfigureBot={onConfigureBot}
                        onViewLogs={onViewLogs}
                        onStartBot={handleStartBot}
                    />
                )}
            </div>

            {/* 浮动操作组(互斥):FloatingActions / BatchBottomBar 切换走 AnimatePresence,
                避免互换瞬变,保持视觉连贯。 */}
            <AnimatePresence mode="wait" initial={false}>
                {batch.isBatchMode ? (
                    <BatchBottomBar
                        key="batch"
                        selectedCount={batch.selectedIds.size}
                        totalCount={botSnapshots.length}
                        allSelected={allSelected}
                        onSelectAll={selectAll}
                        onSelectNone={selectNone}
                        onBatchStart={onBatchStart}
                        onBatchStop={onBatchStop}
                        onBatchDelete={() => setConfirmDeleteOpen(true)}
                        onExitBatch={batch.toggleBatch}
                        busy={mutations.isPending}
                    />
                ) : (
                    <FloatingActions
                        key="floating"
                        busy={mutations.isPending}
                        onCreate={() => onConfigureBot(null)}
                        onRefresh={() => refetch()}
                        onEnterBatch={batch.toggleBatch}
                    />
                )}
            </AnimatePresence>

            {/* 批量删除确认 */}
            <Dialog open={confirmDeleteOpen} onOpenChange={setConfirmDeleteOpen}>
                <DialogContent>
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

function ErrorState({ message, onRetry }: { message: string; onRetry: () => void }) {
    return (
        <div className="rounded-md bg-danger-soft p-5 ring-1 ring-danger/20">
            <p className="font-display text-md font-semibold text-danger">加载失败</p>
            <p className="mt-1 text-sm text-text-secondary">{message}</p>
            <Button size="sm" variant="ghost" onClick={onRetry} className="mt-3">
                重试
            </Button>
        </div>
    );
}

function EmptyState({ onCreate }: { onCreate: () => void }) {
    return (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 rounded-md border border-dashed border-border-subtle bg-elevated/50 py-16 text-center">
            <Bot size={32} strokeWidth={1.6} className="text-text-tertiary" />
            <div>
                <p className="font-display text-md font-semibold text-text">
                    还没有 Bot 实例
                </p>
                <p className="mt-1 text-xs text-text-secondary">
                    创建第一个配置后，这里就会显示出来。
                </p>
            </div>
            <Button size="sm" variant="primary" onClick={onCreate}>
                创建第一个实例
            </Button>
        </div>
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
    onConfigureBot: (botId: string | null) => void;
    onViewLogs: (botId: string) => void;
    onStartBot: (botId: string) => void;
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
    onConfigureBot,
    onViewLogs,
    onStartBot,
}: GridProps) {
    const m = useMotion();
    const container = listContainerVariants(m.preset.stagger);

    return (
        <motion.div
            className="flex flex-col gap-3"
            variants={container}
            initial="initial"
            animate="animate"
        >
            <AnimatePresence initial={false}>
                {bots.map((bot) => {
                    const flavor = flavorByBot[bot.bot_id] ?? null;
                    const config = configByBot[bot.bot_id] ?? null;
                    const napcatBot = napcat.byBot[bot.bot_id];
                    const snowlumaBot = snowluma.byBot[bot.bot_id];
                    return (
                        <ListItem key={bot.bot_id} layout hoverable>
                            <BotCard
                                bot={bot}
                                config={config}
                                flavor={flavor}
                                qrcodeUrl={napcatBot?.qrcodeUrl ?? null}
                                isOnline={napcatBot?.online ?? null}
                                invalidationReason={napcatBot?.invalidationReason ?? null}
                                napcatBinding={napcatBot?.webui ?? null}
                                snowlumaDaemonState={snowluma.daemonState}
                                snowlumaInjected={snowlumaBot?.injected ?? false}
                                snowlumaUin={snowlumaBot?.uin ?? null}
                                snowlumaLoginState={snowlumaBot?.loginState ?? null}
                                isBatchMode={batch.isBatchMode}
                                isSelected={batch.selectedIds.has(bot.bot_id)}
                                onStart={onStartBot}
                                onStop={mutations.stopBot}
                                onConfigure={onConfigureBot}
                                onViewLogs={onViewLogs}
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
                            />
                        </ListItem>
                    );
                })}
            </AnimatePresence>
        </motion.div>
    );
}
