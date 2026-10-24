// 容器诊断信息与按需资源采样。
import { RefreshCw } from 'lucide-react';
import type { ReactNode } from 'react';
import {
    Badge,
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
    Spinner,
} from '../../shared/ui';
import { ActionMotionIcon, refreshMotion } from '../../shared/ui/motion';
import { useDockerContainer } from '../../hooks/docker/useDockerContainer';
import { compactPorts, containerStateBadge } from '../../core/domain/docker/status';
import { errorText } from '../../core/domain/errors';
import { cn } from '../../shared/utils/cn';

function dateText(value: string) {
    if (!value || value.startsWith('0001-')) return '—';
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function Section({ title, children }: { title: string; children: ReactNode }) {
    return (
        <section className="space-y-2">
            <h3 className="flex items-center gap-2 text-[11px] font-medium uppercase tracking-wider text-text-tertiary">
                <span className="h-3 w-0.5 rounded-full bg-brand/45" aria-hidden />
                {title}
            </h3>
            {children}
        </section>
    );
}

function InfoRow({ label, value, mono }: { label: string; value: ReactNode; mono?: boolean }) {
    return (
        <div className="grid min-w-0 grid-cols-[5.5rem_minmax(0,1fr)] gap-x-4 border-b border-border-subtle/50 py-2 last:border-b-0">
            <dt className="text-[12px] text-text-tertiary">{label}</dt>
            <dd
                className={cn(
                    'min-w-0 break-all text-[12.5px] text-text',
                    mono && 'font-mono text-[12px]',
                )}
            >
                {value}
            </dd>
        </div>
    );
}

function PillList({ items, empty }: { items: string[]; empty: string }) {
    if (items.length === 0) return <p className="text-xs text-text-tertiary">{empty}</p>;
    return (
        <div className="flex flex-wrap gap-1.5">
            {items.map((item) => (
                <span
                    key={item}
                    title={item}
                    className="rounded-xs bg-inset/60 px-2 py-1 font-mono text-2xs text-text-secondary"
                >
                    {item}
                </span>
            ))}
        </div>
    );
}

function ErrorNotice({
    title,
    message,
    onRetry,
}: {
    title: string;
    message: string;
    onRetry?: () => void;
}) {
    return (
        <div role="alert" className="rounded-md border border-danger/30 bg-danger-soft/40 p-3">
            <p className="text-sm font-medium text-danger">{title}</p>
            <p className="mt-1 break-all text-xs leading-relaxed text-text-secondary">{message}</p>
            {onRetry && (
                <Button size="sm" variant="secondary" className="mt-2" onClick={onRetry}>
                    重试
                </Button>
            )}
        </div>
    );
}

export function ContainerDetailsDialog({
    hostId,
    id,
    name,
    onClose,
}: {
    hostId: string;
    id: string;
    name: string;
    onClose: () => void;
}) {
    const { details, stats } = useDockerContainer(hostId, id);
    const data = details.data;
    const badge = data ? containerStateBadge(data.state) : null;
    const busy = details.isFetching || stats.isFetching;
    const refresh = () => {
        void details.refetch();
        if (data?.state === 'running') void stats.refetch();
    };
    return (
        <Dialog open onOpenChange={(open) => !open && onClose()}>
            <DialogContent size="sheet">
                <DialogHeader className="shrink-0 pr-8">
                    <DialogTitle className="flex min-w-0 items-center gap-2">
                        <span className="truncate">{name}</span>
                        {badge && (
                            <Badge tone={badge.tone} appearance="soft" className="shrink-0">
                                {badge.label}
                            </Badge>
                        )}
                    </DialogTitle>
                    <DialogDescription className="truncate font-mono text-xs">
                        {data?.image ?? `容器 ${id.slice(0, 12)}`}
                    </DialogDescription>
                </DialogHeader>

                <div className="flex shrink-0 items-center justify-between gap-3 border-b border-border-subtle/70 pb-2.5">
                    <span className="min-w-0 truncate text-xs text-text-tertiary">
                        {details.isLoading ? '正在读取…' : '详情每 15 秒自动刷新'}
                    </span>
                    <Button
                        size="sm"
                        variant="ghost"
                        disabled={busy}
                        onClick={refresh}
                        className="shrink-0"
                    >
                        <ActionMotionIcon icon={RefreshCw} size={13} motion={refreshMotion(busy)} />
                        刷新
                    </Button>
                </div>

                <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-0.5 py-4">
                    {details.isLoading ? (
                        <div className="flex min-h-[11rem] flex-col items-center justify-center gap-2 text-sm text-text-tertiary">
                            <Spinner size="md" />
                            正在读取容器详情…
                        </div>
                    ) : details.isError ? (
                        <ErrorNotice
                            title="读取容器详情失败"
                            message={errorText(details.error)}
                            onRetry={refresh}
                        />
                    ) : data ? (
                        <div className="space-y-5">
                            {data.error && (
                                <ErrorNotice title="容器状态异常" message={data.error} />
                            )}

                            <div className="grid gap-5 sm:grid-cols-2">
                                <Section title="基本信息">
                                    <dl className="rounded-md border border-border-subtle/60 bg-inset/25 px-3">
                                        <InfoRow label="镜像" value={data.image} mono />
                                        <InfoRow label="容器 ID" value={data.id} mono />
                                        <InfoRow label="创建时间" value={dateText(data.created)} />
                                        <InfoRow
                                            label="最近启动"
                                            value={dateText(data.startedAt)}
                                        />
                                        <InfoRow
                                            label="最近退出"
                                            value={dateText(data.finishedAt)}
                                        />
                                    </dl>
                                </Section>

                                <Section title="运行状态">
                                    <dl className="rounded-md border border-border-subtle/60 bg-inset/25 px-3">
                                        <InfoRow label="退出码" value={String(data.exitCode)} />
                                        <InfoRow
                                            label="内存不足终止"
                                            value={data.oomKilled ? '是' : '否'}
                                        />
                                        <InfoRow
                                            label="重启次数"
                                            value={String(data.restartCount)}
                                        />
                                        <InfoRow
                                            label="重启策略"
                                            value={data.restartPolicy || 'no'}
                                        />
                                        <InfoRow label="健康检查" value={data.health ?? '未配置'} />
                                        <InfoRow label="网络模式" value={data.networkMode} />
                                        <InfoRow
                                            label="网络"
                                            value={data.networks.join('、') || '—'}
                                        />
                                    </dl>
                                </Section>
                            </div>

                            <Section title="资源占用">
                                {data.state !== 'running' ? (
                                    <p className="text-xs text-text-tertiary">
                                        运行后可查看资源占用
                                    </p>
                                ) : stats.isError ? (
                                    <ErrorNotice
                                        title="读取资源占用失败"
                                        message={errorText(stats.error)}
                                    />
                                ) : stats.data ? (
                                    <dl className="grid grid-cols-2 gap-2 sm:grid-cols-3">
                                        {Object.entries({
                                            CPU: stats.data.cpuPercent,
                                            内存: stats.data.memoryUsage,
                                            内存比例: stats.data.memoryPercent || '—',
                                            网络读写: stats.data.networkIo,
                                            磁盘读写: stats.data.blockIo,
                                            进程及线程: stats.data.pids,
                                        }).map(([label, value]) => (
                                            <div
                                                key={label}
                                                className="rounded-md border border-border-subtle/60 bg-inset/25 px-3 py-2"
                                            >
                                                <dt className="text-2xs text-text-tertiary">
                                                    {label}
                                                </dt>
                                                <dd className="mt-1 break-all font-mono text-[13px] text-text">
                                                    {value}
                                                </dd>
                                            </div>
                                        ))}
                                    </dl>
                                ) : (
                                    <p className="flex items-center gap-2 text-xs text-text-tertiary">
                                        <Spinner size="xs" />
                                        正在采样…
                                    </p>
                                )}
                            </Section>

                            <Section title="端口">
                                <PillList items={compactPorts(data.ports)} empty="无端口映射" />
                            </Section>

                            <Section title="挂载">
                                {data.mounts.length === 0 ? (
                                    <p className="text-xs text-text-tertiary">无挂载</p>
                                ) : (
                                    <div className="space-y-2">
                                        {data.mounts.map((mount) => (
                                            <div
                                                key={`${mount.kind}:${mount.destination}`}
                                                className="rounded-md border border-border-subtle/60 bg-inset/25 px-3 py-2"
                                            >
                                                <p className="text-2xs text-text-tertiary">
                                                    {mount.kind} ·{' '}
                                                    {mount.readOnly ? '只读' : '读写'}
                                                </p>
                                                <p className="mt-1 break-all font-mono text-xs text-text-secondary">
                                                    {mount.source || '—'} → {mount.destination}
                                                </p>
                                            </div>
                                        ))}
                                    </div>
                                )}
                            </Section>
                        </div>
                    ) : null}
                </div>
            </DialogContent>
        </Dialog>
    );
}
