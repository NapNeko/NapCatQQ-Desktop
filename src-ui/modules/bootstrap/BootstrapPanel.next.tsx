// Overview 页（next）。
//
// 信息架构沿用 legacy home_page，并按当前后端实情更新：
//   - 后端不止 NapCat，还有 SnowLuma（SL，暂无 logo，使用几何占位）
//   - mascot 衣服跟随主题色，半身从 Hero 卡顶端破圈而出（参考图风格）
//
//   主列 (col-span-7)  HelloCard / RemoteSummary / NoticeTimeline
//   副列 (col-span-5)  CoreCardsRow (NapCat + SnowLuma) / Occupancy(CPU) / Occupancy(RAM)
//
// 响应式：
//   ≥ 1100px  双列 7:5
//   < 1100px  单列堆叠
//
// 严守 frontend-layering：仅 import hooks / shared/ui / 自身 widgets，不碰 services / @tauri-apps。

import React from 'react';
import {
    AlertTriangle,
    BellRing,
    Cpu,
    HardDrive,
    type LucideIcon,
    MessageSquare,
    PowerOff,
    Server,
    Snowflake,
    ThumbsUp,
} from 'lucide-react';
import { Card } from '../../shared/ui';
import { Mascot } from '../../shared/components/next/Mascot';
import logoPng from '../../assets/logo.png';
import { useResourceMonitor } from '../../hooks/diagnostics/useResourceMonitor';
import { OccupancyChart } from './widgets/OccupancyChart';

export const BootstrapPanelNext: React.FC = () => {
    const resource = useResourceMonitor();

    return (
        // flex-1 撑满父；min-h-0 让 grid 高度被父限定，不再被内容撑大；
        // pt-8 给 mascot 破圈预留空间
        <div className="grid min-h-0 flex-1 grid-cols-12 gap-4 pt-8">
            {/* ─── 主列：内容区 ≥ 1100px 占 7 列；否则占满 12 列 ─── */}
            <div className="col-span-12 flex min-h-0 flex-col gap-4 [@media(min-width:1100px)]:col-span-7">
                <HelloCard />
                <RemoteSummaryCard />
                {/* NoticeTimeline 吸收主列剩余高度，内部可滚动 */}
                <NoticeTimelineCard className="min-h-0 flex-1" />
            </div>

            {/* ─── 副列：≥ 1100px 占 5 列；否则占满 12 列堆到主列下方 ─── */}
            <div className="col-span-12 flex min-h-0 flex-col gap-4 [@media(min-width:1100px)]:col-span-5">
                <CoreCardsRow />
                {/* CPU + RAM 平分副列剩余高度，等高 */}
                <OccupancyChart
                    title="CPU"
                    icon={Cpu}
                    history={resource.history}
                    dataKey="cpu"
                    valueText={`${resource.cpu}%`}
                    accentColor="var(--brand-500)"
                    className="min-h-0 flex-1"
                />
                <OccupancyChart
                    title="RAM"
                    icon={HardDrive}
                    history={resource.history}
                    dataKey="ram"
                    valueText={`${resource.ram}%`}
                    accentColor="var(--accent-500)"
                    className="min-h-0 flex-1"
                />
            </div>
        </div>
    );
};

// ─── HelloCard ───────────────────────────────────────────────────────────
//
// 设计点：
//   - mascot 衣服跟主题色（brand-500 主 / brand-700 深），通过 <Mascot> 组件运行时染色
//   - 卡 overflow-visible，mascot 头部从卡顶溢出 ~32px 制造"破圈"感（参考图风格）
//   - mascot 仅在 md+ 显示，避免窄窗口压文字

const HelloCard: React.FC = () => (
    <Card variant="hero" padding="lg" className="relative overflow-visible">
        <div className="max-w-[300px] pr-2 sm:pr-0">
            <h1 className="font-display text-[36px] font-extrabold leading-none text-brand">
                Hello !!
            </h1>
            <p className="mt-3 text-[14px] leading-relaxed text-text-secondary">
                欢迎回到主页 NapCatQQ Desktop 帮你高效管理多个 QQ 机器人实例。
            </p>
            <div className="mt-4 inline-flex items-center gap-2 text-[13.5px] text-brand">
                <ThumbsUp size={14} strokeWidth={2} className="shrink-0" />
                <span>如果你喜欢，请去 GitHub 给个 Star</span>
            </div>
        </div>

        {/* mascot：身高 200px，bottom-0 贴卡底，头从卡顶溢出 ~30px。
            主题色通过 Mascot 组件运行时替换 SVG 衣服色。
            < md 隐藏避免压文字。 */}
        <div className="pointer-events-none absolute -top-8 right-2 hidden md:block lg:right-6">
            <Mascot
                primaryColor="var(--brand-500)"
                secondaryColor="var(--brand-700)"
                className="h-[200px] w-[133px] drop-shadow-md [&>svg]:h-full [&>svg]:w-full"
            />
        </div>
    </Card>
);

// ─── RemoteSummary 卡 ────────────────────────────────────────────────────

const RemoteSummaryCard: React.FC = () => (
    <Card padding="md" hover="lift" className="cursor-pointer">
        <div className="flex items-center gap-3">
            <div className="grid h-10 w-10 shrink-0 place-items-center rounded-md bg-info/10 text-info">
                <Server size={18} strokeWidth={1.75} />
            </div>
            <div className="min-w-0 flex-1">
                <p className="text-[14px] font-semibold text-text">远端主机集群</p>
                <p className="mt-0.5 text-[12.5px] text-text-tertiary">
                    尚未配置远端主机，点击进入 Remote 页面添加。
                </p>
            </div>
            <span className="shrink-0 rounded-pill bg-inset px-2.5 py-0.5 text-[11.5px] font-medium text-text-secondary">
                0 hosts
            </span>
        </div>
    </Card>
);

