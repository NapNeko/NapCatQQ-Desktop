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

import { forwardRef, useEffect, useRef, useState, type ComponentType } from 'react';
import type { LucideProps } from 'lucide-react';
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
import gsap from 'gsap';
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from '../../../../shared/ui';
import {
    ActionMotionIcon,
    GsapPresence,
    MotionIcon,
    type EnterFn,
    type ExitFn,
    type MotionIconPreset,
} from '../../../../shared/ui/motion';
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
    }, [bot.state, m.enabled, m.level, m.speed]);

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

    const needsQrLogin = hasQrcode && isOnline === false;

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
        isBotStarting(bot.state) || bot.state === 'repairing' ? 'brand' : 'none';

    const isActive = isBotActive(bot.state);

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
    const visibleChips = chips.slice(0, 3);

    return (
        <>
            <BotManageCard
                    status={cardStatus}
                    selected={isSelected}
                    batchMode={isBatchMode}
                    accent={cardAccent}
                    onRowClick={isBatchMode ? handleRowClick : undefined}
                    processBadgeRef={badgeRef}
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
                                <GsapPresence
                                    visible={hasQrcode}
                                    onEnter={iconBtnEnter}
                                    onExit={iconBtnExit}
                                >
                                    <IconButton
                                        presence
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
                                </GsapPresence>
                                <GsapPresence
                                    visible={isActive}
                                    onEnter={iconBtnEnter}
                                    onExit={iconBtnExit}
                                >
                                    <IconButton
                                        presence
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
                                </GsapPresence>
                                <GsapPresence
                                    visible={!isActive && canStartBot(bot.state)}
                                    onEnter={iconBtnEnter}
                                    onExit={iconBtnExit}
                                >
                                    <IconButton
                                        presence
                                        tooltip="启动 Bot"
                                        onClick={stopAction(() => onStart(bot.bot_id))}
                                        disabled={!canStartBot(bot.state)}
                                        tone="success"
                                    >
                                        <ToolbarMotionIcon
                                            icon={Play}
                                            size={14}
                                            strokeWidth={2.6}
                                            hoverAccent
                                        />
                                    </IconButton>
                                </GsapPresence>
                                <GsapPresence
                                    visible={isBotRunning(bot.state) || isBotStarting(bot.state)}
                                    onEnter={iconBtnEnter}
                                    onExit={iconBtnExit}
                                >
                                    <IconButton
                                        presence
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
                                </GsapPresence>
                                <GsapPresence
                                    visible={novncAvailable}
                                    onEnter={iconBtnEnter}
                                    onExit={iconBtnExit}
                                >
                                    <IconButton
                                        presence
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
                                </GsapPresence>
                                <GsapPresence
                                    visible={isBotRunning(bot.state) || isBotStarting(bot.state)}
                                    onEnter={iconBtnEnter}
                                    onExit={iconBtnExit}
                                >
                                    <IconButton
                                        presence
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
                                </GsapPresence>
                                <IconButton
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

// ============== 头像 ==============

function BotAvatar({
    qqid,
    displayName,
    flavorTone,
}: {
    qqid: string;
    displayName: string;
    flavorTone: 'brand' | 'info';
}) {
    const [failed, setFailed] = useState(false);
    const numericQQ = /^\d+$/.test(qqid) ? qqid : null;
    const showImg = numericQQ && !failed;
    const initials = (displayName.trim().charAt(0) || '?').toUpperCase();
    const palette = pickAvatarPalette(qqid);

    return (
        <div className="relative h-11 w-11 shrink-0">
            <div
                className={cn(
                    'h-full w-full overflow-hidden rounded-md ring-1 ring-border-subtle',
                    'bg-gradient-to-br',
                    palette,
                )}
            >
                {showImg && (
                    <img
                        src={`https://q.qlogo.cn/headimg_dl?dst_uin=${numericQQ}&spec=640`}
                        alt=""
                        className="h-full w-full object-cover"
                        referrerPolicy="no-referrer"
                        draggable={false}
                        onError={() => setFailed(true)}
                    />
                )}
                {!showImg && (
                    <div className="flex h-full w-full items-center justify-center font-display text-base font-semibold text-white/95">
                        {initials}
                    </div>
                )}
            </div>
            <span
                aria-hidden
                className={cn(
                    'absolute -bottom-0.5 -right-0.5 inline-block h-2.5 w-2.5 rounded-full ring-2 ring-elevated',
                    flavorTone === 'info' ? 'bg-info' : 'bg-brand',
                )}
            />
        </div>
    );
}

const PALETTES = [
    'from-pink-300 to-rose-400',
    'from-amber-300 to-orange-400',
    'from-emerald-300 to-teal-400',
    'from-sky-300 to-indigo-400',
    'from-violet-300 to-fuchsia-400',
    'from-rose-300 to-red-400',
] as const;

function pickAvatarPalette(seed: string): string {
    let h = 0;
    for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) | 0;
    return PALETTES[Math.abs(h) % PALETTES.length] ?? PALETTES[0];
}

