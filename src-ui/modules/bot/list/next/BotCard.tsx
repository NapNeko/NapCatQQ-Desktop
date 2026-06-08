// 列表行式 BotCard（new tree）。
//
// 单卡内部布局：
//   1. Header 行：[复选框] [Avatar] [身份块（标题 + 徽章 / QQ ID · flavor · 时间 · 状态）] [操作区]
//   2. Chip 行：对外 / 自启 / 运行位置 / daemon / 注入 / UIN / 启动模式 / WebUI 端口 / rev
//   3. （可选）错误行 / 踢线 toast：仅高 priority 状态独立显示
//
// 卡片走内容自适应高度（不再固定 h-[120px]）：固定高度遇到只有一两个 chip 时
// 下方会出现大块留白看着稀疏。普通状态文案（"运行中" / "已登录"）合并到副标题
// 行，不单独占一整行；只有错误 / 被踢这类高 priority 的红色标签会撑出独立行。
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
    AlertTriangle,
    Check,
    Cpu,
    DoorOpen,
    FileText,
    Globe,
    Hash,
    LinkIcon,
    Play,
    Power,
    QrCode,
    RefreshCw,
    Settings,
    Square,
    UserCheck,
    Wifi,
    Zap,
} from 'lucide-react';
import gsap from 'gsap';
import {
    Badge,
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
    botStateBadge,
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
import { cn } from '../../../../shared/utils/cn';
import { pushInfoBar } from '../../../../hooks/ui/globalInfoBarStore';
import { QrCodeDialog } from './QrCodeDialog';

interface BotCardProps {
    bot: BotActorSnapshot;
    config?: BotConfig | null;
    flavor: Flavor | null;
    qrcodeUrl?: string | null;
    isOnline?: boolean | null;
    invalidationReason?: NapCatLoginInvalidationReason | null;
    napcatBinding?: NapcatWebuiBinding | null;
    snowlumaDaemonState?: DaemonState | null;
    snowlumaInjected?: boolean;
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
    snowlumaInjected,
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
}: BotCardProps) {
    const [qrOpen, setQrOpen] = useState(false);

    const [showKickedToast, setShowKickedToast] = useState(false);
    useEffect(() => {
        if (invalidationReason === 'kicked') {
            setShowKickedToast(true);
            // 自动重启场景下后端会很快触发 restart，状态会自然刷新，3s 足够；
            // 手动重启场景下 toast 是用户行动指引，给 15s 让用户来得及反应。
            // 真踢线 → 重启过程 isOnline 跳 true 后这个 toast 已经被父组件
            // 通过 invalidationReason=null 清掉，没必要担心残留。
            const dismissMs = config?.bot.offlineAutoRestart ? 3000 : 15000;
            const timer = setTimeout(() => setShowKickedToast(false), dismissMs);
            return () => clearTimeout(timer);
        }
        setShowKickedToast(false);
        return undefined;
    }, [invalidationReason, config?.bot.offlineAutoRestart]);

    const stateBadge = botStateBadge(bot.state);
    const stateBadgeTone = mapStateBadgeTone(stateBadge.color);
    const isSL = isSnowLumaFlavor(flavor);
    const hasQrcode = !!qrcodeUrl;

    // 状态切换反馈:
    //   - 关键状态转移(starting→running 等) → 状态徽章 pop,而不是整张卡 pop
    //     (大卡 pop 会跟 hover lift / shadow 叠加放大成"整张卡突然鼓一下")
    //   - last_error 首次出现 → 整张卡 shake(shake 是水平摇,不会跟 hover 冲突)
    const m = useMotion();
    const cardRef = useRef<HTMLDivElement | null>(null);
    const badgeRef = useRef<HTMLSpanElement | null>(null);
    const prevStateRef = useRef<typeof bot.state>(bot.state);
    const prevErrorRef = useRef<string | null | undefined>(bot.last_error);

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

    useEffect(() => {
        const el = cardRef.current;
        if (!el || !m.enabled) {
            prevErrorRef.current = bot.last_error;
            return;
        }
        const hadError = !!prevErrorRef.current;
        const hasError = !!bot.last_error;
        if (!hadError && hasError) {
            m.shake(el);
        }
        prevErrorRef.current = bot.last_error;
    }, [bot.last_error, m.enabled, m.level, m.speed]);

    const webuiAvailable = isWebuiAvailable({
        flavor,
        napcat: napcatBinding ?? null,
        snowlumaDaemonState: snowlumaDaemonState ?? null,
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

    const statusLine = computeStatusLine({
        bot,
        isSL,
        showKickedToast,
        offlineAutoRestart: !!config?.bot.offlineAutoRestart,
        snowlumaDaemonState,
        snowlumaLoginState,
        snowlumaInjected,
        snowlumaUin,
        isOnline,
        hasQrcode,
        webuiAvailable,
    });

    const isActive = isBotActive(bot.state);

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
    if (runtimeTarget) {
        chips.push(
            <InfoChip
                key="runtime"
                icon={Activity}
                label="运行"
                value={runtimeTarget === 'local' ? '本机' : runtimeTarget}
                muted={runtimeTarget === 'local'}
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
    // SnowLuma 运行时 chip 只在 bot active 时显示。stopped 后这些 domain event
    // 的值残留在 store 里不会主动清掉,但对用户没有参考价值——已经停了就别展示了。
    if (isSL && snowlumaDaemonState && isActive) {
        chips.push(
            <InfoChip
                key="daemon"
                icon={Cpu}
                label="daemon"
                value={daemonStateLabel(snowlumaDaemonState)}
                muted={snowlumaDaemonState !== 'ready'}
                tooltip="SnowLuma daemon 是全局单例，所有 SL Bot 共享"
            />,
        );
    }
    if (isSL && snowlumaInjected && isActive) {
        chips.push(
            <InfoChip
                key="injected"
                icon={Zap}
                iconMotion="pulse"
                label="注入"
                value="已就绪"
            />,
        );
    }
    if (isSL && snowlumaUin && isActive) {
        chips.push(
            <InfoChip
                key="uin"
                icon={UserCheck}
                label="UIN"
                value={snowlumaUin}
            />,
        );
    }
    if (napcatBinding?.port && isActive) {
        chips.push(
            <InfoChip
                key="webui"
                icon={Wifi}
                label="WebUI"
                value={`:${napcatBinding.port}`}
            />,
        );
    }
    if (bot.revision > 0) {
        chips.push(
            <InfoChip
                key="rev"
                icon={Hash}
                label="rev"
                value={`#${bot.revision}`}
                muted
                tooltip={`状态机第 ${bot.revision} 次状态变更 · token 代数 ${bot.token_generation}`}
            />,
        );
    }

    return (
        <>
            <div
                ref={cardRef}
                role={isBatchMode ? 'button' : undefined}
                onClick={handleRowClick}
                className={cn(
                    'group relative flex flex-col gap-2 rounded-md bg-elevated px-4 py-3',
                    'ring-1 ring-border-subtle shadow-card transition-all duration-150',
                    'hover:shadow-popover hover:bg-elevated/90',
                    isBatchMode && 'cursor-pointer',
                    isSelected && 'ring-2 ring-brand bg-brand-soft/30',
                )}
            >
                {/* Header 行：复选框 + 头像 + 身份块 + 操作区 */}
                <div className="flex items-center gap-3">
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

                    {/* 身份块（占满剩余空间，可压缩） */}
                    <div className="flex min-w-0 flex-1 flex-col">
                        <div className="flex min-w-0 items-center gap-2">
                            <h3
                                className="truncate font-display text-md font-semibold leading-tight text-text"
                                title={displayName}
                            >
                                {displayName}
                            </h3>
                            <Badge
                                ref={badgeRef}
                                tone={stateBadgeTone}
                                appearance="soft"
                                dot={bot.state === 'running'}
                            >
                                {stateBadge.label}
                            </Badge>
                            {bot.pending_restart && (
                                <Badge tone="warning" appearance="outline">
                                    待重启
                                </Badge>
                            )}
                        </div>
                        <div className="mt-0.5 flex min-w-0 items-center gap-1.5 text-2xs text-text-tertiary">
                            <span className="font-mono tabular-nums">
                                QQ {bot.bot_id}
                            </span>
                            {flavor && (
                                <>
                                    <span aria-hidden className="text-border">·</span>
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
                                    <span aria-hidden className="text-border">·</span>
                                    <span className="tabular-nums">{lastTransitionRel}</span>
                                </>
                            )}
                            {/* 状态文案：跟着副标题走，避免单独占一整行造成稀疏空白。
                                被踢 / 错误这类高 priority 事件也合并进这里（红 tone +
                                完整文本悬停 tooltip），不再单独占一行撑高卡片。 */}
                            {statusLine && (
                                <>
                                    <span aria-hidden className="text-border">·</span>
                                    <span
                                        className={cn(
                                            'inline-flex min-w-0 items-center gap-1 truncate',
                                            statusLineTextClass(statusLine.tone),
                                        )}
                                        title={
                                            // 错误类长文案截断后用原生 tooltip 露出全文，
                                            // 踢线 / 普通状态的文案本身就短，不需要 title。
                                            statusLine.tone === 'danger' && bot.last_error
                                                ? bot.last_error
                                                : undefined
                                        }
                                    >
                                        {statusLine.icon ? (
                                            <StatusLineIcon node={statusLine.icon} tone={statusLine.tone} />
                                        ) : null}
                                        <span className="truncate">{statusLine.text}</span>
                                    </span>
                                </>
                            )}
                        </div>
                    </div>

                    {/* 操作区。每个按钮用 GsapPresence(visible=...) 包,按钮按状态
                        进退场:scale 0.7 + autoAlpha 0/1。Play 和 Square 互斥(不会
                        同时存在),日志/WebUI 仅 running/starting 时存在,扫码仅
                        hasQrcode 时存在。 */}
                    {!isBatchMode && (
                        <div
                            className="flex shrink-0 items-center gap-1"
                            onClick={(e) => e.stopPropagation()}
                        >
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
                                    <ToolbarMotionIcon icon={QrCode} size={16} strokeWidth={2.2} hoverAccent />
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
                                    <ActionMotionIcon icon={Square} size={14} strokeWidth={2.6} motion="none" />
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
                                    <ToolbarMotionIcon icon={Play} size={14} strokeWidth={2.6} hoverAccent />
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
                                    <ToolbarMotionIcon icon={FileText} size={14} strokeWidth={2.2} hoverAccent />
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
                                    <ToolbarMotionIcon icon={Globe} size={14} strokeWidth={2.2} hoverAccent />
                                </IconButton>
                            </GsapPresence>
                            <IconButton
                                tooltip="配置"
                                onClick={stopAction(() => onConfigure(bot.bot_id))}
                            >
                                <ToolbarMotionIcon icon={Settings} size={14} strokeWidth={2.2} hoverAccent />
                            </IconButton>
                        </div>
                    )}
                </div>

                {/* Chip 行 */}
                {chips.length > 0 && (
                    <div className="flex flex-wrap items-center gap-1.5">{chips}</div>
                )}
            </div>

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
                    pushInfoBar({
                        tone: 'warning',
                        title: '账号已被踢',
                        // 文案区分恢复路径，与卡片内的踢线 toast 对齐。
                        content: config?.bot.offlineAutoRestart
                            ? `Bot ${bot.bot_id} 被踢，正在自动重启`
                            : `Bot ${bot.bot_id} 被踢，请手动重启`,
                        autoDismissMs: 5000,
                    });
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
        <div className="relative h-12 w-12 shrink-0">
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

// ============== Status line（替代 Callout） ==============

interface StatusLine {
    text: string;
    tone: 'success' | 'warning' | 'danger' | 'brand' | 'neutral';
    icon?: React.ReactNode;
}

function computeStatusLine(args: {
    bot: BotActorSnapshot;
    isSL: boolean;
    showKickedToast: boolean;
    /** 被踢后恢复路径文案的判断依据（开了自动重启就显示"自动重启中"，否则提示手动重启）。 */
    offlineAutoRestart: boolean;
    snowlumaDaemonState: DaemonState | null | undefined;
    snowlumaLoginState: SnowLumaLoginState | null | undefined;
    snowlumaInjected: boolean | undefined;
    snowlumaUin: string | null | undefined;
    isOnline: boolean | null | undefined;
    hasQrcode: boolean;
    webuiAvailable: boolean;
}): StatusLine | null {
    const {
        bot,
        isSL,
        showKickedToast,
        offlineAutoRestart,
        snowlumaDaemonState,
        snowlumaLoginState,
        snowlumaInjected,
        snowlumaUin,
        isOnline,
        hasQrcode,
        webuiAvailable,
    } = args;

    // 踢线提示视觉权重最高，先判断；保留 DoorOpen 图标做语义区分。
    if (showKickedToast) {
        return {
            text: offlineAutoRestart
                ? '账号被踢，自动重启中…'
                : '账号被踢，请手动重启',
            tone: 'danger',
            icon: <DoorOpen size={12} strokeWidth={2.4} />,
        };
    }

    // last_error 同样合并进副标题行；完整错误信息靠悬停 tooltip 看，避免独立
    // 一行红字撑高卡片。
    if (bot.last_error) {
        return {
            text: bot.last_error,
            tone: 'danger',
            icon: <AlertTriangle size={12} strokeWidth={2.4} />,
        };
    }

    // stopped 状态：徽章已经讲清楚，状态行就别再画蛇添足；并且后端事件链可能
    // 滞后清理 isOnline / loginState，这里短路掉避免出现"徽章说停了，状态行
    // 说在线"的矛盾画面。
    if (bot.state === 'stopped') return null;

    if (isSL) {
        if (snowlumaDaemonState === 'crashed') {
            return {
                text: 'daemon 已崩溃',
                tone: 'danger',
                icon: <AlertTriangle size={12} strokeWidth={2.4} />,
            };
        }
        if (snowlumaDaemonState === 'starting') {
            return { text: 'daemon 启动中', tone: 'brand' };
        }
        if (snowlumaLoginState === 'logged_in') {
            return {
                text: snowlumaUin ? `已登录 · ${snowlumaUin}` : '已登录',
                tone: 'success',
            };
        }
        if (snowlumaLoginState === 'waiting_for_qr_scan') {
            return { text: '等待扫码 — 打开 WebUI', tone: 'warning' };
        }
        if (snowlumaLoginState === 'starting') {
            return { text: '正在连接 QQ', tone: 'brand' };
        }
        if (snowlumaLoginState === 'disconnected') {
            return { text: '已断开 — 重启可恢复', tone: 'neutral' };
        }
        if (snowlumaInjected) {
            return { text: '已注入，等待登录', tone: 'neutral' };
        }
    } else {
        if (isOnline === true) {
            return { text: 'Bot 在线 · 接收 OneBot 事件', tone: 'success' };
        }
        if (hasQrcode) {
            return { text: '新二维码，点 QR 扫码', tone: 'warning' };
        }
        if (isOnline === false) {
            return { text: 'Bot 已离线', tone: 'neutral' };
        }
        if ((isOnline === null || isOnline === undefined) && webuiAvailable) {
            return { text: '等待登录态首次推送', tone: 'neutral' };
        }
    }

    if (bot.state === 'starting') return { text: '正在启动…', tone: 'brand' };
    if (bot.state === 'stopping') return { text: '正在停止…', tone: 'warning' };
    if (bot.state === 'repairing') return { text: '正在修复…', tone: 'warning' };
    if (bot.state === 'running') return { text: '运行中', tone: 'success' };
    if (bot.state === 'crashed') {
        return {
            text: 'Bot 已崩溃',
            tone: 'danger',
            icon: <AlertTriangle size={12} strokeWidth={2.4} />,
        };
    }
    return null;
}

function statusLineTextClass(tone: StatusLine['tone']): string {
    switch (tone) {
        case 'success':
            return 'text-success';
        case 'warning':
            return 'text-warning';
        case 'danger':
            return 'text-danger';
        case 'brand':
            return 'text-brand';
        case 'neutral':
        default:
            return 'text-text-secondary';
    }
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

function StatusLineIcon({
    node,
    tone,
}: {
    node: React.ReactNode;
    tone: StatusLine['tone'];
}) {
    if (tone !== 'danger') return <>{node}</>;
    const icon = statusLineIconComponent(node);
    if (!icon) return <>{node}</>;
    return (
        <MotionIcon
            icon={icon}
            motion="wiggle"
            size={12}
            strokeWidth={2.4}
            playEnter={false}
            className="shrink-0"
        />
    );
}

function statusLineIconComponent(node: React.ReactNode): ComponentType<LucideProps> | null {
    if (!node || typeof node !== 'object' || !('type' in node)) return null;
    const t = (node as React.ReactElement).type;
    if (typeof t !== 'function' && typeof t !== 'object') return null;
    const known: Record<string, ComponentType<LucideProps>> = {
        DoorOpen,
        AlertTriangle,
    };
    const name =
        typeof t === 'function'
            ? (t as { displayName?: string; name?: string }).displayName ??
              (t as { name?: string }).name
            : undefined;
    if (name && known[name]) return known[name];
    if (t === DoorOpen) return DoorOpen;
    if (t === AlertTriangle) return AlertTriangle;
    return null;
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
                'inline-flex items-center gap-1 rounded-pill border px-2 py-0.5 text-2xs',
                'border-border-subtle/80 bg-canvas/60',
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

/// IconButton enter/exit 工厂:scale 0.7→1 + autoAlpha,作用在外层包裹的 button 节点。
const iconBtnEnter: EnterFn = (el, env) =>
    gsap.fromTo(
        el,
        { autoAlpha: 0, scale: 0.6 },
        {
            autoAlpha: 1,
            scale: 1,
            duration: env.duration('fast'),
            ease: env.ease.release,
        },
    );
const iconBtnExit: ExitFn = (el, env) =>
    gsap.to(el, {
        autoAlpha: 0,
        scale: 0.6,
        duration: env.duration('fast') * 0.7,
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
            if (!el || !m.enabled || disabled) return;
            // IconButton 是密集型按钮,hover lift / shadow / brightness 都关,只动 scale。
            return m.bindHover(el, { lift: null, shadow: false, brightness: false });
        }, [m.enabled, m.level, m.speed, disabled]);

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

function mapStateBadgeTone(
    color: ReturnType<typeof botStateBadge>['color'],
): 'brand' | 'success' | 'warning' | 'danger' | 'neutral' | 'info' {
    switch (color) {
        case 'success':
            return 'success';
        case 'warning':
            return 'warning';
        case 'danger':
            return 'danger';
        case 'brand':
            return 'brand';
        case 'informative':
            return 'info';
        case 'severe':
        case 'important':
            return 'danger';
        case 'tiny':
        case 'subtle':
        case 'neutral':
        default:
            return 'neutral';
    }
}

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
    const httpServer = c.httpServers.filter((x) => x.enable).length;
    const httpSse = c.httpSseServers.filter((x) => x.enable).length;
    const httpClient = c.httpClients.filter((x) => x.enable).length;
    const wsServer = c.websocketServers.filter((x) => x.enable).length;
    const wsClient = c.websocketClients.filter((x) => x.enable).length;
    const plugins = c.plugins.length;
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

function daemonStateLabel(state: DaemonState): string {
    switch (state) {
        case 'ready':
            return '就绪';
        case 'starting':
            return '启动中';
        case 'stopping':
            return '停止中';
        case 'crashed':
            return '已崩溃';
        case 'stopped':
            return '已停止';
        default:
            return String(state);
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
