import React, { useState } from 'react';
import {
    Button,
    Spinner,
    MessageBar,
    MessageBarBody,
    Tooltip,
    Text,
    Dialog,
    DialogSurface,
    DialogTitle,
    DialogBody,
    DialogContent,
    DialogActions,
} from '@fluentui/react-components';
import {
    AddRegular,
    ArrowClockwiseRegular,
    CheckmarkCircleRegular,
} from '@fluentui/react-icons';
import { useBotSnapshots } from '../../../hooks/bot/useBotSnapshots';
import { useBotMutations, type ActionMessage } from '../../../hooks/bot/useBotMutations';
import { useBotBatchSelection } from '../../../hooks/bot/useBotBatchSelection';
import { useBotFlavorMap } from '../../../hooks/bot/useBotFlavorMap';
import { useNapcatLogin } from '../../../hooks/webui/useNapcatLogin';
import { useSnowlumaState } from '../../../hooks/webui/useSnowlumaState';
import { isSnowLumaFlavor } from '../../../core/domain/bot/flavor';
import { BotCard } from './BotCard';
import { BatchToolbar } from './BatchToolbar';
import './BotListPage.css';

interface BotListPageProps {
    onConfigureBot: (botId: string | null) => void;
    onViewLogs: (botId: string) => void;
}