// ============== Chip / IconButton ==============

function ToolbarMotionIcon({
    icon,
    size,
    strokeWidth,
    hoverAccent = true,
}: {
    icon: ComponentType<LucideProps>;
    size: number;
    strokeWidth: number;
    hoverAccent?: boolean;
}) {
    return (
        <MotionIcon
            icon={icon}
            motion="none"
            hoverAccent={hoverAccent}
            size={size}
            strokeWidth={strokeWidth}
            playEnter={false}
        />
    );
}

function InfoChip({
    icon,
    iconMotion = 'none',
    label,
    value,
    muted,
    tooltip,
}: {
    icon: ComponentType<LucideProps>;
    iconMotion?: MotionIconPreset;
    label: string;
    value: React.ReactNode;
    muted?: boolean;
    tooltip?: string;
}) {
    const node = (
        <span
            className={cn(
                'inline-flex max-w-full items-center gap-1 rounded-pill border border-border-subtle/80',
                'bg-inset/80 px-2 py-0.5 text-2xs',
                muted ? 'text-text-tertiary' : 'text-text-secondary',
            )}
        >
            <span className="inline-flex text-text-tertiary">
                <ActionMotionIcon
                    icon={icon}
                    motion={iconMotion}
                    size={11}
                    strokeWidth={2.4}
                    playEnter={false}
                />
            </span>
            <span className="text-text-tertiary">{label}</span>
            <span className={cn('font-medium', muted ? 'text-text-tertiary' : 'text-text')}>
                {value}
            </span>
        </span>
    );
    if (!tooltip) return node;
    return (
        <Tooltip>
            <TooltipTrigger asChild>{node}</TooltipTrigger>
            <TooltipContent>{tooltip}</TooltipContent>
        </Tooltip>
    );
}

interface IconButtonProps {
    tooltip: string;
    onClick: (e: React.MouseEvent) => void;
    disabled?: boolean;
    tone?: 'neutral' | 'brand' | 'success' | 'danger';
    children: React.ReactNode;
    /// 被 GsapPresence 包裹时传 true,首帧 visibility:hidden 让 GSAP 的 enter
    /// fromTo 接管;常驻按钮(配置)不传,默认显示。
    presence?: boolean;
}

// 底栏工具钮：只用 autoAlpha，避免 scale 与 bindHover 抢 transform 导致卡顿；
// 时长略短，多张卡同时进退场时 GPU 压力更小。
const iconBtnEnter: EnterFn = (el, env) =>
    gsap.fromTo(
        el,
        { autoAlpha: 0, visibility: 'visible' },
        {
            autoAlpha: 1,
            duration: env.duration('fast') * 0.85,
            ease: env.ease.enterMicro,
        },
    );
const iconBtnExit: ExitFn = (el, env) =>
    gsap.to(el, {
        autoAlpha: 0,
        duration: env.duration('fast') * 0.55,
        ease: env.ease.exit,
    });

const IconButton = forwardRefIcon();

