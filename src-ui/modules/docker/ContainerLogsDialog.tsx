// 容器日志对话框：打开时拉一次最近 400 行，可手动刷新。
// fetchLogs 是 useDocker 暴露的命令式方法（不走 react-query 缓存，按需取）。

import React, { useCallback, useEffect, useState } from 'react';
import { RefreshCw } from 'lucide-react';
import { ActionMotionIcon, refreshMotion } from '../../shared/ui/motion';
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    Button,
} from '../../shared/ui';

interface ContainerLogsDialogProps {
    name: string;
    fetchLogs: (name: string, tail?: number) => Promise<string>;
    onClose: () => void;
}

export const ContainerLogsDialog: React.FC<ContainerLogsDialogProps> = ({
    name,
    fetchLogs,
    onClose,
}) => {
    const [logs, setLogs] = useState('');
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const load = useCallback(async () => {
        setLoading(true);
        setError(null);
        try {
            const text = await fetchLogs(name, 400);
            setLogs(text || '（暂无日志）');
        } catch (e) {
            setError(e instanceof Error ? e.message : String(e));
        } finally {
            setLoading(false);
        }
    }, [fetchLogs, name]);

    useEffect(() => {
        load();
    }, [load]);

    return (
        <Dialog open onOpenChange={(o) => !o && onClose()}>
            <DialogContent className="max-w-3xl">
                <DialogHeader>
                    <div className="flex items-center justify-between pr-6">
                        <DialogTitle>{name} · 日志</DialogTitle>
                        <Button
                            size="sm"
                            variant="ghost"
                            onClick={load}
                            disabled={loading}
                        >
                            <ActionMotionIcon
                                icon={RefreshCw}
                                size={13}
                                motion={refreshMotion(loading)}
                            />
                            刷新
                        </Button>
                    </div>
                </DialogHeader>

                {error ? (
                    <p className="text-sm text-danger">取日志失败：{error}</p>
                ) : loading && !logs ? (
                    <div className="flex items-center gap-2 py-10 text-text-tertiary">
                        <ActionMotionIcon icon={RefreshCw} size={16} motion="spin" />
                        <span className="text-sm">加载中…</span>
                    </div>
                ) : (
                    <pre className="max-h-[60vh] overflow-auto rounded-md bg-canvas/70 p-3 font-mono text-2xs leading-relaxed text-text-secondary">
                        {logs}
                    </pre>
                )}
            </DialogContent>
        </Dialog>
    );
};
