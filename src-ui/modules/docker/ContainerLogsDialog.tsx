// 日志查询绑定容器 ID，切换行数或主机后旧请求不能覆盖新结果。
import { useMemo, useState } from 'react';
import { RefreshCw } from 'lucide-react';
import {
    Button,
    Checkbox,
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    Select,
    Spinner,
} from '../../shared/ui';
import { ActionMotionIcon, refreshMotion } from '../../shared/ui/motion';
import { useDockerLogs } from '../../hooks/docker/useDockerContainer';
import { errorText } from '../../core/domain/errors';

const TAIL_OPTIONS = ['100', '400', '1000', '2000'];

export function ContainerLogsDialog({
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
    const [tail, setTail] = useState('400');
    const [autoRefresh, setAutoRefresh] = useState(false);
    const [search, setSearch] = useState('');
    const logs = useDockerLogs(hostId, id, Number(tail), autoRefresh);
    const lines = useMemo(() => {
        const raw = logs.data ?? '';
        if (!raw) return [];
        const needle = search.trim().toLowerCase();
        return raw.split('\n').filter((line) => !needle || line.toLowerCase().includes(needle));
    }, [logs.data, search]);
    const filtered = search.trim().length > 0;

    return (
        <Dialog open onOpenChange={(open) => !open && onClose()}>
            <DialogContent size="sheet">
                <DialogHeader className="shrink-0 pr-8">
                    <DialogTitle className="truncate">{name} · 运行日志</DialogTitle>
                </DialogHeader>

                <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-border-subtle/70 pb-2.5">
                    <Select
                        aria-label="最近行数"
                        id="docker-log-tail"
                        value={tail}
                        onValueChange={setTail}
                        className="w-28 shrink-0"
                        items={TAIL_OPTIONS.map((value) => ({
                            value,
                            label: `${value} 行`,
                        }))}
                    />
                    <input
                        aria-label="过滤日志"
                        placeholder="过滤日志"
                        value={search}
                        onChange={(event) => setSearch(event.target.value)}
                        className="min-w-0 flex-1 rounded-sm border border-border-subtle bg-field px-3 py-2 text-sm text-text"
                    />
                    <Checkbox
                        id="docker-log-auto"
                        label="自动刷新"
                        checked={autoRefresh}
                        onCheckedChange={setAutoRefresh}
                    />
                    <Button
                        size="sm"
                        variant="ghost"
                        disabled={logs.isFetching}
                        onClick={() => void logs.refetch()}
                    >
                        <ActionMotionIcon
                            icon={RefreshCw}
                            size={13}
                            motion={refreshMotion(logs.isFetching)}
                        />
                        刷新
                    </Button>
                </div>

                <div className="flex min-h-0 flex-1 flex-col gap-2 py-3">
                    {logs.isError && (
                        <p
                            role="alert"
                            className="shrink-0 rounded-md border border-danger/30 bg-danger-soft/40 px-3 py-2 text-xs text-text-secondary"
                        >
                            <span className="font-medium text-danger">读取日志失败</span> ·{' '}
                            {errorText(logs.error)}
                        </p>
                    )}
                    <div className="relative min-h-0 flex-1 overflow-hidden rounded-md border border-border-subtle/60 bg-canvas/70">
                        {logs.isLoading ? (
                            <div className="flex h-full min-h-[16rem] items-center justify-center gap-2 text-xs text-text-tertiary">
                                <Spinner size="sm" />
                                正在读取日志…
                            </div>
                        ) : (
                            <pre className="h-full min-h-[16rem] overflow-auto whitespace-pre-wrap break-all p-3 font-mono text-xs leading-relaxed text-text-secondary">
                                {lines.length > 0
                                    ? lines.join('\n')
                                    : filtered
                                      ? '没有匹配的日志'
                                      : '（暂无日志）'}
                            </pre>
                        )}
                    </div>
                    <div className="flex shrink-0 items-center justify-between gap-2 text-2xs text-text-tertiary">
                        <span>{logs.isLoading ? '…' : `显示 ${lines.length} 行`}</span>
                        <span>{autoRefresh ? '每 5 秒自动刷新' : '手动刷新'}</span>
                    </div>
                </div>
            </DialogContent>
        </Dialog>
    );
}
