// docker pull 分层进度展示（任务详情 / 可复用）。

import { CheckCircle2, Loader2 } from 'lucide-react';
import { cn } from '../../shared/utils/cn';
import { Progress } from '../../shared/ui';
import type { ActionProgressView } from '../../core/domain/components/progress';

function layerBarPercent(layer: ActionProgressView['dockerLayers'][number]): number {
    if (layer.done) return 100;
    const p = layer.phase;
    if (p === '下载中' && layer.detail) return 55;
    if (p === '解压中') return 88;
    if (p === '校验') return 75;
    if (p === '等待' || p === '准备层') return 8;
    if (p === '下载完成') return 92;
    return 28;
}

export function DockerPullLayersPanel({
    progress,
    className,
}: {
    progress: ActionProgressView;
    className?: string;
}) {
    const layers = progress.dockerLayers;
    if (layers.length === 0) {
        if (progress.status === 'running' && progress.currentStep >= 2) {
            return (
                <p className={cn('mt-2 text-[11px] text-text-tertiary', className)}>
                    等待镜像层输出…（连接镜像站或解析 manifest 时可能稍久）
                </p>
            );
        }
        return null;
    }

    const doneCount = layers.filter((l) => l.done).length;

    return (
        <div className={cn('mt-2.5 space-y-2 border-t border-border-subtle/60 pt-2.5', className)}>
            <div className="flex items-center justify-between gap-2 text-[11px] text-text-tertiary">
                <span>镜像层</span>
                <span className="font-mono tabular-nums text-text-secondary">
                    {doneCount}/{layers.length} · {progress.percent}%
                </span>
            </div>
            <ul className="max-h-52 space-y-2 overflow-y-auto pr-0.5">
                {layers.map((layer) => {
                    const active = !layer.done && layer.phase !== '等待';
                    const bar = layerBarPercent(layer);
                    return (
                        <li
                            key={layer.id}
                            className={cn(
                                'rounded-md border px-2.5 py-2',
                                active
                                    ? 'border-brand/35 bg-brand-soft/20'
                                    : 'border-border-subtle/50 bg-inset/30',
                            )}
                        >
                            <div className="flex min-w-0 items-center gap-2">
                                {layer.done ? (
                                    <CheckCircle2
                                        size={12}
                                        className="shrink-0 text-success"
                                        aria-hidden
                                    />
                                ) : (
                                    <Loader2
                                        size={12}
                                        className="shrink-0 animate-spin text-brand"
                                        aria-hidden
                                    />
                                )}
                                <span
                                    className="shrink-0 font-mono text-[10.5px] tabular-nums text-text-tertiary"
                                    title={layer.id}
                                >
                                    {layer.id}
                                </span>
                                <span
                                    className={cn(
                                        'min-w-0 truncate text-[11px]',
                                        layer.done ? 'text-success' : 'text-text',
                                    )}
                                >
                                    {layer.phase}
                                </span>
                                {layer.detail ? (
                                    <span className="ml-auto min-w-0 max-w-[55%] truncate font-mono text-[10px] tabular-nums text-text-tertiary">
                                        {layer.detail}
                                    </span>
                                ) : null}
                            </div>
                            {!layer.done && (
                                <Progress
                                    size="sm"
                                    tone="brand"
                                    value={bar}
                                    className="mt-1.5 h-1"
                                />
                            )}
                        </li>
                    );
                })}
            </ul>
        </div>
    );
}