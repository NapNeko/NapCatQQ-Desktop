// 引导用「裁切预览」：复用真实卡壳 / Badge / 侧栏视觉，假数据只读展示。
// 不接业务 hook，避免引导弹窗拉真实 Bot/组件状态。

import type { ComponentType, ReactNode } from 'react';
import type { LucideProps } from 'lucide-react';
import {
    Bot,
    LayoutDashboard,
    ListTodo,
    Package,
    Play,
    QrCode,
    Server,
    Settings,
    Square,
} from 'lucide-react';
import { Badge, Button } from '../../ui';
import { cn } from '../../utils/cn';
import { BotManageCard } from '../../../modules/bot/list/next/BotManageCard';
import { ComponentManageCard } from '../../../modules/components/ComponentEntityCard';
import logoSidebar from '../../../assets/logo-32.png?inline';

function PreviewChrome({
    label,
    children,
    className,
    bodyClassName,
}: {
    label?: string;
    children: ReactNode;
    className?: string;
    bodyClassName?: string;
}) {
    return (
        <div
            className={cn(
                'relative overflow-hidden rounded-lg border border-border-subtle/80 bg-canvas/90',
                className,
            )}
        >
            <div
                aria-hidden
                className="pointer-events-none absolute inset-0 opacity-60"
                style={{
                    background:
                        'radial-gradient(70% 55% at 100% 0%, color-mix(in srgb, var(--brand-500) 10%, transparent), transparent 60%)',
                }}
            />
            {label ? (
                <div className="relative z-[1] flex items-center gap-2 border-b border-border-subtle/60 px-3 py-1.5">
                    <span className="flex gap-1" aria-hidden>
                        <span className="h-1.5 w-1.5 rounded-full bg-text-disabled/50" />
                        <span className="h-1.5 w-1.5 rounded-full bg-text-disabled/40" />
                        <span className="h-1.5 w-1.5 rounded-full bg-text-disabled/30" />
                    </span>
                    <span className="text-[10px] font-medium tracking-wide text-text-tertiary">
                        {label}
                    </span>
                </div>
            ) : null}
            <div className={cn('relative z-[1]', bodyClassName ?? 'p-3')}>{children}</div>
        </div>
    );
}

function MiniNavRow({
    icon: Icon,
    label,
    active,
}: {
    icon: ComponentType<LucideProps>;
    label: string;
    active?: boolean;
}) {
    return (
        <div
            className={cn(
                'relative flex items-center gap-2 rounded-sm px-2 py-1.5 text-[12px]',
                active ? 'bg-brand/12 font-medium text-brand' : 'text-text-secondary',
            )}
        >
            {active ? (
                <span
                    aria-hidden
                    className="absolute bottom-1.5 left-0 top-1.5 w-0.5 rounded-r-pill bg-brand"
                />
            ) : null}
            <Icon size={14} strokeWidth={1.75} className="shrink-0 opacity-90" />
            <span className="truncate">{label}</span>
        </div>
    );
}

/** 纯侧栏轨：不再拼半截主内容区 */
function SidebarRail({
    highlight = 'bots',
    className,
}: {
    highlight?: 'bots' | 'components';
    className?: string;
}) {
    return (
        <div
            className={cn(
                'flex w-[10.25rem] shrink-0 flex-col bg-sidebar',
                className,
            )}
        >
            <div className="flex items-center gap-1.5 px-2.5 py-2.5">
                <img
                    src={logoSidebar}
                    alt=""
                    width={18}
                    height={18}
                    className="h-[18px] w-[18px] object-contain"
                    draggable={false}
                />
                <span className="truncate font-display text-[11px] font-semibold text-text">
                    NapCatQQ
                </span>
            </div>
            <div className="mx-2 h-px bg-border-subtle/80" />
            <div className="space-y-0.5 px-1.5 py-2">
                <MiniNavRow icon={LayoutDashboard} label="概览" />
                <MiniNavRow icon={Bot} label="机器人" active={highlight === 'bots'} />
                <MiniNavRow
                    icon={Package}
                    label="组件"
                    active={highlight === 'components'}
                />
                <MiniNavRow icon={Server} label="远端" />
                <MiniNavRow icon={ListTodo} label="任务" />
                <MiniNavRow icon={Settings} label="设置" />
            </div>
        </div>
    );
}