export const BotListPage: React.FC<BotListPageProps> = ({
    onConfigureBot,
    onViewLogs,
}) => {
    const [actionMessage, setActionMessage] = useState<ActionMessage | null>(null);
    const [isBatchDeleteDialogOpen, setIsBatchDeleteDialogOpen] = useState(false);

    const { data: botSnapshots = [], isLoading, error, refetch } = useBotSnapshots();
    const flavorByBot = useBotFlavorMap(botSnapshots);
    const napcat = useNapcatLogin();
    const snowluma = useSnowlumaState();
    const batch = useBotBatchSelection();

    const handleMessage = (msg: ActionMessage) => {
        setActionMessage(msg);
        // 批量动作完成后退出批量模式。
        if (msg.text.startsWith('批量')) batch.exitBatch();
    };

    const mutations = useBotMutations({ onMessage: handleMessage });

    const onBatchStart = () => {
        if (batch.selectedIds.size === 0) return;
        setActionMessage(null);
        mutations.batchStart(Array.from(batch.selectedIds));
    };
    const onBatchStop = () => {
        if (batch.selectedIds.size === 0) return;
        setActionMessage(null);
        mutations.batchStop(Array.from(batch.selectedIds));
    };
    const onBatchDeleteConfirm = () => {
        if (batch.selectedIds.size === 0) return;
        setActionMessage(null);
        mutations.batchDelete(Array.from(batch.selectedIds));
        setIsBatchDeleteDialogOpen(false);
    };

    return (
        <div className="ndf-bot-panel-container">
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', marginBottom: '20px' }}>
                <div>
                    <Text size={500} weight="semibold" style={{ color: 'var(--colorNeutralForeground1)' }}>
                        Bot 实例管理 (Bot Instance Manager)
                    </Text>
                    <Text size={100} block style={{ color: 'var(--colorNeutralForeground4)', marginTop: '4px' }}>
                        管理本地与远程的 Bot 实例、维护各自的数据路由以及底层引擎的生命周期。
                    </Text>
                </div>
            </div>

            {actionMessage && (
                <MessageBar intent={actionMessage.type} style={{ marginBottom: '16px' }}>
                    <MessageBarBody>{actionMessage.text}</MessageBarBody>
                </MessageBar>
            )}

            {isLoading ? (
                <div style={{ display: 'flex', flexDirection: 'column', gap: '10px', alignItems: 'center', padding: '100px 0' }}>
                    <Spinner size="large" />
                    <Text size={200} style={{ color: 'var(--colorNeutralForeground4)' }}>正在检索 Bot 实例快照数据...</Text>
                </div>
            ) : error ? (
                <div style={{ padding: '24px', backgroundColor: 'var(--colorPaletteRedBackground1)', border: '1px solid var(--colorPaletteRedBorder1)', borderRadius: '6px' }}>
                    <Text size={300} weight="semibold" style={{ color: 'var(--colorPaletteRedForeground1)' }}>检索数据失败</Text>
                    <Text size={200} block style={{ marginTop: '4px', color: 'var(--colorPaletteRedForeground1)' }}>{error.message}</Text>
                </div>
            ) : botSnapshots.length === 0 ? (
                <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', padding: '80px 0', border: '1px dashed var(--ndf-border-subtle)', borderRadius: '12px', backgroundColor: 'var(--ndf-bg-card)' }}>
                    <Text size={300} weight="semibold" style={{ color: 'var(--colorNeutralForeground3)' }}>暂无 Bot 实例快照</Text>
                    <Text size={100} block style={{ color: 'var(--colorNeutralForeground4)', marginTop: '4px', marginBottom: '16px' }}>
                        你尚未创建任何 Bot 实例配置文件，点击右下角加号按钮来初始化首个配置。
                    </Text>
                    <Button icon={<AddRegular />} appearance="primary" onClick={() => onConfigureBot(null)}>
                        创建第一个实例
                    </Button>
                </div>
            ) : (
                <div className="ndf-bot-flow-grid">
                    {botSnapshots.map((bot) => {
                        const flavor = flavorByBot[bot.bot_id] ?? null;
                        const napcatBot = napcat.byBot[bot.bot_id];
                        const snowlumaBot = snowluma.byBot[bot.bot_id];
                        return (
                            <BotCard
                                key={bot.bot_id}
                                bot={bot}
                                flavor={flavor}
                                onStart={mutations.startBot}
                                onStop={mutations.stopBot}
                                onConfigure={onConfigureBot}
                                onViewLogs={onViewLogs}
                                isBatchMode={batch.isBatchMode}
                                isSelected={batch.selectedIds.has(bot.bot_id)}
                                onToggleSelect={batch.toggleSelect}
                                qrcodeUrl={napcatBot?.qrcodeUrl ?? null}
                                isOnline={napcatBot?.online ?? null}
                                invalidationReason={napcatBot?.invalidationReason ?? null}
                                napcatBinding={napcatBot?.webui ?? null}
                                isSnowLuma={isSnowLumaFlavor(flavor)}
                                snowlumaDaemonState={snowluma.daemonState}
                                snowlumaInjected={snowlumaBot?.injected ?? false}
                                snowlumaUin={snowlumaBot?.uin ?? null}
                                snowlumaLoginState={snowlumaBot?.loginState ?? null}
                            />
                        );
                    })}
                </div>
            )}

            {!batch.isBatchMode && (
                <div className="ndf-floating-group">
                    <Tooltip content="批量管理 (Batch Actions)" relationship="label">
                        <Button
                            className="ndf-circle-btn"
                            icon={<CheckmarkCircleRegular style={{ fontSize: '18px', color: 'var(--colorNeutralForeground2)' }} />}
                            onClick={batch.toggleBatch}
                        />
                    </Tooltip>

                    <Tooltip content="刷新状态 (Manual Refresh)" relationship="label">
                        <Button
                            className="ndf-circle-btn"
                            icon={<ArrowClockwiseRegular style={{ fontSize: '18px', color: 'var(--colorNeutralForeground2)' }} />}
                            onClick={() => refetch()}
                        />
                    </Tooltip>

                    <Tooltip content="新增 Bot 实例 (Create Config)" relationship="label">
                        <Button
                            className="ndf-circle-btn ndf-primary-circle"
                            icon={<AddRegular style={{ fontSize: '20px', color: '#ffffff' }} />}
                            onClick={() => onConfigureBot(null)}
                        />
                    </Tooltip>
                </div>
            )}

            <BatchToolbar
                selectedCount={batch.selectedIds.size}
                onBatchStart={onBatchStart}
                onBatchStop={onBatchStop}
                onBatchDelete={() => setIsBatchDeleteDialogOpen(true)}
                onCancel={batch.toggleBatch}
                isLoading={mutations.isPending}
            />

            <Dialog open={isBatchDeleteDialogOpen} onOpenChange={(_, data) => setIsBatchDeleteDialogOpen(data.open)}>
                <DialogSurface style={{ maxWidth: '400px' }}>
                    <DialogBody>
                        <DialogTitle>确认批量删除选中的实例？</DialogTitle>
                        <DialogContent style={{ marginTop: '10px' }}>
                            <Text>
                                你即将删除选中的 <Text weight="bold" style={{ color: '#bc2f32' }}>{batch.selectedIds.size}</Text> 个 Bot 实例的全部配置文件与数据项。
                            </Text>
                            <Text block style={{ color: 'var(--colorNeutralForeground4)', marginTop: '8px', fontSize: '12px' }}>
                                如果其中有任何正在运行的 Bot，系统会先自动将其停止后再彻底删除。此操作不可撤销！
                            </Text>
                        </DialogContent>
                        <DialogActions style={{ marginTop: '16px' }}>
                            <Button appearance="secondary" onClick={() => setIsBatchDeleteDialogOpen(false)}>
                                取消
                            </Button>
                            <Button
                                appearance="primary"
                                style={{ backgroundColor: '#bc2f32', color: '#ffffff', border: 'none' }}
                                onClick={onBatchDeleteConfirm}
                                disabled={mutations.isPending}
                            >
                                彻底删除
                            </Button>
                        </DialogActions>
                    </DialogBody>
                </DialogSurface>
            </Dialog>
        </div>
    );
};
