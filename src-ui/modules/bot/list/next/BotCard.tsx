// 卡片只组合各区域：状态和操作由 controller 决定，指标独立订阅。
import { ContextMenu, ContextMenuTrigger } from '../../../../shared/ui';
import { BotManageCard } from './BotManageCard';
import { BotCardHeader } from './BotCardHeader';
import { BotCardMetrics } from './BotCardMetrics';
import { BotCardToolbar, BotCardContextMenu } from './BotCardActions';
import { BotCardDialogs } from './BotCardDialogs';
import { useBotCardController } from './useBotCardController';
import { useBotStatusBadgeMotion } from './useBotStatusBadgeMotion';
import type { BotCardProps } from './BotCard.types';

export function BotCard(props: BotCardProps) {
    const model = useBotCardController(props);
    const badgeRef = useBotStatusBadgeMotion(props.bot.state);
    return (
        <ContextMenu>
            <ContextMenuTrigger asChild disabled={props.isBatchMode}>
                <div className="min-w-0">
                    <BotManageCard
                        status={model.status}
                        selected={props.isSelected}
                        batchMode={props.isBatchMode}
                        accent={model.accent}
                        processBadgeRef={badgeRef}
                        onRowClick={
                            props.isBatchMode
                                ? () => props.onToggleSelect(props.bot.bot_id)
                                : undefined
                        }
                        header={
                            <BotCardHeader
                                bot={props.bot}
                                config={props.config}
                                flavor={props.flavor}
                                displayName={model.displayName}
                                runtime={model.runtime}
                            />
                        }
                        metaExtra={
                            <BotCardMetrics
                                botId={props.bot.bot_id}
                                onViewMetrics={props.onViewMetrics}
                            />
                        }
                        footerActions={
                            <BotCardToolbar
                                actions={model.actions}
                                onAction={model.onAction}
                                batchMode={props.isBatchMode}
                            />
                        }
                    />
                </div>
            </ContextMenuTrigger>
            <BotCardContextMenu
                actions={model.actions}
                onAction={model.onAction}
                label={`QQ ${props.bot.bot_id} ${model.displayName !== props.bot.bot_id ? `(${model.displayName})` : ''}`}
            />
            <BotCardDialogs {...model.dialogs} />
        </ContextMenu>
    );
}