function DemoBotCard({ variant }: { variant: 'running' | 'need-login' }) {
    const running = variant === 'running';
    return (
        <div className="pointer-events-none w-full min-w-0 select-none">
            <BotManageCard
                status={{
                    lifecycle: running
                        ? { label: '运行中', tone: 'success', dot: true }
                        : { label: '已停止', tone: 'neutral', dot: false },
                    session: running
                        ? { label: 'QQ 已登录', tone: 'success', dot: false }
                        : { label: '待扫码', tone: 'warning', dot: false },
                    alert: null,
                }}
                accent={running ? 'brand' : 'none'}
                compact
                header={
                    <div className="flex min-w-0 flex-1 items-start gap-2.5">
                        <div
                            className={cn(
                                'relative flex h-9 w-9 shrink-0 items-center justify-center rounded-full',
                                'bg-brand/15 font-display text-[12px] font-semibold text-brand',
                            )}
                        >
                            Q
                            {running ? (
                                <span
                                    aria-hidden
                                    className="absolute -bottom-0.5 -right-0.5 h-2.5 w-2.5 rounded-full border-2 border-surface bg-success"
                                />
                            ) : null}
                        </div>
                        <div className="min-w-0 flex-1">
                            <p className="truncate font-display text-[13px] font-semibold text-text">
                                示例 Bot
                            </p>
                            <p className="mt-0.5 truncate text-[11px] text-text-tertiary">
                                10001 · NapCat · 本机
                            </p>
                        </div>
                    </div>
                }
                meta={
                    <span className="text-text-secondary">
                        {running
                            ? '进程正常，账号在线'
                            : '保存后点启动，手机 QQ 扫码'}
                    </span>
                }
                chips={
                    <div className="flex flex-wrap gap-1">
                        <Badge tone="neutral" appearance="soft">
                            本机
                        </Badge>
                        <Badge tone="brand" appearance="soft">
                            NapCat
                        </Badge>
                    </div>
                }
                footerActions={
                    <div className="flex items-center gap-1">
                        <Button variant="ghost" size="icon" tabIndex={-1} aria-hidden>
                            {running ? (
                                <Square size={14} strokeWidth={2} />
                            ) : (
                                <Play size={14} strokeWidth={2} />
                            )}
                        </Button>
                        {!running ? (
                            <Button variant="ghost" size="icon" tabIndex={-1} aria-hidden>
                                <QrCode size={14} strokeWidth={2} />
                            </Button>
                        ) : null}
                    </div>
                }
            />
        </div>
    );
}

/** 欢迎：侧栏 + Bot 卡合成一块（高度随内容，便于右栏竖直居中） */
export function WelcomePreview() {
    return (
        <PreviewChrome label="主界面 · 机器人" bodyClassName="p-0">
            <div className="flex overflow-hidden">
                <SidebarRail
                    highlight="bots"
                    className="border-r border-border-subtle/80"
                />
                <div className="flex min-w-0 flex-1 flex-col gap-2.5 bg-canvas p-3">
                    <div className="flex items-center justify-between gap-2">
                        <span className="text-[12px] font-medium text-text">
                            机器人
                        </span>
                        <span className="text-[10px] text-text-tertiary">
                            列表里一张卡
                        </span>
                    </div>
                    <DemoBotCard variant="running" />
                    <p className="text-[11px] leading-snug text-text-tertiary">
                        启停、扫码、日志都在这种卡上操作。
                    </p>
                </div>
            </div>
        </PreviewChrome>
    );
}

