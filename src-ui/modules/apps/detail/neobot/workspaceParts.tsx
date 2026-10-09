import { useState, type ReactNode } from 'react';
import type { UseQueryResult } from '@tanstack/react-query';
import type { PanelState } from '../../../../hooks/apps/useNeoBotPanel';
import {
    Button,
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from '../../../../shared/ui';
import {
    json,
    record,
    records,
    text,
    type PanelObject,
} from '../../../../core/domain/apps/neobotWorkspace';
import { PanelStateView } from './PanelStateView';

export interface NeoBotPageProps {
    instanceId: string;
    onGoTab: (tab: string) => void;
}

export function ConfirmAction({
    label,
    description,
    onConfirm,
    disabled,
}: {
    label: string;
    description: string;
    onConfirm: () => void;
    disabled?: boolean;
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
                            variant="primary"
                            disabled={disabled}
                            onClick={() => {
                                setOpen(false);
                                onConfirm();
                            }}
                        >
                            确认{label}
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
    children,
}: {
    query: UseQueryResult<PanelState<T>, Error>;
    onGoTab: (tab: string) => void;
    children: (data: T) => ReactNode;
}) {
    return (
        <PanelStateView
            state={query.data}
            isError={query.isError}
            errorMessage={query.error?.message}
            onRetry={() => void query.refetch()}
            onGoTab={onGoTab}
        >
            {children}
        </PanelStateView>
    );
}

export function PanelSection({
    title,
    actions,
    children,
}: {
    title: string;
    actions?: ReactNode;
    children: ReactNode;
}) {
    return (
        <section className="rounded-md border border-border-subtle bg-surface p-4">
            <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
                <h3 className="text-sm font-semibold text-text">{title}</h3>
                {actions && <div className="flex flex-wrap gap-2">{actions}</div>}
            </div>
            {children}
        </section>
    );
}

export function FullText({ value, label }: { value: unknown; label?: string }) {
    return (
        <pre
            aria-label={label}
            className="max-h-[32rem] overflow-auto whitespace-pre-wrap break-words rounded-sm bg-inset p-3 font-mono text-xs leading-relaxed text-text-secondary"
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
    cpu_percent: 'CPU %',
    cpu_count: 'CPU 核心',
    mem_percent: '内存 %',
    mem_used_mb: '已用内存 MB',
    mem_total_mb: '总内存 MB',
    process_memory_mb: '进程内存 MB',
    process_threads: '线程',
    disk_percent: '磁盘 %',
    disk_used_gb: '已用磁盘 GB',
    disk_total_gb: '总磁盘 GB',
    load_average: '系统负载',
    online: '在线',
    standby: '待机',
    state: '状态',
    phase: '阶段',
    reason: '原因',
    transition: '切换中',
    connect_onebot: '保持 OneBot 连接',
    since_text: '开始时间',
    uptime_seconds: '运行秒数',
    today_messages: '今日消息',
    total_messages: '总消息',
    cost_cny: '金额（元）',
    calls: '调用次数',
    input_tokens: '输入 Token',
    output_tokens: '输出 Token',
    latency_ms: '延迟 ms',
    registered: '已注册',
    assigned: '已分配',
    provider: '供应商',
    model_name: '模型',
    model_ref: '引用名',
    nickname: '昵称',
    user_id: 'QQ',
    platform: '平台',
};

export function ObjectView({ data }: { data: unknown }) {
    const obj = record(data);
    return (
        <dl className="grid grid-cols-[minmax(7rem,auto)_1fr] gap-x-4 gap-y-2 text-xs">
            {Object.entries(obj)
                .filter(([key]) => !['ok', 'avatar_url'].includes(key))
                .map(([key, value]) => (
                    <div key={key} className="contents">
                        <dt className="break-words text-text-tertiary">{LABELS[key] ?? key}</dt>
                        <dd className="min-w-0 break-words text-text">
                            {typeof value === 'object' && value !== null ? (
                                <details>
                                    <summary className="cursor-pointer text-brand">
                                        查看详情
                                    </summary>
                                    <FullText value={value} />
                                </details>
                            ) : typeof value === 'boolean' ? (
                                value ? (
                                    '是'
                                ) : (
                                    '否'
                                )
                            ) : (
                                text(value) || '—'
                            )}
                        </dd>
                    </div>
                ))}
        </dl>
    );
}

export function RecordTable({
    items,
    columns,
    onSelect,
}: {
    items: PanelObject[];
    columns: readonly (readonly [string, string])[];
    onSelect?: (item: PanelObject) => void;
}) {
    if (!items.length) return <p className="text-xs text-text-tertiary">暂无数据</p>;
    return (
        <div className="overflow-x-auto">
            <table className="w-full text-left text-xs">
                <thead>
                    <tr>
                        {columns.map(([key, label]) => (
                            <th
                                key={key}
                                className="border-b border-border-subtle p-2 font-medium text-text-tertiary"
                            >
                                {label}
                            </th>
                        ))}
                    </tr>
                </thead>
                <tbody>
                    {items.map((item, index) => (
                        <tr
                            key={text(item.id) || text(item.key) || text(item.task_id) || index}
                            className="border-b border-border-subtle last:border-0"
                        >
                            {columns.map(([key], col) => (
                                <td
                                    key={key}
                                    className="max-w-[24rem] break-words p-2 text-text-secondary"
                                >
                                    {col === 0 && onSelect ? (
                                        <Button
                                            size="sm"
                                            variant="ghost"
                                            onClick={() => onSelect(item)}
                                        >
                                            {text(item[key]) || '查看'}
                                        </Button>
                                    ) : typeof item[key] === 'object' ? (
                                        text(
                                            Array.isArray(item[key])
                                                ? (item[key] as unknown[]).map(text).join('、')
                                                : '',
                                        )
                                    ) : (
                                        text(item[key]) || '—'
                                    )}
                                </td>
                            ))}
                        </tr>
                    ))}
                </tbody>
            </table>
        </div>
    );
}

export function Trend({ data, field, title }: { data: unknown; field: string; title: string }) {
    const points = records(record(data).series ?? record(data).points).slice(-500);
    const values = points.map((p) => Math.max(0, Number(p[field]) || 0));
    const max = Math.max(0, ...values);
    const coords = values
        .map(
            (n, i) =>
                `${(i / Math.max(1, values.length - 1)) * 600},${100 - (n / (max || 1)) * 90}`,
        )
        .join(' ');
    return (
        <div>
            <div className="mb-2 flex justify-between text-xs text-text-secondary">
                <span>{title}</span>
                <span>峰值 {max}</span>
            </div>
            {points.length ? (
                <>
                    <svg
                        viewBox="0 0 600 110"
                        role="img"
                        aria-label={title}
                        className="h-28 w-full text-brand"
                        preserveAspectRatio="none"
                    >
                        <polyline
                            points={coords}
                            fill="none"
                            stroke="currentColor"
                            strokeWidth="2"
                            vectorEffect="non-scaling-stroke"
                        />
                    </svg>
                    <div className="flex justify-between text-2xs text-text-tertiary">
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
