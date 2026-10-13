// 容器诊断信息与按需资源采样。
import { Dialog, DialogContent, DialogHeader, DialogTitle, Button, Badge } from '../../shared/ui';
import { useDockerContainer } from '../../hooks/docker/useDockerContainer';
import { containerStateBadge } from '../../core/domain/docker/status';
import { errorText } from '../../core/domain/errors';

function dateText(value: string) {
    if (!value || value.startsWith('0001-')) return '—';
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
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
    return (
        <Dialog open onOpenChange={(open) => !open && onClose()}>
            <DialogContent size="xl">
                <DialogHeader>
                    <DialogTitle>{name} · 容器详情</DialogTitle>
                </DialogHeader>
                <div className="flex items-center justify-between gap-3">
                    {badge && <Badge tone={badge.tone}>{badge.label}</Badge>}
                    <Button
                        size="sm"
                        variant="secondary"
                        disabled={details.isFetching || stats.isFetching}
                        onClick={() => {
                            void details.refetch();
                            if (data?.state === 'running') void stats.refetch();
                        }}
                    >
                        刷新
                    </Button>
                </div>
                {details.isError && (
                    <p role="alert" className="break-all text-sm text-danger">
                        {errorText(details.error)}
                    </p>
                )}
                {details.isLoading && <p className="text-sm text-text-tertiary">加载中…</p>}
                {data && (
                    <div className="max-h-[65vh] space-y-5 overflow-y-auto">
                        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
                            {Object.entries({
                                ID: data.id,
                                镜像: data.image,
                                创建时间: dateText(data.created),
                                最近启动: dateText(data.startedAt),
                                最近退出: dateText(data.finishedAt),
                                退出码: String(data.exitCode),
                                内存不足终止: data.oomKilled ? '是' : '否',
                                重启次数: String(data.restartCount),
                                重启策略: data.restartPolicy || 'no',
                                健康检查: data.health ?? '未配置',
                                网络模式: data.networkMode,
                                网络: data.networks.join('、') || '—',
                            }).map(([label, value]) => (
                                <div key={label} className="contents">
                                    <dt className="text-text-tertiary">{label}</dt>
                                    <dd className="min-w-0 break-all text-text">{value}</dd>
                                </div>
                            ))}
                        </dl>
                        {data.error && (
                            <p className="break-all text-sm text-danger">{data.error}</p>
                        )}
                        <section className="space-y-2">
                            <h3 className="text-sm font-medium text-text">资源占用</h3>
                            {data.state !== 'running' ? (
                                <p className="text-xs text-text-tertiary">运行后可查看资源占用</p>
                            ) : stats.isError ? (
                                <p role="alert" className="break-all text-xs text-danger">
                                    {errorText(stats.error)}
                                </p>
                            ) : stats.data ? (
                                <dl className="grid grid-cols-2 gap-3 sm:grid-cols-3">
                                    {Object.entries({
                                        CPU: stats.data.cpuPercent,
                                        内存: stats.data.memoryUsage,
                                        内存比例: stats.data.memoryPercent || '—',
                                        网络读写: stats.data.networkIo,
                                        磁盘读写: stats.data.blockIo,
                                        进程及线程: stats.data.pids,
                                    }).map(([label, value]) => (
                                        <div key={label} className="rounded-md bg-inset/40 p-3">
                                            <dt className="text-xs text-text-tertiary">{label}</dt>
                                            <dd className="mt-1 break-all font-mono text-sm text-text">
                                                {value}
                                            </dd>
                                        </div>
                                    ))}
                                </dl>
                            ) : (
                                <p className="text-xs text-text-tertiary">采样中…</p>
                            )}
                        </section>
                        <section className="space-y-2">
                            <h3 className="text-sm font-medium text-text">端口</h3>
                            <pre className="whitespace-pre-wrap break-all text-xs text-text-secondary">
                                {data.ports.join('\n') || '无端口映射'}
                            </pre>
                        </section>
                        <section className="space-y-2">
                            <h3 className="text-sm font-medium text-text">挂载</h3>
                            {data.mounts.length === 0 && (
                                <p className="text-xs text-text-tertiary">无挂载</p>
                            )}
                            {data.mounts.map((mount) => (
                                <div
                                    key={`${mount.kind}:${mount.destination}`}
                                    className="rounded-md bg-inset/40 p-3 text-xs"
                                >
                                    <p className="text-text-secondary">
                                        {mount.kind} · {mount.readOnly ? '只读' : '读写'}
                                    </p>
                                    <p className="mt-1 break-all font-mono text-text">
                                        {mount.source || '—'} → {mount.destination}
                                    </p>
                                </div>
                            ))}
                        </section>
                    </div>
                )}
            </DialogContent>
        </Dialog>
    );
}
