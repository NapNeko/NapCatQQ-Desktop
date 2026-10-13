// 日志查询绑定容器 ID，切换行数或主机后旧请求不能覆盖新结果。
import { useMemo, useState } from 'react';
import { RefreshCw } from 'lucide-react';
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    Button,
    Checkbox,
    Select,
} from '../../shared/ui';
import { ActionMotionIcon, refreshMotion } from '../../shared/ui/motion';
import { useDockerLogs } from '../../hooks/docker/useDockerContainer';
import { errorText } from '../../core/domain/errors';

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
    const text = useMemo(
        () =>
            (logs.data ?? '')
                .split('\n')
                .filter((line) => line.toLowerCase().includes(search.toLowerCase()))
                .join('\n'),
        [logs.data, search],
    );
    return (
        <Dialog open onOpenChange={(open) => !open && onClose()}>
            <DialogContent size="xl">
                <DialogHeader>
                    <DialogTitle>{name} · 容器运行日志</DialogTitle>
                </DialogHeader>
                <div className="flex flex-wrap items-center gap-3">
                    <Select
                        label="最近行数"
                        id="docker-log-tail"
                        value={tail}
                        onValueChange={setTail}
                        items={['100', '400', '1000', '2000'].map((value) => ({
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
                        variant="secondary"
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
                {logs.isError && (
                    <p role="alert" className="break-all text-sm text-danger">
                        {errorText(logs.error)}
                    </p>
                )}
                <pre className="max-h-[60vh] overflow-auto whitespace-pre-wrap break-all rounded-md bg-canvas/70 p-3 font-mono text-xs leading-relaxed text-text-secondary">
                    {logs.isLoading
                        ? '加载中…'
                        : text || (search ? '没有匹配的日志' : '（暂无日志）')}
                </pre>
            </DialogContent>
        </Dialog>
    );
}
