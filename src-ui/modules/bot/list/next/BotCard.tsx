// 列表 Bot 卡：头像/标题行 + 中间配置 Chip/指标行 + 底栏状态与操作。

import { useEffect, useRef, useState } from 'react';
import { LinkIcon, Power, RefreshCw, Server, WifiOff } from 'lucide-react';
import { ContextMenu, ContextMenuTrigger } from '../../../../shared/ui';
import {
    BotCardToolbar,
    BotCardContextMenu,
    type BotCardActionProps,
    type ActionState,
} from './BotCardActions';
import { useFeatureEnabled } from '../../../../hooks/preferences/featureTogglesStore';
import { useDebugConsoleEnabled } from '../../../../hooks/debug/useDebugConsoleEnabled';
import { BotCardMetrics } from './BotCardMetrics';
import { useMotion } from '../../../../hooks/preferences/useMotion';
import type {
    BotActorSnapshot,
    DaemonState,
    NapCatLoginInvalidationReason,
    NapCatWebuiAuthIssue,
    SnowLumaLoginState,
} from '../../../../core/ipc/types';
import type { BotConfig } from '../../../../core/ipc/generated/domain/BotConfig';
import { isBotActive, isBotStarting } from '../../../../core/domain/bot/status';
import {
    isWebuiAvailable,
    webuiTooltip,
    type NapcatWebuiBinding,
} from '../../../../core/domain/webui/availability';
import { isSnowLumaFlavor, type Flavor } from '../../../../core/domain/bot/flavor';
import {
    isSnowlumaRemoteNativeConfig,
    isSnowlumaRemoteUiRetryAvailable,
    isSnowlumaTunnelReady,
} from '../../../../core/domain/bot/snowluma-remote-ui';
import { cn } from '../../../../shared/utils/cn';
import { pushInfoBar } from '../../../../hooks/ui/globalInfoBarStore';
import { QrCodeDialog } from './QrCodeDialog';
import { RemoteSnowLumaQrLoginDialog } from './RemoteSnowLumaQrLoginDialog';
import { useRemoteSnowlumaQrEnabled } from '../../../../hooks/webui/useRemoteSnowlumaQrLogin';
import { localOs } from '../../../../core/domain/runtime/env';
import { BotManageCard } from './BotManageCard';
import { buildBotListCardStatus } from './botCardPresentation';
import {
    BotAvatar,
    InfoChip,
    channelDetailLabel,
    countEnabledChannels,
    formatRestartHint,
} from './botCardParts';
import { formatRelativeTime } from '../../../../core/domain/ui/relativeTime';
import { useIsHostReachable, useServerProfiles } from '../../../../hooks/remote/useIsHostReachable';
import {
    isRuntimeTargetLocal,
    remoteHostIdFromRuntimeTarget,
    runtimeTargetDisplayLabel,
    runtimeTargetTooltip,
} from '../../../../core/domain/bot/runtime-target';
import { BotMigrationDialog } from '../../dialogs/BotMigrationDialog';

interface BotCardProps {
    bot: BotActorSnapshot;
    config?: BotConfig | null;
    flavor: Flavor | null;
    qrcodeUrl?: string | null;
    isOnline?: boolean | null;
    invalidationReason?: NapCatLoginInvalidationReason | null;
    napcatAuthIssue?: NapCatWebuiAuthIssue | null;
    napcatBinding?: NapcatWebuiBinding | null;
    /** 仅用于 WebUI 可用性判断，不在卡片上展示 */
    snowlumaDaemonState?: DaemonState | null;
    /** 远端 SnowLuma Docker：隧道就绪（Native 远端用 daemon + 同套 IPC） */
    snowlumaDockerEndpointsReady?: boolean;
    snowlumaLoginState?: SnowLumaLoginState | null;
    snowlumaProbeUnavailable?: boolean;

    isBatchMode: boolean;
    isSelected: boolean;
    actionPending?: boolean;

