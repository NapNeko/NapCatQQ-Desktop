// NeoBot 页面共用的信息呈现与操作组件。

import { useState, type ReactNode } from 'react';
import type { UseQueryResult } from '@tanstack/react-query';
import { AlertCircle, CheckCircle2, Info, TriangleAlert } from 'lucide-react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from '../../../../shared/ui';
import { panelQueryState, type PanelState } from '../../../../core/domain/apps/neobotPanelState';
import { json, record, records, text } from '../../../../core/domain/apps/neobotWorkspace';
import { PanelStateView, PanelSectionState } from './PanelStateView';
import { PanelDisclosure } from './PanelDisclosure';
export { PanelTable, RecordTable, type PanelColumn } from './PanelTable';

export interface NeoBotPageProps {
    instanceId: string;
    onGoTab: (tab: string) => void;
}

export function ConfirmAction({
    label,
    description,
    onConfirm,
    disabled,
    variant = 'secondary',
}: {
    label: string;
    description: string;
    onConfirm: () => void;
    disabled?: boolean;
    variant?: 'secondary' | 'danger';
}) {
    const [open, setOpen] = useState(false);
    return (
        <>
            <Button size="sm" variant="secondary" disabled={disabled} onClick={() => setOpen(true)}>
                {label}
            </Button>
            <Dialog open={open} onOpenChange={setOpen}>
                <DialogContent>
                    <DialogHeader>
                        <DialogTitle>{label}</DialogTitle>
                        <DialogDescription>{description}</DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                        <Button variant="ghost" onClick={() => setOpen(false)}>
                            取消
                        </Button>
                        <Button
                            variant={variant === 'danger' ? 'danger' : 'primary'}
                            disabled={disabled}
                            onClick={() => {
                                setOpen(false);
                                onConfirm();
                            }}
                        >
                            {label}
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </>
    );
}

export function PanelPage<T>({
    query,
    onGoTab,
    notFoundHint,
    children,
}: {
    query: UseQueryResult<PanelState<T>, Error>;
    onGoTab: (tab: string) => void;
    notFoundHint?: string;
    children: (data: T) => ReactNode;
}) {
    return (
        <PanelStateView
            state={query.data}
            isError={query.isError}
            errorMessage={query.error?.message}
            onRetry={() => void query.refetch()}
            onGoTab={onGoTab}
            notFoundHint={notFoundHint}
        >
            {children}
        </PanelStateView>
    );
}

/** 一节卡片：标题行（可带状态与操作）+ 分隔线 + 正文。 */
export function PanelSection({
    title,
    meta,
    actions,
    children,
}: {
    title: ReactNode;
    /** 标题下的一行现状（多久没变、共几条），别在这里写解释 */
    meta?: ReactNode;
    actions?: ReactNode;
    children: ReactNode;
}) {
    return (
        <section className="min-w-0 rounded-md border border-border-subtle bg-surface">
            <header className="flex flex-wrap items-start justify-between gap-x-3 gap-y-2 px-4 pb-3 pt-3.5">
                <div className="min-w-0 flex-1">
                    <h3 className="text-[13px] font-semibold leading-5 text-text">{title}</h3>
                    {meta && <div className="mt-0.5 text-2xs text-text-tertiary">{meta}</div>}
                </div>
                {actions && (
                    <div className="flex shrink-0 flex-wrap items-center gap-1.5">{actions}</div>
                )}
            </header>
            <div className="border-t border-border-subtle px-4 py-3.5">{children}</div>
        </section>
    );
}

/** 小节自带一个面板请求：成功渲染正文，失败在正文位置给一行行内提示。 */
export function PanelQuerySection<T>({
    title,
    meta,
    actions,
    query,
    notFoundHint,
    children,
}: {
    title: ReactNode;
    meta?: ReactNode;
    actions?: ReactNode;
    query: UseQueryResult<PanelState<T>, Error>;
    notFoundHint?: string;
    children: (data: T) => ReactNode;
}) {
    const state = panelQueryState(query);
    return (
        <PanelSection title={title} meta={meta} actions={actions}>
            {state?.kind === 'ok' ? (
                children(state.data)
            ) : (
                <PanelSectionState
                    state={state}
                    onRetry={() => void query.refetch()}
                    notFoundHint={notFoundHint}
                />
            )}
        </PanelSection>
    );
}