// ─── NoticeTimeline 卡 ───────────────────────────────────────────────────

interface NoticeItem {
    id: string;
    icon: LucideIcon;
    iconBg: string;
    iconColor: string;
    title: string;
    detail: string;
    date: string;
    tone: 'info' | 'warning' | 'danger' | 'neutral';
}

const PLACEHOLDER_NOTICES: NoticeItem[] = [
    {
        id: 'n1',
        icon: BellRing,
        iconBg: 'bg-brand/10',
        iconColor: 'text-brand',
        title: 'NapCat 有新版本',
        detail: '最新版 v4.2.33 已发布，包含修复与性能改进。',
        date: '12-16',
        tone: 'info',
    },
    {
        id: 'n2',
        icon: PowerOff,
        iconBg: 'bg-warning/10',
        iconColor: 'text-warning',
        title: '机器人 永恒 已离线',
        detail: '账号被踢下线，已开始尝试自动重连。',
        date: '12-15',
        tone: 'warning',
    },
    {
        id: 'n3',
        icon: AlertTriangle,
        iconBg: 'bg-danger/10',
        iconColor: 'text-danger',
        title: '内存使用率达到 70%',
        detail: '建议优化资源以避免触发自愈重启。',
        date: '12-14',
        tone: 'danger',
    },
    {
        id: 'n4',
        icon: MessageSquare,
        iconBg: 'bg-accent-soft',
        iconColor: 'text-accent',
        title: '欢迎使用 NapCatQQ Desktop',
        detail: '你可以在 Bots 页面创建第一个机器人实例。',
        date: '12-13',
        tone: 'neutral',
    },
];

const dotToneClass: Record<NoticeItem['tone'], string> = {
    info: 'bg-info',
    warning: 'bg-warning',
    danger: 'bg-danger',
    neutral: 'bg-text-disabled',
};

const NoticeTimelineCard: React.FC<{ className?: string }> = ({ className }) => (
    <Card padding="md" className={`flex flex-col ${className ?? ''}`.trim()}>
        <div className="mb-3 flex shrink-0 items-center justify-between">
            <h3 className="font-display text-[15px] font-semibold text-text">
                Recent Notices
            </h3>
            <span className="text-[12px] text-text-tertiary">
                最近 {PLACEHOLDER_NOTICES.length} 条
            </span>
        </div>

        {/* min-h-0 + overflow 让 list 在 flex 父高度受限时内部滚动 */}
        <ol className="relative min-h-0 flex-1 space-y-2 overflow-y-auto pl-4 pr-1">
            <span
                aria-hidden
                className="absolute left-[5px] top-2 bottom-2 w-px bg-border-subtle"
            />

            {PLACEHOLDER_NOTICES.map((notice) => (
                <li key={notice.id} className="relative">
                    <span
                        aria-hidden
                        className={`absolute -left-4 top-3 h-2.5 w-2.5 rounded-full ring-2 ring-surface ${dotToneClass[notice.tone]}`}
                    />

                    <div className="flex items-start gap-3 rounded-sm bg-inset/50 px-3 py-2.5 transition-colors hover:bg-inset">
                        <div className={`grid h-9 w-9 shrink-0 place-items-center rounded-sm ${notice.iconBg}`}>
                            <notice.icon size={16} strokeWidth={1.75} className={notice.iconColor} />
                        </div>
                        <div className="min-w-0 flex-1">
                            <div className="flex items-baseline justify-between gap-2">
                                <p className="truncate text-[13.5px] font-semibold text-text">
                                    {notice.title}
                                </p>
                                <span className="shrink-0 font-mono text-[11px] text-text-tertiary tabular-nums">
                                    {notice.date}
                                </span>
                            </div>
                            <p className="mt-0.5 truncate text-[12.5px] text-text-tertiary">
                                {notice.detail}
                            </p>
                        </div>
                    </div>
                </li>
            ))}
        </ol>
    </Card>
);

// ─── Core 双卡：NapCat + SnowLuma ────────────────────────────────────────
//
// 后端现支持 NapCat 和 SnowLuma 两套 core。SnowLuma 暂无 logo，
// 用 lucide Snowflake 作占位（语义贴合"Snow"），等正式 logo 来直接换 src。

const CoreCardsRow: React.FC = () => (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <CoreCard
            kind="napcat"
            label="NapCat"
            version="V4.2.32"
        />
        <CoreCard
            kind="snowluma"
            label="SnowLuma"
            version="未安装"
        />
    </div>
);

interface CoreCardProps {
    kind: 'napcat' | 'snowluma';
    label: string;
    version: string;
}

const CoreCard: React.FC<CoreCardProps> = ({ kind, label, version }) => (
    <Card padding="md" className="flex items-center gap-3">
        <div
            className={`grid h-12 w-12 shrink-0 place-items-center rounded-md ${kind === 'napcat' ? 'bg-brand-soft' : 'bg-info-soft'
                }`}
        >
            {kind === 'napcat' ? (
                <img src={logoPng} alt="" className="h-7 w-7 select-none" draggable={false} />
            ) : (
                // SnowLuma 占位 logo：lucide Snowflake + info 色块
                <Snowflake size={22} strokeWidth={1.75} className="text-info" />
            )}
        </div>
        <div className="min-w-0 flex-1">
            <p className="text-[11px] uppercase tracking-wider text-text-tertiary">Core</p>
            <p className="mt-0.5 truncate text-[14px] font-semibold text-text">{label}</p>
            <p className="truncate font-mono text-[12px] text-text-tertiary tabular-nums">{version}</p>
        </div>
    </Card>
);

export default BootstrapPanelNext;