    onStart: (botId: string) => void;
    onStop: (botId: string) => void;
    onConfigure: (botId: string) => void;
    onViewLogs: (botId: string) => void;
    onViewMetrics: (botId: string) => void;
    onToggleSelect: (botId: string) => void;
    onOpenWebui: (params: {
        botId: string;
        flavor: Flavor | null;
        napcat: NapcatWebuiBinding | null;
    }) => void;
    /** 远端 SnowLuma：Docker 或 Native 下展示 noVNC（扫码） */
    isSnowlumaRemoteTunnelUi?: boolean;
    onOpenNovnc?: (botId: string) => void;
    /** 远端 SnowLuma 隧道失效时重建 WebUI/noVNC，不重启 Bot。 */
    onRetrySnowlumaUi?: (botId: string) => Promise<void>;
}

export function BotCard({
    bot,
    config,
    flavor,
    qrcodeUrl,
    isOnline,
    invalidationReason,
    napcatAuthIssue,
    napcatBinding,
    snowlumaDaemonState,
    snowlumaDockerEndpointsReady,
    snowlumaLoginState,
    snowlumaProbeUnavailable,
    isBatchMode,
    isSelected,
    actionPending = false,
    onStart,
    onStop,
    onConfigure,
    onViewLogs,
    onViewMetrics,
    onToggleSelect,
    onOpenWebui,
    isSnowlumaRemoteTunnelUi = false,
    onOpenNovnc,
    onRetrySnowlumaUi,
}: BotCardProps) {
    const [qrOpen, setQrOpen] = useState(false);
    const [remoteQrOpen, setRemoteQrOpen] = useState(false);
    const [migrationOpen, setMigrationOpen] = useState(false);
    const localQrEligible =
        localOs() === 'windows' &&
        config?.bot.backend_type === 'snowluma' &&
        isRuntimeTargetLocal(config.bot.runtime_target) &&
        config.bot.deploymentType === 'native' &&
        (config.bot.snowlumaStartMode?.mode ?? 'cold_start') === 'cold_start';
    const remoteQrEnabled = useRemoteSnowlumaQrEnabled(
        localQrEligible ||
            (config?.bot.backend_type === 'snowluma' &&
                (config.bot.deploymentType === 'docker' ||
                    !isRuntimeTargetLocal(config.bot.runtime_target))),
        localQrEligible,
    );

    const isSL = isSnowLumaFlavor(flavor);
    const hasQrcode = !!qrcodeUrl;

    // Transport 层失败检查（优先于 Bot 状态和组件状态）
    const isRemoteTarget =
        config?.bot.runtime_target != null && !isRuntimeTargetLocal(config.bot.runtime_target);
    const remoteHostIdForCheck =
        isRemoteTarget && config?.bot.runtime_target != null
            ? remoteHostIdFromRuntimeTarget(config.bot.runtime_target)
            : null;
    const remoteReachable = useIsHostReachable(remoteHostIdForCheck);
    const transportFailed = isRemoteTarget && remoteHostIdForCheck != null && !remoteReachable;
    const serversQuery = useServerProfiles(isRemoteTarget);
    const servers = serversQuery.data ?? [];

    // 状态切换反馈:
    //   - 关键状态转移(starting→running 等) → 状态徽章 pop,而不是整张卡 pop
    //     (大卡 pop 会跟 hover lift / shadow 叠加放大成"整张卡突然鼓一下")
    const { enabled: motionEnabled, pop: popBadge, preset: motionPreset } = useMotion();
    const popPeak = motionPreset.feel.popPeak;
    const badgeRef = useRef<HTMLSpanElement>(null);
    const prevStateRef = useRef<typeof bot.state>(bot.state);

    useEffect(() => {
        const el = badgeRef.current;
        if (!el || !motionEnabled) {
            prevStateRef.current = bot.state;
            return;
        }
        const prev = prevStateRef.current;
        if (prev !== bot.state) {
            const isImpactful =
                (prev === 'starting' && bot.state === 'running') ||
                (prev === 'stopping' && bot.state === 'stopped') ||
                (prev === 'running' && bot.state === 'stopped');
            if (isImpactful && popPeak > 1) {
                popBadge(el);
            }
        }
        prevStateRef.current = bot.state;
    }, [bot.state, motionEnabled, popBadge, popPeak]);

    const webuiAvailable = isWebuiAvailable({
        flavor,
        napcat: napcatBinding ?? null,
        snowlumaDaemonState: snowlumaDaemonState ?? null,
        snowlumaDockerEndpointsReady: snowlumaDockerEndpointsReady ?? false,
        snowlumaRemoteNativeTunnelReady:
            isSnowlumaRemoteNativeConfig(config ?? null) && snowlumaDaemonState === 'ready',
    });
    const webuiTip = webuiTooltip({ flavor, available: webuiAvailable });

    const displayName =
        config?.bot.name && config.bot.name.trim().length > 0 ? config.bot.name.trim() : bot.bot_id;
    const enabledChannels = config ? countEnabledChannels(config) : null;
    const restartHint = config ? formatRestartHint(config) : null;
    const runtimeTarget = config?.bot.runtime_target ?? null;
    const isDockerBot = config?.bot.deploymentType === 'docker';
    const slStartMode = config?.bot.snowlumaStartMode;

    const handleRowClick = () => {
        if (isBatchMode) onToggleSelect(bot.bot_id);
    };

    const lastTransitionRel = bot.last_transition ? formatRelativeTime(bot.last_transition) : null;

    const needsQrLogin = hasQrcode && isOnline !== true;

    const cardStatus = buildBotListCardStatus({
        state: bot.state,
        flavor: flavor ?? null,
        pendingRestart: !!bot.pending_restart,
        needsQrLogin,
        isOnline,
        napcatAuthIssue,
        snowlumaLoginState,
        snowlumaDaemonState,
        snowlumaProbeUnavailable,
        snowlumaDockerEndpointsReady: snowlumaDockerEndpointsReady ?? false,
    });

    const cardAccent = transportFailed
        ? 'danger'
        : isBotStarting(bot.state) || bot.state === 'repairing'
          ? 'brand'
          : 'none';

    const isActive = isBotActive(bot.state);
    const startPending = actionPending && !isActive;

    const snowlumaTunnelReady = isSnowlumaTunnelReady({
        config: config ?? null,
        dockerEndpointsReady: snowlumaDockerEndpointsReady ?? false,
        daemonState: snowlumaDaemonState ?? null,
    });

    const novncAvailable = isSnowlumaRemoteTunnelUi && snowlumaTunnelReady && isActive;
    const remoteQrAvailable =
        remoteQrEnabled && bot.state === 'running' && snowlumaLoginState !== 'logged_in';
    useEffect(() => {
        if (!remoteQrEnabled || bot.state !== 'running' || snowlumaLoginState === 'logged_in') {
            setRemoteQrOpen(false);
        }
    }, [bot.state, remoteQrEnabled, snowlumaLoginState]);
    const snowlumaUiRetryAvailable = isSnowlumaRemoteUiRetryAvailable({
        config: config ?? null,
        active: bot.state === 'running',
        transportFailed,
    });

    const chips: React.ReactNode[] = [];
    if (runtimeTarget && runtimeTarget !== 'local') {
        const hostLabel = runtimeTargetDisplayLabel(runtimeTarget, servers);
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
                <InfoChip
                    key="runtime"
                    icon={Server}
                    tooltip={`运行位置：${runtimeTargetTooltip(runtimeTarget, servers)}`}
                />,
            );
        }
    } else if (transportFailed) {
        chips.push(
            <InfoChip
                key="remote-unreachable"
                icon={WifiOff}
                tone="danger"
                tooltip="远端主机不可达，请检查网络或主机连接"
            />,
        );
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
    const visibleChips = chips.slice(0, 4);
    const actions = useBotCardActionState({
        botId: bot.bot_id,
        state: bot.state,
        displayName,
        flavor,
        napcatBinding,
        isBatchMode,
        isActive,
        isDockerBot,
        hasQrcode,
        startPending,
        transportFailed,
        webuiAvailable,
        webuiTip,
        novncAvailable,
        remoteQrAvailable,
        snowlumaUiRetryAvailable,
        onStart,
        onStop,
        onConfigure,
        onViewLogs,
        onViewMetrics,
        onOpenWebui,
        onOpenNovnc,
        onRetrySnowlumaUi,
        onOpenQr: () => setQrOpen(true),
        onOpenRemoteQr: () => setRemoteQrOpen(true),
        onOpenMigration: () => setMigrationOpen(true),
    });

    return (
        <ContextMenu>
            <ContextMenuTrigger asChild disabled={isBatchMode}>
                <div className="min-w-0">
                    <BotManageCard
                        status={cardStatus}
                        selected={isSelected}
                        batchMode={isBatchMode}
                        accent={cardAccent}
                        onRowClick={isBatchMode ? handleRowClick : undefined}
                        processBadgeRef={badgeRef}
                        metaExtra={
                            <BotCardMetrics botId={bot.bot_id} onViewMetrics={onViewMetrics} />
                        }
                        header={
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
                                                    <span className="tabular-nums">
                                                        {lastTransitionRel}
                                                    </span>
                                                </>
                                            )}
                                        </p>
                                    </div>
                                    {visibleChips.length > 0 ? (
                                        <div className="flex shrink-0 items-center justify-end gap-1 flex-nowrap overflow-hidden pt-0.5">
                                            {visibleChips}
                                        </div>
                                    ) : null}
                                </div>
                            </>
                        }
                        meta={null}
                        footerActions={<BotCardToolbar {...actions} />}
                    />
                </div>
            </ContextMenuTrigger>

            <BotCardContextMenu {...actions} />

            <BotMigrationDialog
                botId={bot.bot_id}
                sourceRuntimeTarget={config?.bot.runtime_target ?? 'local'}
                open={migrationOpen}
                onOpenChange={setMigrationOpen}
                onOpenLogin={
                    hasQrcode
                        ? () => setQrOpen(true)
                        : remoteQrAvailable
                          ? () => setRemoteQrOpen(true)
                          : novncAvailable && onOpenNovnc
                            ? () => onOpenNovnc(bot.bot_id)
                            : undefined
                }
            />
            <QrCodeDialog
                open={qrOpen}
                onOpenChange={setQrOpen}
                qrcodeUrl={qrcodeUrl ?? null}
                botId={bot.bot_id}
                isOnline={isOnline}
                invalidationReason={invalidationReason}
                onLoginSuccess={() => {
                    setQrOpen(false);
                    pushInfoBar({
                        tone: 'success',
                        title: '扫码登录成功',
                        // 保留 QQ 号定位上下文：多 Bot 同时挂着时一眼能分清是哪个登录上了。
                        content: `Bot ${bot.bot_id} 已上线`,
                        autoDismissMs: 3000,
                    });
                }}
                onKicked={() => {
                    setQrOpen(false);
                }}
            />
            {remoteQrEnabled && (
                <RemoteSnowLumaQrLoginDialog
                    initialRefresh
                    botId={bot.bot_id}
                    open={remoteQrOpen && bot.state === 'running'}
                    onOpenChange={setRemoteQrOpen}
                    isOnline={snowlumaLoginState === 'logged_in'}
                    onOpenVnc={
                        novncAvailable && onOpenNovnc ? () => onOpenNovnc(bot.bot_id) : undefined
                    }
                />
            )}
        </ContextMenu>
    );
}

// 一份重试状态供工具栏和右键菜单共享；切换入口也不会发出重复重建请求。
function useBotCardActionState(props: BotCardActionProps): ActionState {
    const terminalEnabled = useFeatureEnabled('terminal');
    const debugEnabled = useDebugConsoleEnabled();
    const [retryingSnowlumaUi, setRetryingSnowlumaUi] = useState(false);
    const runSnowlumaUiRetry = async () => {
        if (!props.onRetrySnowlumaUi || retryingSnowlumaUi) return;
        setRetryingSnowlumaUi(true);
        try {
            await props.onRetrySnowlumaUi(props.botId);
        } finally {
            setRetryingSnowlumaUi(false);
        }
    };
    return { ...props, terminalEnabled, debugEnabled, retryingSnowlumaUi, runSnowlumaUiRetry };
}
