import { useId, useState } from 'react';
import { ExpandChevron, ExpandPresence } from '../../shared/ui/motion';
import type { CacheCategory } from '../../core/ipc/types';
import { useCacheCleanup } from '../../hooks/preferences/useCacheCleanup';
import {
    Button,
    Checkbox,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from '../../shared/ui';

const choices: Record<CacheCategory, { label: string; description: string }> = {
    installers: {
        label: '安装包',
        description: 'QQ、VC++ 安装失败后保留的安装包。清理后重试安装需要重新下载。',
    },
    desktop_updates: {
        label: '旧更新包',
        description: 'Desktop 下载的更新安装包；保留最近 24 小时的文件和安装日志。',
    },
    download_residues: {
        label: '下载暂存与残留',
        description:
            '组件下载、传输和解压的暂存文件。历史系统临时下载仅清理满 7 天且所属进程已退出的文件。',
    },
};

function bytesLabel(bytes: bigint | number) {
    const value = Number(bytes);
    if (value < 1024) return `${value} B`;
    if (value < 1024 ** 2) return `${(value / 1024).toFixed(1)} KiB`;
    if (value < 1024 ** 3) return `${(value / 1024 ** 2).toFixed(1)} MiB`;
    return `${(value / 1024 ** 3).toFixed(2)} GiB`;
}

export function CacheCleanupDialog({
    open,
    onOpenChange,
}: {
    open: boolean;
    onOpenChange: (open: boolean) => void;
}) {
    const { scan, clean, busy, result, error } = useCacheCleanup(open);
    const [showWarnings, setShowWarnings] = useState(false);
    const warningsId = useId();
    const [selected, setSelected] = useState<CacheCategory[]>([
        'installers',
        'desktop_updates',
        'download_residues',
    ]);
    const groups = scan.data?.categories ?? [];
    const eligible = groups.filter((group) => selected.includes(group.category) && group.files > 0);
    const totalBytes = eligible.reduce((sum, group) => sum + Number(group.bytes), 0);
    const totalFiles = eligible.reduce((sum, group) => sum + group.files, 0);
    const blocked = busy || scan.isFetching || scan.isError || scan.data?.busy || totalFiles === 0;
    const warnings = [...groups.flatMap((group) => group.warnings), ...(result?.warnings ?? [])];
    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!busy) onOpenChange(next);
            }}
        >
            <DialogContent size="md" hideClose={busy}>
                <DialogHeader>
                    <DialogTitle>清理下载缓存</DialogTitle>
                    <DialogDescription>
                        选择要清理的类别。配置、聊天记录和另存的文件会保留。
                    </DialogDescription>
                </DialogHeader>
                <div className="space-y-3 py-2">
                    {scan.isFetching && (
                        <p role="status" className="text-sm text-text-tertiary">
                            正在扫描缓存…
                        </p>
                    )}
                    {scan.isError && (
                        <p role="alert" className="text-sm text-danger">
                            扫描失败：
                            {scan.error instanceof Error ? scan.error.message : String(scan.error)}
                        </p>
                    )}
                    {groups.map((group) => (
                        <div
                            key={group.category}
                            className="rounded-md border border-border-subtle p-3"
                        >
                            <Checkbox
                                id={`cache-${group.category}`}
                                checked={selected.includes(group.category)}
                                disabled={busy || scan.isFetching}
                                onCheckedChange={(checked) =>
                                    setSelected((previous) =>
                                        checked
                                            ? [...previous, group.category]
                                            : previous.filter((c) => c !== group.category),
                                    )
                                }
                                label={choices[group.category].label}
                                hint={choices[group.category].description}
                            />
                            <p className="mt-2 pl-5 text-[12px] text-text-tertiary">
                                可清理 {group.files} 个文件 · {bytesLabel(group.bytes)}
                                {group.skipped > 0 && ` · 保留或跳过 ${group.skipped} 项`}
                            </p>
                        </div>
                    ))}
                    {scan.data?.busy && (
                        <p role="status" className="text-sm text-warning">
                            安装或维护正在进行，完成后请重新扫描。
                        </p>
                    )}
                    {error && (
                        <p role="alert" className="text-sm text-danger">
                            清理失败：{error}
                        </p>
                    )}
                    {result && (
                        <p role="status" className="text-sm text-text">
                            已删除 {result.deleted_files} 个文件（{bytesLabel(result.deleted_bytes)}
                            ）； 跳过 {result.skipped_files} 项，失败 {result.failed_files} 项。
                        </p>
                    )}
                    {warnings.length > 0 && (
                        <div className="text-[12px] text-text-tertiary">
                            <button
                                type="button"
                                aria-expanded={showWarnings}
                                aria-controls={warningsId}
                                onClick={() => setShowWarnings((current) => !current)}
                                className="inline-flex items-center gap-1.5 rounded-sm text-left hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                            >
                                <ExpandChevron open={showWarnings} />
                                查看未清理项目的原因
                            </button>
                            <ExpandPresence visible={showWarnings}>
                                <ul
                                    id={warningsId}
                                    className="mt-2 max-h-32 space-y-1 overflow-auto break-all"
                                >
                                    {[...new Set(warnings)].map((warning) => (
                                        <li key={warning}>{warning}</li>
                                    ))}
                                </ul>
                            </ExpandPresence>
                        </div>
                    )}
                    {totalFiles > 0 && (
                        <p className="text-sm text-text-tertiary">
                            已选 {totalFiles} 个文件，预计可清理 {bytesLabel(totalBytes)}。
                        </p>
                    )}
                </div>
                <DialogFooter>
                    <Button
                        variant="ghost"
                        onClick={() => scan.refetch()}
                        disabled={busy || scan.isFetching}
                    >
                        重新扫描
                    </Button>
                    <Button variant="secondary" onClick={() => onOpenChange(false)} disabled={busy}>
                        关闭
                    </Button>
                    <Button
                        onClick={() => clean(eligible.map((group) => group.category))}
                        disabled={Boolean(blocked)}
                    >
                        {busy ? '清理中…' : '清理所选缓存'}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