/** 地图：侧栏 + 旁注 */
export function MapPreview() {
    return (
        <PreviewChrome label="侧栏" bodyClassName="p-3">
            <div className="flex items-stretch gap-3">
                <div className="overflow-hidden rounded-md border border-border-subtle shadow-card">
                    <SidebarRail highlight="components" />
                </div>
                <div className="flex min-w-0 flex-1 flex-col justify-center gap-2 py-1">
                    <MapHint active label="组件" text="装依赖" />
                    <MapHint label="机器人" text="建实例、扫码" />
                    <MapHint label="其它" text="用到再点" muted />
                </div>
            </div>
        </PreviewChrome>
    );
}

function MapHint({
    label,
    text,
    active,
    muted,
}: {
    label: string;
    text: string;
    active?: boolean;
    muted?: boolean;
}) {
    return (
        <div
            className={cn(
                'rounded-md border px-2.5 py-2',
                active
                    ? 'border-brand/35 bg-brand/[0.08]'
                    : muted
                        ? 'border-border-subtle/70 bg-inset/40'
                        : 'border-border-subtle bg-surface/80',
            )}
        >
            <p
                className={cn(
                    'text-[11px] font-semibold',
                    active ? 'text-brand' : 'text-text',
                )}
            >
                {label}
            </p>
            <p className="mt-0.5 text-[11px] leading-snug text-text-tertiary">{text}</p>
        </div>
    );
}

/** 路径：三列等高故事板 */
export function PathStoryPreview() {
    return (
        <div className="grid gap-2.5 sm:grid-cols-3">
            <StoryCol n={1} title="组件" hint="依赖">
                <div className="pointer-events-none select-none">
                    <ComponentManageCard
                        statusBadge={{ label: '未安装', tone: 'warning', dot: false }}
                        title="Node.js"
                        description="运行时依赖"
                        meta={
                            <span className="text-[11px] text-text-tertiary">本机</span>
                        }
                        footer={
                            <Button variant="primary" size="sm" tabIndex={-1} aria-hidden>
                                安装
                            </Button>
                        }
                    />
                </div>
            </StoryCol>
            <StoryCol n={2} title="建 Bot" hint="本机">
                <DemoBotCard variant="need-login" />
            </StoryCol>
            <StoryCol n={3} title="扫码" hint="登录">
                <DemoBotCard variant="running" />
            </StoryCol>
        </div>
    );
}

function StoryCol({
    n,
    title,
    hint,
    children,
}: {
    n: number;
    title: string;
    hint: string;
    children: ReactNode;
}) {
    return (
        <div className="flex min-w-0 flex-col rounded-lg border border-border-subtle/80 bg-canvas/60 p-2.5">
            <div className="mb-2 flex items-center gap-1.5">
                <span className="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-brand/15 text-[10px] font-bold text-brand">
                    {n}
                </span>
                <span className="text-[12px] font-semibold text-text">{title}</span>
                <span className="truncate text-[11px] text-text-tertiary">{hint}</span>
            </div>
            <div className="min-w-0 flex-1">{children}</div>
        </div>
    );
}