/** 指标格：一排数字，等宽对齐；标签在上、值在下。 */
export function MetricGrid({ children, cols = 3 }: { children: ReactNode; cols?: 2 | 3 | 4 }) {
    return (
        <div
            className={
                cols === 2
                    ? 'grid grid-cols-2 gap-2'
                    : cols === 4
                      ? 'grid grid-cols-2 gap-2 sm:grid-cols-4'
                      : 'grid grid-cols-2 gap-2 sm:grid-cols-3'
            }
        >
            {children}
        </div>
    );
}

export function Metric({
    label,
    value,
    unit,
    tone = 'default',
    hint,
}: {
    label: string;
    value: ReactNode;
    unit?: string;
    tone?: 'default' | 'warn' | 'brand' | 'muted';
    hint?: string;
}) {
    return (
        <div className="min-w-0 rounded-sm border border-border-subtle bg-inset/40 px-3 py-2">
            <p className="truncate text-2xs text-text-tertiary" title={hint ?? label}>
                {label}
            </p>
            <p
                className={
                    'mt-1 truncate text-[15px] font-medium leading-5 tabular-nums ' +
                    (tone === 'warn'
                        ? 'text-warning'
                        : tone === 'brand'
                          ? 'text-brand'
                          : tone === 'muted'
                            ? 'text-text-tertiary'
                            : 'text-text')
                }
                title={typeof value === 'string' ? value : undefined}
            >
                {value}
                {unit && (
                    <span className="ml-0.5 text-2xs font-normal text-text-tertiary">{unit}</span>
                )}
            </p>
        </div>
    );
}

const NOTE_TONE = {
    info: { box: 'border-info/20 bg-info-soft/40', icon: 'text-info', Icon: Info },
    warning: {
        box: 'border-warning/20 bg-warning-soft/40',
        icon: 'text-warning',
        Icon: TriangleAlert,
    },
    danger: { box: 'border-danger/20 bg-danger-soft/40', icon: 'text-danger', Icon: AlertCircle },
    success: {
        box: 'border-success/20 bg-success-soft/40',
        icon: 'text-success',
        Icon: CheckCircle2,
    },
} as const;

/** 行内提示：一条现状 + 可选动作。不用整块卡片，也不该写成长段说明。 */
export function InlineNote({
    tone = 'info',
    children,
    action,
}: {
    tone?: keyof typeof NOTE_TONE;
    children: ReactNode;
    action?: ReactNode;
}) {
    const look = NOTE_TONE[tone];
    const Icon = look.Icon;
    return (
        <div
            className={`flex items-start gap-2 rounded-sm border px-3 py-2 ${look.box}`}
            role={tone === 'danger' ? 'alert' : undefined}
        >
            <Icon size={13} strokeWidth={2.1} className={`mt-0.5 shrink-0 ${look.icon}`} />
            <div className="min-w-0 flex-1 text-xs leading-relaxed text-text-secondary">
                {children}
            </div>
            {action && <div className="shrink-0">{action}</div>}
        </div>
    );
}

/** 空态：一句「还没有什么」+ 可选下一步。 */
export function PanelEmpty({ children, action }: { children: ReactNode; action?: ReactNode }) {
    return (
        <div className="flex flex-wrap items-center justify-between gap-2 rounded-sm border border-dashed border-border-subtle px-3 py-3">
            <p className="text-xs text-text-tertiary">{children}</p>
            {action}
        </div>
    );
}

export function FullText({ value, label }: { value: unknown; label?: string }) {
    return (
        <pre
            aria-label={label}
            className="max-h-[28rem] overflow-auto whitespace-pre-wrap break-words rounded-sm bg-inset p-3 font-mono-strict text-xs leading-relaxed text-text-secondary"
        >
            {typeof value === 'string' ? value : json(value)}
        </pre>
    );
}