function forwardRefIcon() {
    return forwardRef<HTMLButtonElement, IconButtonProps>(function IconButtonImpl(
        { tooltip, onClick, disabled, tone = 'neutral', children, presence },
        ref,
    ) {
        const m = useMotion();
        const localRef = useRef<HTMLButtonElement | null>(null);
        const setRef = (node: HTMLButtonElement | null) => {
            localRef.current = node;
            if (typeof ref === 'function') ref(node);
            else if (ref) (ref as React.MutableRefObject<HTMLButtonElement | null>).current = node;
        };

        // hover/tap 弹性。GsapPresence 在外层管 enter/exit,这里只管交互反馈。
        useEffect(() => {
            const el = localRef.current;
            if (!el || !m.enabled || disabled || presence) return;
            // presence 钮由 GsapPresence 管显隐；再绑 scale hover 会与 autoAlpha 抢帧。
            return m.bindHover(el, { lift: null, shadow: false, brightness: false });
        }, [m.enabled, m.level, m.speed, disabled, presence]);

        return (
            <Tooltip>
                <TooltipTrigger asChild>
                    <button
                        ref={setRef}
                        type="button"
                        onClick={onClick}
                        disabled={disabled}
                        style={presence ? { visibility: 'hidden', opacity: 0 } : undefined}
                        className={cn(
                            'inline-flex h-8 w-8 items-center justify-center rounded-xs',
                            'transition-colors duration-100',
                            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                            'disabled:cursor-not-allowed disabled:opacity-40',
                            tone === 'neutral' && 'text-text-secondary hover:bg-inset hover:text-text',
                            tone === 'brand' && 'text-brand hover:bg-brand-soft',
                            tone === 'success' && 'text-success hover:bg-success-soft',
                            tone === 'danger' && 'text-danger hover:bg-danger-soft',
                        )}
                    >
                        {children}
                    </button>
                </TooltipTrigger>
                <TooltipContent>{tooltip}</TooltipContent>
            </Tooltip>
        );
    });
}

// ============== Helpers ==============

interface ChannelCount {
    httpServer: number;
    httpSse: number;
    httpClient: number;
    wsServer: number;
    wsClient: number;
    plugins: number;
    total: number;
}

function countEnabledChannels(config: BotConfig): ChannelCount {
    const c = config.connect;
    if (!c) {
        return {
            httpServer: 0,
            httpSse: 0,
            httpClient: 0,
            wsServer: 0,
            wsClient: 0,
            plugins: 0,
            total: 0,
        };
    }
    const httpServer = (c.httpServers ?? []).filter((x) => x.enable).length;
    const httpSse = (c.httpSseServers ?? []).filter((x) => x.enable).length;
    const httpClient = (c.httpClients ?? []).filter((x) => x.enable).length;
    const wsServer = (c.websocketServers ?? []).filter((x) => x.enable).length;
    const wsClient = (c.websocketClients ?? []).filter((x) => x.enable).length;
    const plugins = (c.plugins ?? []).length;
    return {
        httpServer,
        httpSse,
        httpClient,
        wsServer,
        wsClient,
        plugins,
        total: httpServer + httpSse + httpClient + wsServer + wsClient + plugins,
    };
}

function channelDetailLabel(c: ChannelCount): string {
    const parts: string[] = [];
    if (c.httpServer || c.httpSse) parts.push(`HTTP ${c.httpServer + c.httpSse}`);
    if (c.httpClient) parts.push(`回调 ${c.httpClient}`);
    if (c.wsServer) parts.push(`WS ${c.wsServer}`);
    if (c.wsClient) parts.push(`反向 WS ${c.wsClient}`);
    if (c.plugins) parts.push(`插件 ${c.plugins}`);
    return parts.join(' · ');
}

function formatRestartHint(config: BotConfig): string | null {
    const sched = config.bot.autoRestartSchedule;
    const offline = config.bot.offlineAutoRestart;
    const parts: string[] = [];
    if (sched.enable) parts.push(`每 ${sched.duration}${formatTimeUnit(sched.time_unit)}`);
    if (offline) parts.push('离线时');
    if (parts.length === 0) return null;
    return parts.join(' · ');
}

function formatTimeUnit(unit: BotConfig['bot']['autoRestartSchedule']['time_unit']): string {
    switch (unit) {
        case 'm':
            return '分钟';
        case 'h':
            return '小时';
        case 'd':
            return '天';
        case 'mon':
            return '个月';
        case 'year':
            return '年';
        default:
            return String(unit);
    }
}

function formatRelativeTime(iso: string): string | null {
    const ts = Date.parse(iso);
    if (Number.isNaN(ts)) return null;
    const diffSec = Math.max(0, Math.floor((Date.now() - ts) / 1000));
    if (diffSec < 5) return '刚刚';
    if (diffSec < 60) return `${diffSec} 秒前`;
    const min = Math.floor(diffSec / 60);
    if (min < 60) return `${min} 分钟前`;
    const hr = Math.floor(min / 60);
    if (hr < 24) return `${hr} 小时前`;
    const day = Math.floor(hr / 24);
    if (day < 7) return `${day} 天前`;
    const week = Math.floor(day / 7);
    if (week < 4) return `${week} 周前`;
    const month = Math.floor(day / 30);
    if (month < 12) return `${month} 个月前`;
    const year = Math.floor(day / 365);
    return `${year} 年前`;
}
