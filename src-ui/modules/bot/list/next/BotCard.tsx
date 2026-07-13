// 列表 BotCard（new tree）。
//
// 单卡布局（BotManageCard）：
//   1. Header：[复选框] [Avatar] [名称 + QQ · flavor · 相对时间]
//   2. meta：补一句副标题（不与底栏状态徽章重复）；错误走 InfoBar
//   3. Chip：配置摘要单行槽（空也占位）
//   4. 底栏：进程 + 账号 + 告警（最多 3 枚短徽章），右工具按钮
//
// 操作区按钮按状态收缩：日志 / WebUI 只在 running / starting 时显示（停止状态
// 这俩按了也没意义）；启停 / 配置永远显示。
//
// 头像 BotAvatar 把 overflow-hidden 限制在内层 img 包装上，外层留给指示点的
// absolute 定位，圆点不会被裁。
//
// 批量模式下整行变 selectable，左侧出复选框。

import { useEffect, useRef, useState } from 'react';
import {
    Activity,
    Check,
    FileText,
    Globe,
    LinkIcon,
    Monitor,
    Play,
    Power,
    QrCode,
    RefreshCw,
    Settings,
    Square,
} from 'lucide-react';
import { ActionMotionIcon } from '../../../../shared/ui/motion';
import { useMotion } from '../../../../hooks/preferences/useMotion';
import type {
    BotActorSnapshot,
    DaemonState,
    NapCatLoginInvalidationReason,
    SnowLumaLoginState,
} from '../../../../core/ipc/types';
import type { BotConfig } from '../../../../core/ipc/generated/domain/BotConfig';
import {
    canStartBot,
    canStopBot,
    isBotActive,
    isBotRunning,
    isBotStarting,
} from '../../../../core/domain/bot/status';
import {
    isWebuiAvailable,
    webuiTooltip,
    type NapcatWebuiBinding,
} from '../../../../core/domain/webui/availability';
import { isSnowLumaFlavor, type Flavor } from '../../../../core/domain/bot/flavor';
import {
    isSnowlumaRemoteNativeConfig,
    isSnowlumaTunnelReady,
} from '../../../../core/domain/bot/snowluma-remote-ui';
import { cn } from '../../../../shared/utils/cn';
import { pushInfoBar } from '../../../../hooks/ui/globalInfoBarStore';
import { QrCodeDialog } from './QrCodeDialog';
import { BotManageCard } from './BotManageCard';
import { buildBotListCardStatus, botListCardMetaLine } from './botCardPresentation';
import {
    BotAvatar,
    IconButton,
    InfoChip,
    ToolbarMotionIcon,
    channelDetailLabel,
    countEnabledChannels,
    formatRelativeTime,
    formatRestartHint,
} from './botCardParts';
import { useIsHostReachable } from '../../../../hooks/remote/useIsHostReachable';
import { isRuntimeTargetLocal, remoteHostIdFromRuntimeTarget } from '../../../../core/domain/bot/runtime-target';
import { BotRuntimeMetricsStrip } from './BotRuntimeMetricsStrip';

interface BotCardProps {
    bot: BotActorSnapshot;
    config?: BotConfig | null;
    flavor: Flavor | null;
    qrcodeUrl?: string | null;
    isOnline?: boolean | null;
    invalidationReason?: NapCatLoginInvalidationReason | null;
    napcatBinding?: NapcatWebuiBinding | null;
    /** 仅用于 WebUI 可用性判断，不在卡片上展示 */
    snowlumaDaemonState?: DaemonState | null;
    /** 远端 SnowLuma Docker：隧道就绪（Native 远端用 daemon + 同套 IPC） */
    snowlumaDockerEndpointsReady?: boolean;
    snowlumaUin?: string | null;
    snowlumaLoginState?: SnowLumaLoginState | null;

    isBatchMode: boolean;
    isSelected: boolean;
    actionPending?: boolean;