/** 对照：组件与 Bot 两卡等高，避免左右分栏高低不齐 */
export function TipsPreview() {
    return (
        <PreviewChrome label="组件与 Bot" bodyClassName="p-0">
            <div className="grid sm:grid-cols-2 sm:divide-x sm:divide-border-subtle/70">
                <div className="flex min-w-0 flex-col gap-2 border-b border-border-subtle/70 p-3 sm:border-b-0 sm:p-3.5">
                    <div className="flex items-center justify-between gap-2">
                        <span className="text-[11px] font-semibold text-warning">
                            组件未就绪
                        </span>
                        <span className="text-[10px] text-text-tertiary">组件页</span>
                    </div>
                    <div className="pointer-events-none min-w-0 flex-1 select-none">
                        <ComponentManageCard
                            statusBadge={{ label: '未安装', tone: 'warning', dot: false }}
                            title="NapCat"
                            description="协议端尚未安装"
                            meta={
                                <span className="text-[11px] text-text-tertiary">本机</span>
                            }
                            footer={
                                <Button
                                    variant="primary"
                                    size="sm"
                                    tabIndex={-1}
                                    aria-hidden
                                >
                                    安装
                                </Button>
                            }
                        />
                    </div>
                </div>
                <div className="flex min-w-0 flex-col gap-2 p-3 sm:p-3.5">
                    <div className="flex items-center justify-between gap-2">
                        <span className="text-[11px] font-semibold text-text-secondary">
                            Bot 可建，启动需依赖
                        </span>
                        <span className="text-[10px] text-text-tertiary">机器人页</span>
                    </div>
                    <div className="min-w-0 flex-1">
                        <DemoBotCard variant="need-login" />
                    </div>
                </div>
            </div>
        </PreviewChrome>
    );
}

/** 收尾：主路径是组件页，Bot 只作「装齐之后」 */
export function GoPreview() {
    return (
        <div className="grid gap-2.5 sm:grid-cols-[minmax(0,1.15fr)_minmax(0,0.85fr)]">
            <PreviewChrome label="组件页" bodyClassName="p-2.5">
                <div className="pointer-events-none select-none">
                    <ComponentManageCard
                        statusBadge={{ label: '未安装', tone: 'warning', dot: false }}
                        title="NapCat"
                        description="Node / QQ / 框架装在这里"
                        meta={
                            <span className="text-[11px] text-text-tertiary">本机</span>
                        }
                        footer={
                            <Button variant="primary" size="sm" tabIndex={-1} aria-hidden>
                                安装
                            </Button>
                        }
                        accent="brand"
                    />
                </div>
            </PreviewChrome>
            <PreviewChrome label="机器人页" bodyClassName="p-2.5">
                <div className="opacity-90">
                    <DemoBotCard variant="need-login" />
                </div>
            </PreviewChrome>
        </div>
    );
}

/** 兼容旧导出名（若有引用） */
export function SidebarPreview({
    highlight = 'bots',
}: {
    highlight?: 'bots' | 'components';
}) {
    return (
        <PreviewChrome label="侧栏" bodyClassName="p-0">
            <SidebarRail highlight={highlight} className="w-full max-w-[10.25rem]" />
        </PreviewChrome>
    );
}

export function BotCardPreview({
    variant = 'running',
}: {
    variant?: 'running' | 'need-login';
}) {
    return (
        <PreviewChrome label="机器人 · 列表卡" bodyClassName="p-2.5">
            <DemoBotCard variant={variant} />
        </PreviewChrome>
    );
}

export function ComponentCardsPreview() {
    return (
        <PreviewChrome label="组件 · 本机" bodyClassName="p-2.5">
            <div className="pointer-events-none grid select-none gap-2 sm:grid-cols-2">
                <ComponentManageCard
                    statusBadge={{ label: '已安装', tone: 'success', dot: true }}
                    title="NapCat"
                    description="协议端核心"
                    meta={
                        <span className="text-[11px] text-text-tertiary">v4.x</span>
                    }
                    footer={
                        <Badge tone="success" appearance="soft">
                            就绪
                        </Badge>
                    }
                    accent="brand"
                />
                <ComponentManageCard
                    statusBadge={{ label: '未安装', tone: 'warning', dot: false }}
                    title="Node.js"
                    description="运行时依赖"
                    meta={
                        <span className="text-[11px] text-text-tertiary">LTS</span>
                    }
                    footer={
                        <Button variant="primary" size="sm" tabIndex={-1} aria-hidden>
                            安装
                        </Button>
                    }
                />
            </div>
        </PreviewChrome>
    );
}
