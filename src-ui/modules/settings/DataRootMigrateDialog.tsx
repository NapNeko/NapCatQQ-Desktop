// 数据根整树迁移向导：说明 → 选空目录 → 预览路径/结构 → 确认迁移 → 进度 → 完成。

import { useCallback, useEffect, useState } from 'react';
import { dataRootMigrateService } from '../../core/services/data-root-migrate.service';
import type {
    DataRootMigratePreview,
    DataRootMigrateProgress,
    DataRootMigrateResult,
    DataRootTreeEntry,
} from '../../core/ipc/types';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Progress,
} from '../../shared/ui';
import { cn } from '../../shared/utils/cn';

type Phase = 'guide' | 'preview' | 'running' | 'done' | 'error';

export interface DataRootMigrateDialogProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    currentDataRoot: string;
}

function formatBytes(n: number | bigint | null | undefined): string {
    if (n == null) return '—';
    const v = typeof n === 'bigint' ? Number(n) : n;
    if (!Number.isFinite(v) || v < 0) return '—';
    if (v < 1024) return `${Math.round(v)} B`;
    if (v < 1024 * 1024) return `${(v / 1024).toFixed(1)} KB`;
    if (v < 1024 * 1024 * 1024) return `${(v / (1024 * 1024)).toFixed(1)} MB`;
    return `${(v / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function progressPercent(p: DataRootMigrateProgress | null): number {
    if (!p) return 0;
    const total = Number(p.bytes_total);
    const done = Number(p.bytes_done);
    if (!total || total <= 0) return p.phase === 'done' ? 100 : 8;
    return Math.min(100, Math.round((done / total) * 100));
}

function kindLabel(kind: string): string {
    if (kind === 'dir') return '目录';
    if (kind === 'file') return '文件';
    if (kind === 'skip') return '跳过';
    return kind;
}

function TreePreview({ entries }: { entries: DataRootTreeEntry[] }) {
    if (entries.length === 0) {
        return (
            <p className="text-[12px] text-text-tertiary">源目录为空或无法列出结构。</p>
        );
    }
    return (
        <div className="max-h-48 overflow-auto rounded-md border border-border-subtle bg-inset/30">
            <table className="w-full border-collapse text-left text-[12px]">
                <thead className="sticky top-0 bg-elevated/95 text-text-tertiary">
                    <tr className="border-b border-border-subtle">
                        <th className="px-2.5 py-1.5 font-medium">名称</th>
                        <th className="w-14 px-2 py-1.5 font-medium">类型</th>
                        <th className="w-20 px-2 py-1.5 text-right font-medium">大小</th>
                    </tr>
                </thead>
                <tbody>
                    {entries.map((e) => (
                        <tr
                            key={`${e.kind}:${e.name}`}
                            className={cn(
                                'border-b border-border-subtle/60 last:border-0',
                                e.kind === 'skip' && 'text-text-tertiary',
                            )}
                        >
                            <td className="px-2.5 py-1.5 font-mono">
                                {e.name}
                                {e.note ? (
                                    <span className="ml-1.5 font-sans text-[11px] text-text-tertiary">
                                        {e.note}
                                    </span>
                                ) : null}
                            </td>
                            <td className="px-2 py-1.5 text-text-tertiary">{kindLabel(e.kind)}</td>
                            <td className="px-2 py-1.5 text-right tabular-nums text-text-tertiary">
                                {formatBytes(e.bytes)}
                            </td>
                        </tr>
                    ))}
                </tbody>
            </table>
        </div>
    );
}

export function DataRootMigrateDialog({
    open,
    onOpenChange,
    currentDataRoot,
}: DataRootMigrateDialogProps) {
    const [phase, setPhase] = useState<Phase>('guide');
    const [target, setTarget] = useState<string | null>(null);
    const [preview, setPreview] = useState<DataRootMigratePreview | null>(null);
    const [progress, setProgress] = useState<DataRootMigrateProgress | null>(null);
    const [result, setResult] = useState<DataRootMigrateResult | null>(null);
    const [errorMsg, setErrorMsg] = useState<string | null>(null);
    const [picking, setPicking] = useState(false);

    const reset = useCallback(() => {
        setPhase('guide');
        setTarget(null);
        setPreview(null);
        setProgress(null);
        setResult(null);
        setErrorMsg(null);
        setPicking(false);
    }, []);

    useEffect(() => {
        if (!open) {
            reset();
        }
    }, [open, reset]);

    useEffect(() => {
        if (!open || phase !== 'running') return;
        let unlisten: (() => void) | undefined;
        void dataRootMigrateService.listenProgress((p) => setProgress(p)).then((u) => {
            unlisten = u;
        });
        return () => {
            unlisten?.();
        };
    }, [open, phase]);

    const handleOpenChange = (next: boolean) => {
        if (phase === 'running') return;
        if (!next) onOpenChange(false);
        else onOpenChange(true);
    };

    const pickTarget = async () => {
        setPicking(true);
        setErrorMsg(null);
        try {
            const path = await dataRootMigrateService.pickTargetDirectory();
            if (!path) return;
            setTarget(path);
            const p = await dataRootMigrateService.preview(path);
            setPreview(p);
            setPhase('preview');
            if (!p.ok) {
                setErrorMsg(p.blocking_reasons[0] ?? '预检未通过');
            }
        } catch (e) {
            setErrorMsg(e instanceof Error ? e.message : String(e));
            setPhase('error');
        } finally {
            setPicking(false);
        }
    };

    const startMigrate = async () => {
        if (!target || !preview?.ok) return;
        setPhase('running');
        setErrorMsg(null);
        setProgress(null);
        try {
            const r = await dataRootMigrateService.start(target);
            setResult(r);
            setPhase('done');
        } catch (e) {
            setErrorMsg(e instanceof Error ? e.message : String(e));
            setPhase('error');
        }
    };

    const cancelRunning = async () => {
        try {
            await dataRootMigrateService.cancel();
        } catch {
            // ignore
        }
    };

    return (
        <Dialog open={open} onOpenChange={handleOpenChange}>
            <DialogContent size="lg" className="max-w-xl">
                <DialogHeader>
                    <DialogTitle>迁移数据目录</DialogTitle>
                    <DialogDescription>
                        将当前数据根整树复制到新位置（配置、密钥、组件），成功后自动重启。
                        与「导出 ZIP」不同：ZIP 不含密钥与组件安装树。
                    </DialogDescription>
                </DialogHeader>

                <div className="space-y-3 text-[13px] text-text">
                    <div className="rounded-md border border-border-subtle bg-inset/40 px-3 py-2">
                        <div className="text-[11px] text-text-tertiary">当前数据根</div>
                        <div className="mt-0.5 break-all font-mono text-[12px]">{currentDataRoot}</div>
                    </div>

                    {phase === 'guide' ? (
                        <div className="space-y-2 text-text-secondary">
                            <p>步骤：</p>
                            <ol className="list-decimal space-y-1 pl-5">
                                <li>
                                    选择一个<strong>空文件夹</strong>
                                    作为新的数据根（路径本身即 data root，不会再拼产品名）。
                                </li>
                                <li>确认预览中的目标路径与将复制的文件结构。</li>
                                <li>确认后开始迁移；完成后应用会自动重启。</li>
                            </ol>
                            <p className="text-[12px] text-text-tertiary">
                                旧目录默认保留；全程无需管理员权限（用户级指针）。
                            </p>
                        </div>
                    ) : null}

                    {target ? (
                        <div className="rounded-md border border-border-subtle px-3 py-2">
                            <div className="text-[11px] text-text-tertiary">目标数据根</div>
                            <div className="mt-0.5 break-all font-mono text-[12px]">{target}</div>
                        </div>
                    ) : null}

                    {preview && (phase === 'preview' || phase === 'running' || phase === 'done') ? (
                        <div className="space-y-2">
                            <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1 text-text-secondary">
                                <span>
                                    预估复制{' '}
                                    <span className="font-medium text-text">
                                        {formatBytes(preview.bytes_estimate)}
                                    </span>
                                </span>
                                {preview.local_active_bots > 0 ? (
                                    <span className="text-text-tertiary">
                                        本机运行中 Bot {preview.local_active_bots} 个（将先停止）
                                    </span>
                                ) : null}
                            </div>

                            {phase === 'preview' ? (
                                <>
                                    <div className="text-[12px] font-medium text-text">
                                        将复制的结构
                                    </div>
                                    <TreePreview entries={preview.tree_entries ?? []} />
                                </>
                            ) : null}

                            {preview.blocking_reasons.length > 0 ? (
                                <ul className="list-disc space-y-1 pl-4 text-danger">
                                    {preview.blocking_reasons.map((r) => (
                                        <li key={r}>{r}</li>
                                    ))}
                                </ul>
                            ) : null}

                            {phase === 'preview' && preview.warnings.length > 0 ? (
                                <ul className="list-disc space-y-1 pl-4 text-[12px] text-text-tertiary">
                                    {preview.warnings.map((w) => (
                                        <li key={w}>{w}</li>
                                    ))}
                                </ul>
                            ) : null}
                        </div>
                    ) : null}

                    {phase === 'running' ? (
                        <div className="space-y-2">
                            <Progress value={progressPercent(progress)} />
                            <p className="text-text-secondary">
                                {progress?.message ??
                                    (progress?.current_rel
                                        ? `复制中：${progress.current_rel}`
                                        : '正在迁移…')}
                            </p>
                            <p className="text-[12px] text-text-tertiary">
                                {formatBytes(progress?.bytes_done ?? 0)} /{' '}
                                {formatBytes(progress?.bytes_total ?? preview?.bytes_estimate ?? 0)}
                            </p>
                        </div>
                    ) : null}

                    {phase === 'done' && result ? (
                        <div className="space-y-2 text-text-secondary">
                            <p>迁移完成。新数据根：</p>
                            <p className="break-all font-mono text-[12px] text-text">
                                {result.new_root}
                            </p>
                            <p>应用即将重启；若未自动重启，请手动退出后重新打开。</p>
                            {result.warnings.length > 0 ? (
                                <ul className="list-disc pl-4 text-text-tertiary">
                                    {result.warnings.map((w) => (
                                        <li key={w}>{w}</li>
                                    ))}
                                </ul>
                            ) : null}
                        </div>
                    ) : null}

                    {(phase === 'error' || (phase === 'preview' && errorMsg)) && errorMsg ? (
                        <p className="text-danger">{errorMsg}</p>
                    ) : null}
                </div>

                <DialogFooter className="gap-2">
                    {phase === 'running' ? (
                        <Button variant="secondary" size="sm" onClick={() => void cancelRunning()}>
                            取消复制
                        </Button>
                    ) : (
                        <Button
                            variant="secondary"
                            size="sm"
                            onClick={() => handleOpenChange(false)}
                        >
                            {phase === 'done' ? '关闭' : '取消'}
                        </Button>
                    )}

                    {phase === 'guide' || phase === 'error' ? (
                        <Button size="sm" disabled={picking} onClick={() => void pickTarget()}>
                            {picking ? '选择中…' : '选择目标文件夹'}
                        </Button>
                    ) : null}

                    {phase === 'preview' ? (
                        <>
                            <Button
                                variant="secondary"
                                size="sm"
                                disabled={picking}
                                onClick={() => void pickTarget()}
                            >
                                重选文件夹
                            </Button>
                            <Button
                                size="sm"
                                disabled={!preview?.ok || picking}
                                onClick={() => void startMigrate()}
                            >
                                确认并开始迁移
                            </Button>
                        </>
                    ) : null}
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
