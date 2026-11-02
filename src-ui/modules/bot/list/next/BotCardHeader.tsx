import { LinkIcon, Power, RefreshCw, Server, WifiOff } from 'lucide-react';
import { isSnowLumaFlavor } from '../../../../core/domain/bot/flavor';
import { isBotActive } from '../../../../core/domain/bot/status';
import { formatRelativeTime } from '../../../../core/domain/ui/relativeTime';
import { cn } from '../../../../shared/utils/cn';
import {
    BotAvatar,
    InfoChip,
    channelDetailLabel,
    countEnabledChannels,
    formatRestartHint,
} from './botCardParts';
import type { BotCardProps } from './BotCard.types';

export function BotCardHeader({
    bot,
    config,
    flavor,
    displayName,
    runtime,
}: Pick<BotCardProps, 'bot' | 'config' | 'flavor'> & {
    displayName: string;
    runtime: { remote: boolean; label: string; tooltip: string; transportFailed: boolean };
}) {
    const isSL = isSnowLumaFlavor(flavor);
    const isActive = isBotActive(bot.state);
    const { transportFailed } = runtime;
    const enabledChannels = config ? countEnabledChannels(config) : null;
    const restartHint = config ? formatRestartHint(config) : null;
    const slStartMode = config?.bot.snowlumaStartMode;
    const lastTransitionRel = bot.last_transition ? formatRelativeTime(bot.last_transition) : null;
    const chips: React.ReactNode[] = [];
    if (runtime.remote) {
        const hostLabel = runtime.label;
        if (transportFailed) {
            chips.push(
                <InfoChip
                    key="runtime"
                    icon={WifiOff}
                    tone="danger"
                    tooltip={`远端主机不可达：${hostLabel}（请检查网络或主机状态）`}
                />,
            );
        } else {
            chips.push(
                <InfoChip key="runtime" icon={Server} tooltip={`运行位置：${runtime.tooltip}`} />,
            );
        }
    }
    if (isSL && slStartMode) {
        chips.push(
            <InfoChip
                key="sl-mode"
                icon={Power}
                tooltip={
                    slStartMode.mode === 'cold_start' ? '启动方式：冷启动' : '启动方式：热启动'
                }
            />,
        );
    }
    if (!isSL && enabledChannels !== null) {
        chips.push(
            <InfoChip
                key="channels"
                icon={LinkIcon}
                muted={enabledChannels.total === 0}
                tooltip={
                    enabledChannels.total > 0
                        ? `对外连接：${channelDetailLabel(enabledChannels)}`
                        : '对外连接：未配置'
                }
            />,
        );
    }
    if (restartHint) {
        chips.push(
            <InfoChip
                key="restart"
                icon={RefreshCw}
                iconMotion={isActive ? 'breathe' : 'none'}
                tooltip={`自动重启：${restartHint}`}
            />,
        );
    }
    return (
        <>
            <BotAvatar
                qqid={bot.bot_id}
                displayName={displayName}
                flavorTone={isSL ? 'info' : 'brand'}
            />
            <div className="flex min-w-0 flex-1 items-start justify-between gap-2">
                <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-1.5">
                        <h3
                            className="truncate font-display text-base font-semibold leading-snug text-text"
                            title={displayName}
                        >
                            {displayName}
                        </h3>
                        {flavor && (
                            <span
                                className={cn(
                                    'shrink-0 inline-flex items-center rounded-xs px-1.5 py-0.5 font-mono text-[10px] font-semibold uppercase tracking-wider',
                                    isSL
                                        ? 'bg-info/12 text-info border border-info/25'
                                        : 'bg-brand/12 text-brand border border-brand/25',
                                )}
                            >
                                {flavor}
                            </span>
                        )}
                    </div>
                    <p className="mt-0.5 flex min-w-0 flex-wrap items-center gap-x-1.5 gap-y-0 text-2xs text-text-tertiary">
                        <span className="font-mono text-xs tabular-nums text-text-secondary">
                            QQ {bot.bot_id}
                        </span>
                        {lastTransitionRel && (
                            <>
                                <span aria-hidden className="text-border">
                                    ·
                                </span>
                                <span className="tabular-nums">{lastTransitionRel}</span>
                            </>
                        )}
                    </p>
                </div>
                {chips.length > 0 ? (
                    <div className="flex shrink-0 items-center justify-end gap-1 flex-nowrap overflow-hidden pt-0.5">
                        {chips}
                    </div>
                ) : null}
            </div>
        </>
    );
}
