// BotCard 子件：头像 / 信息 chip / 底栏工具钮 / 配置摘要 helpers

import {
    forwardRef,
    useEffect,
    useRef,
    useState,
    type ComponentType,
} from 'react';
import type { LucideProps } from 'lucide-react';
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from '../../../../shared/ui';
import {
    ActionMotionIcon,
    MotionIcon,
    type MotionIconPreset,
} from '../../../../shared/ui/motion';
import { useMotion } from '../../../../hooks/preferences/useMotion';
import type { BotConfig } from '../../../../core/ipc/generated/domain/BotConfig';
import { cn } from '../../../../shared/utils/cn';

export function BotAvatar({
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

export function ToolbarMotionIcon({
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

export function InfoChip({
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
    /// 显隐状态：false 时不渲染（return null），完全移除 DOM。
    visible?: boolean;
}

// 底栏工具钮：visible=false 时完全不渲染，避免占位。
// elegant/standard 档用 CSS transition，rich 档保留 GSAP bindHover。
export const IconButton = forwardRefIcon();

function forwardRefIcon() {
    return forwardRef<HTMLButtonElement, IconButtonProps>(function IconButtonImpl(
        { tooltip, onClick, disabled, tone = 'neutral', children, visible = true },
        ref,
    ) {
        const m = useMotion();
        const localRef = useRef<HTMLButtonElement | null>(null);
        const setRef = (node: HTMLButtonElement | null) => {
            localRef.current = node;
            if (typeof ref === 'function') ref(node);
            else if (ref) (ref as React.MutableRefObject<HTMLButtonElement | null>).current = node;
        };

        // hover 弹性：rich 档用 GSAP bindHover，elegant/standard 档用 CSS transition。
        useEffect(() => {
            const el = localRef.current;
            if (!el || disabled || !visible || !m.enabled || m.level !== 'rich') return;
            return m.bindHover(el, { lift: null, shadow: false, brightness: false });
        }, [m.enabled, m.level, m.speed, m.bindHover, disabled, visible]);

        if (!visible) return null;

        return (
            <Tooltip>
                <TooltipTrigger asChild>
                    <button
                        ref={setRef}
                        type="button"
                        onClick={onClick}
                        disabled={disabled}
                        className={cn(
                            'inline-flex h-8 w-8 items-center justify-center rounded-xs',
                            'transition-all duration-150',
                            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                            'disabled:cursor-not-allowed disabled:opacity-40',
                            m.level !== 'rich' && 'hover:scale-[1.04]',
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

export interface ChannelCount {
    httpServer: number;
    httpSse: number;
    httpClient: number;
    wsServer: number;
    wsClient: number;
    plugins: number;
    total: number;
}

export function countEnabledChannels(config: BotConfig): ChannelCount {
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

export function channelDetailLabel(c: ChannelCount): string {
    const parts: string[] = [];
    if (c.httpServer || c.httpSse) parts.push(`HTTP ${c.httpServer + c.httpSse}`);
    if (c.httpClient) parts.push(`回调 ${c.httpClient}`);
    if (c.wsServer) parts.push(`WS ${c.wsServer}`);
    if (c.wsClient) parts.push(`反向 WS ${c.wsClient}`);
    if (c.plugins) parts.push(`插件 ${c.plugins}`);
    return parts.join(' · ');
}

export function formatRestartHint(config: BotConfig): string | null {
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

export function formatRelativeTime(iso: string): string | null {
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