    onStart: (botId: string) => void;
    onStop: (botId: string) => void;
    onConfigure: (botId: string) => void;
    onViewLogs: (botId: string) => void;
    onToggleSelect: (botId: string) => void;
    onOpenWebui: (params: {
        botId: string;
        flavor: Flavor | null;
        napcat: NapcatWebuiBinding | null;
    }) => void;
    /** 远端 SnowLuma：Docker 或 Native 下展示 noVNC（扫码） */
    isSnowlumaRemoteTunnelUi?: boolean;
    onOpenNovnc?: (botId: string) => void;
}

export function BotCard({
    bot,
    config,
    flavor,
    qrcodeUrl,
    isOnline,
    invalidationReason,
    napcatBinding,
    snowlumaDaemonState,
    snowlumaDockerEndpointsReady,
    snowlumaUin,
    snowlumaLoginState,
    isBatchMode,
    isSelected,
    actionPending = false,
    onStart,
    onStop,
    onConfigure,
    onViewLogs,
    onToggleSelect,
    onOpenWebui,
    isSnowlumaRemoteTunnelUi = false,
    onOpenNovnc,
}: BotCardProps) {
    const [qrOpen, setQrOpen] = useState(false);

    const isSL = isSnowLumaFlavor(flavor);
    const hasQrcode = !!qrcodeUrl;

    // Transport 层失败检查（优先于 Bot 状态和组件状态）
    const isRemoteTarget = config?.bot.runtime_target != null && !isRuntimeTargetLocal(config.bot.runtime_target);
    const remoteHostIdForCheck = isRemoteTarget && config?.bot.runtime_target != null
        ? remoteHostIdFromRuntimeTarget(config.bot.runtime_target)
        : null;
    const remoteReachable = useIsHostReachable(remoteHostIdForCheck);
    const transportFailed = isRemoteTarget && remoteHostIdForCheck != null && !remoteReachable;

    // 状态切换反馈:
    //   - 关键状态转移(starting→running 等) → 状态徽章 pop,而不是整张卡 pop
    //     (大卡 pop 会跟 hover lift / shadow 叠加放大成"整张卡突然鼓一下")
    const m = useMotion();
    const badgeRef = useRef<HTMLSpanElement>(null);
    const prevStateRef = useRef<typeof bot.state>(bot.state);

    useEffect(() => {
        const el = badgeRef.current;
        if (!el || !m.enabled) {
            prevStateRef.current = bot.state;
            return;
        }
        const prev = prevStateRef.current;
        if (prev !== bot.state) {
            const isImpactful =
                (prev === 'starting' && bot.state === 'running') ||
                (prev === 'stopping' && bot.state === 'stopped') ||
                (prev === 'running' && bot.state === 'stopped');
            if (isImpactful && m.preset.feel.popPeak > 1) {
                m.pop(el);
            }
        }
        prevStateRef.current = bot.state;
    }, [bot.state, m.enabled, m.level, m.speed, m.pop, m.preset.feel.popPeak]);

    const webuiAvailable = isWebuiAvailable({
        flavor,
        napcat: napcatBinding ?? null,
        snowlumaDaemonState: snowlumaDaemonState ?? null,
        snowlumaDockerEndpointsReady: snowlumaDockerEndpointsReady ?? false,
        snowlumaRemoteNativeTunnelReady:
            isSnowlumaRemoteNativeConfig(config ?? null) &&
            snowlumaDaemonState === 'ready',
    });
    const webuiTip = webuiTooltip({ flavor, available: webuiAvailable });

    const displayName =
        config?.bot.name && config.bot.name.trim().length > 0
            ? config.bot.name.trim()
            : bot.bot_id;
    const enabledChannels = config ? countEnabledChannels(config) : null;
    const restartHint = config ? formatRestartHint(config) : null;
    const runtimeTarget = config?.bot.runtime_target ?? null;
    const slStartMode = config?.bot.snowlumaStartMode;

    const handleRowClick = () => {
        if (isBatchMode) onToggleSelect(bot.bot_id);
    };

    const stopAction = (fn: () => void) => (e: React.MouseEvent) => {
        e.stopPropagation();
        fn();
    };

    const lastTransitionRel = bot.last_transition
        ? formatRelativeTime(bot.last_transition)
        : null;

    const needsQrLogin = hasQrcode && isOnline !== true;

    const cardStatus = buildBotListCardStatus({
        state: bot.state,
        flavor: flavor ?? null,
        pendingRestart: !!bot.pending_restart,
        needsQrLogin,
        isOnline,
        snowlumaLoginState,
        snowlumaDaemonState,
    });

    const metaText = botListCardMetaLine({
        flavor: flavor ?? null,
        state: bot.state,
        snowlumaLoginState,
        snowlumaUin,
    });

    const cardAccent =
        transportFailed
            ? 'danger'
            : (isBotStarting(bot.state) || bot.state === 'repairing' ? 'brand' : 'none');

    const isActive = isBotActive(bot.state);
    const startPending = actionPending && !isActive;

    const snowlumaTunnelReady = isSnowlumaTunnelReady({
        config: config ?? null,
        dockerEndpointsReady: snowlumaDockerEndpointsReady ?? false,
        daemonState: snowlumaDaemonState ?? null,
    });

    const novncAvailable =
        isSnowlumaRemoteTunnelUi &&
        snowlumaTunnelReady &&
        isActive;

    const chips: React.ReactNode[] = [];
    if (!isSL && enabledChannels !== null) {
        chips.push(
            <InfoChip
                key="channels"
                icon={LinkIcon}
                label="对外"
                value={
                    enabledChannels.total > 0
                        ? `${enabledChannels.total} 路`
                        : '未配置'
                }
                muted={enabledChannels.total === 0}
                tooltip={
                    enabledChannels.total > 0
                        ? channelDetailLabel(enabledChannels)
                        : '在配置页中添加 HTTP / WebSocket / 反向连接以接入业务'
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
                label="自启"
                value={restartHint}
            />,
        );
    }
    if (runtimeTarget && runtimeTarget !== 'local') {
        chips.push(
            <InfoChip
                key="runtime"
                icon={Activity}
                label="运行"
                value={runtimeTarget}
            />,
        );
    }
    if (isSL && slStartMode) {
        chips.push(
            <InfoChip
                key="sl-mode"
                icon={Power}
                label="启动"
                value={slStartMode.mode === 'cold_start' ? '冷启动' : '热启动'}
            />,
        );
    }
    if (transportFailed) {
        chips.unshift(
            <InfoChip
                key="remote-unreachable"
                icon={Activity}
                label="远端"
                value="主机不可达"
                muted={false}
            />,
        );
    }
    const visibleChips = chips.slice(0, 3);
    const metricsStrip = <BotRuntimeMetricsStrip botId={bot.bot_id} />;

    return (
        <>
            <BotManageCard
                status={cardStatus}
                selected={isSelected}
                batchMode={isBatchMode}
                accent={cardAccent}
                onRowClick={isBatchMode ? handleRowClick : undefined}
                processBadgeRef={badgeRef}
                metaExtra={metricsStrip}
                header={
                    <>
                        {isBatchMode && (
                            <span
                                aria-hidden
                                className={cn(
                                    'inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-xs border',
                                    isSelected
                                        ? 'border-brand bg-brand text-white'
                                        : 'border-border bg-canvas',
                                )}
                            >
                                {isSelected && <Check size={10} strokeWidth={3} />}
                            </span>
                        )}
                        <BotAvatar
                            qqid={bot.bot_id}
                            displayName={displayName}
                            flavorTone={isSL ? 'info' : 'brand'}
                        />
                        <div className="min-w-0 flex-1">
                            <h3
                                className="truncate font-display text-base font-semibold leading-snug text-text"
                                title={displayName}
                            >
                                {displayName}
                            </h3>
                            <p className="mt-0.5 flex min-w-0 flex-wrap items-center gap-x-1.5 gap-y-0 text-2xs text-text-tertiary">
                                <span className="font-mono tabular-nums">QQ {bot.bot_id}</span>
                                {flavor && (
                                    <>
                                        <span aria-hidden className="text-border">
                                            ·
                                        </span>
                                        <span
                                            className={cn(
                                                'font-medium',
                                                isSL ? 'text-info' : 'text-brand',
                                            )}
                                        >
                                            {flavor}
                                        </span>
                                    </>
                                )}
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
                    </>
                }
                meta={
                    metaText ? (
                        <p className="truncate font-mono text-xs text-text-secondary tabular-nums">
                            {metaText}
                        </p>
                    ) : null
                }
                chips={visibleChips.length > 0 ? visibleChips : undefined}
                footerActions={
                    isBatchMode ? (
                        <span className="text-2xs text-text-tertiary">点击卡片选择</span>
                    ) : (
                        <>
                            <IconButton
                                visible={hasQrcode}
                                tooltip="扫码登录"
                                onClick={() => setQrOpen(true)}
                                tone="brand"
                            >
                                <ToolbarMotionIcon
                                    icon={QrCode}
                                    size={16}
                                    strokeWidth={2.2}
                                    hoverAccent
                                />
                            </IconButton>
                            <IconButton
                                visible={isActive}
                                tooltip="停止 Bot"
                                onClick={stopAction(() => onStop(bot.bot_id))}
                                disabled={!canStopBot(bot.state)}
                                tone="danger"
                            >
                                <ActionMotionIcon
                                    icon={Square}
                                    size={14}
                                    strokeWidth={2.6}
                                    motion="none"
                                />
                            </IconButton>
                            <IconButton
                                visible={!isActive && canStartBot(bot.state)}
                                tooltip={
                                    startPending
                                        ? '正在准备启动'
                                        : transportFailed
                                            ? '远端主机不可达，无法启动'
                                            : '启动 Bot'
                                }
                                onClick={stopAction(() => onStart(bot.bot_id))}
                                disabled={!canStartBot(bot.state) || transportFailed || startPending}
                                tone="success"
                            >
                                {startPending ? (
                                    <ActionMotionIcon
                                        icon={RefreshCw}
                                        size={14}
                                        strokeWidth={2.4}
                                        motion="spin"
                                    />
                                ) : (
                                    <ToolbarMotionIcon
                                        icon={Play}
                                        size={14}
                                        strokeWidth={2.6}
                                        hoverAccent
                                    />
                                )}
                            </IconButton>
                            <IconButton
                                visible={isBotRunning(bot.state) || isBotStarting(bot.state)}
                                tooltip="查看日志"
                                onClick={stopAction(() => onViewLogs(bot.bot_id))}
                            >
                                <ToolbarMotionIcon
                                    icon={FileText}
                                    size={14}
                                    strokeWidth={2.2}
                                    hoverAccent
                                />
                            </IconButton>
                            <IconButton
                                visible={novncAvailable}
                                tooltip={
                                    isSnowlumaRemoteNativeConfig(config ?? null)
                                        ? '打开远端 noVNC 扫码页（SSH 隧道至主机 6081）'
                                        : '打开 noVNC 扫码页（容器内 QQ 图形界面）'
                                }
                                onClick={stopAction(() => onOpenNovnc?.(bot.bot_id))}
                            >
                                <ToolbarMotionIcon
                                    icon={Monitor}
                                    size={14}
                                    strokeWidth={2.2}
                                    hoverAccent
                                />
                            </IconButton>
                            <IconButton
                                visible={isBotRunning(bot.state) || isBotStarting(bot.state)}
                                tooltip={webuiTip}
                                disabled={!webuiAvailable}
                                onClick={stopAction(() =>
                                    onOpenWebui({
                                        botId: bot.bot_id,
                                        flavor,
                                        napcat: napcatBinding ?? null,
                                    }),
                                )}
                            >
                                <ToolbarMotionIcon
                                    icon={Globe}
                                    size={14}
                                    strokeWidth={2.2}
                                    hoverAccent
                                />
                            </IconButton>
                            <IconButton
                                visible={true}
                                tooltip="配置"
                                onClick={stopAction(() => onConfigure(bot.bot_id))}
                            >
                                <ToolbarMotionIcon
                                    icon={Settings}
                                    size={14}
                                    strokeWidth={2.2}
                                    hoverAccent
                                />
                            </IconButton>
                        </>
                    )
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
        </>
    );
}