const LABELS: Record<string, string> = {
    hostname: '主机',
    os: '系统',
    python_version: 'Python',
    pid: '进程',
    cpu_percent: 'CPU',
    cpu_count: '核心数',
    mem_percent: '内存',
    mem_used_mb: '已用内存',
    mem_total_mb: '总内存',
    process_memory_mb: '进程内存',
    process_threads: '线程数',
    disk_percent: '磁盘',
    disk_used_gb: '已用磁盘',
    disk_total_gb: '总磁盘',
    load_average: '系统负载',
    online: '在线',
    standby: '待机',
    available: '可用',
    state: '状态',
    phase: '阶段',
    reason: '原因',
    transition: '切换中',
    connect_onebot: '待机时保持 OneBot',
    since_text: '开始时间',
    uptime_seconds: '运行时长',
    today_messages: '今日消息',
    total_messages: '总消息',
    cost_cny: '金额',
    calls: '调用次数',
    input_tokens: '输入 Token',
    output_tokens: '输出 Token',
    latency_ms: '延迟',
    registered: '已注册',
    assigned: '已分配',
    provider: '供应商',
    model_name: '模型',
    model_ref: '引用名',
    nickname: '昵称',
    user_id: 'QQ',
    platform: '平台',
    bot_id: 'Bot',
    name: '名称',
    description: '说明',
};

/** 字段清单：标签右对齐到一条竖线上，长值换行不撑破卡片。 */
export function FieldList({
    data,
    columns = 1,
    labels = LABELS,
    hide = [],
    empty = '—',
}: {
    data: unknown;
    columns?: 1 | 2;
    labels?: Record<string, string>;
    hide?: string[];
    empty?: ReactNode;
}) {
    const obj = record(data);
    const entries = Object.entries(obj).filter(
        ([key, value]) =>
            !['ok', 'avatar_url'].includes(key) && !hide.includes(key) && value !== undefined,
    );
    if (!entries.length) {
        return <p className="text-xs text-text-tertiary">暂无数据</p>;
    }
    return (
        <dl
            className={
                columns === 2
                    ? 'grid grid-cols-1 gap-x-6 gap-y-1.5 sm:grid-cols-2'
                    : 'grid grid-cols-1 gap-y-1.5'
            }
        >
            {entries.map(([key, value]) => (
                <div
                    key={key}
                    className="grid min-w-0 grid-cols-[6.5rem_minmax(0,1fr)] items-baseline gap-3"
                >
                    <dt className="truncate text-2xs text-text-tertiary" title={labels[key] ?? key}>
                        {labels[key] ?? key}
                    </dt>
                    <dd className="min-w-0 break-words text-xs leading-relaxed text-text">
                        {typeof value === 'object' && value !== null ? (
                            <PanelDisclosure label="查看详情" triggerClassName="text-brand">
                                <FullText value={value} />
                            </PanelDisclosure>
                        ) : typeof value === 'boolean' ? (
                            value ? (
                                '是'
                            ) : (
                                '否'
                            )
                        ) : (
                            text(value) || empty
                        )}
                    </dd>
                </div>
            ))}
        </dl>
    );
}

/** 旧名保留：字段清单之前的调用点还在用 */
export function ObjectView({ data, columns }: { data: unknown; columns?: 1 | 2 }) {
    return <FieldList data={data} columns={columns} />;
}

export function Trend({ data, field, title }: { data: unknown; field: string; title: string }) {
    const points = records(record(data).series ?? record(data).points).slice(-500);
    const values = points.map((p) => Math.max(0, Number(p[field]) || 0));
    const max = Math.max(0, ...values);
    const coords = values.map(
        (n, i) =>
            `${(i / Math.max(1, values.length - 1)) * 600},${100 - (n / (max || 1)) * 86 - 4}`,
    );
    return (
        <div>
            <div className="mb-2 flex items-baseline justify-between gap-2 text-xs">
                <span className="text-text-secondary">{title}</span>
                <span className="tabular-nums text-2xs text-text-tertiary">峰值 {max}</span>
            </div>
            {points.length ? (
                <>
                    <svg
                        viewBox="0 0 600 104"
                        role="img"
                        aria-label={title}
                        className="h-24 w-full text-brand"
                        preserveAspectRatio="none"
                    >
                        <polygon
                            points={`0,100 ${coords.join(' ')} 600,100`}
                            fill="currentColor"
                            opacity="0.07"
                        />
                        <polyline
                            points={coords.join(' ')}
                            fill="none"
                            stroke="currentColor"
                            strokeWidth="1.6"
                            vectorEffect="non-scaling-stroke"
                            strokeLinejoin="round"
                        />
                    </svg>
                    <div className="mt-1 flex justify-between text-2xs text-text-tertiary">
                        <span>{text(points[0]?.at)}</span>
                        <span>{text(points.at(-1)?.at)}</span>
                    </div>
                </>
            ) : (
                <p className="text-xs text-text-tertiary">暂无采样</p>
            )}
        </div>
    );
}
