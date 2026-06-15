// QQ 系统依赖对话框：检测缺失依赖、一键安装、错误引导。
// 仅用于 Linux 远端主机；Windows 不展示。

import { useState } from 'react';
import { PackageCheck, PackagePlus, Check, X, Copy, Loader2 } from 'lucide-react';
import {
    Button,
    Badge,
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from '../../shared/ui';
import { componentService } from '../../core/services/component.service';
import type { QqDependencyReport } from '../../core/ipc/generated/qq/QqDependencyReport';
import type { InstallDependenciesResult } from '../../core/ipc/generated/qq/InstallDependenciesResult';
import type { PackageStatus } from '../../core/ipc/generated/qq/PackageStatus';
import type { FailedPackage } from '../../core/ipc/generated/qq/FailedPackage';

interface QqDependencyDialogProps {
    open: boolean;
    hostId: string;
    report: QqDependencyReport | null;
    onClose: () => void;
    onInstalled?: (result: InstallDependenciesResult) => void;
}

type Phase = 'review' | 'installing' | 'done' | 'error';

export function QqDependencyDialog({
    open,
    hostId,
    report,
    onClose,
    onInstalled,
}: QqDependencyDialogProps) {
    const [phase, setPhase] = useState<Phase>('review');
    const [result, setResult] = useState<InstallDependenciesResult | null>(null);
    const [errorMsg, setErrorMsg] = useState<string>('');

    const missing = report?.missing ?? [];
    const satisfied = report?.satisfied ?? [];
    const installCommand = report?.installCommand ?? null;

    const handleInstall = async () => {
        if (!report || missing.length === 0) return;
        setPhase('installing');
        try {
            const pkgs = missing.map((p: PackageStatus) => p.name);
            const res = await componentService.installQqDependencies(hostId, pkgs);
            setResult(res);
            setPhase(res.success ? 'done' : 'error');
            onInstalled?.(res);
        } catch (e) {
            setErrorMsg(String(e));
            setPhase('error');
        }
    };

    const handleCopy = () => {
        if (installCommand) {
            void navigator.clipboard.writeText(installCommand);
        }
    };

    const handleClose = () => {
        setPhase('review');
        setResult(null);
        setErrorMsg('');
        onClose();
    };

    return (
        <Dialog open={open} onOpenChange={(o: boolean) => { if (!o) handleClose(); }}>
            <DialogContent size="sheet">
                <DialogHeader>
                    <DialogTitle className="flex items-center gap-2">
                        <PackageCheck size={18} className="text-accent" />
                        QQ 系统依赖
                    </DialogTitle>
                    <DialogDescription>
                        {report?.distroInfo
                            ? `检测到 ${report.distroInfo.name} ${report.distroInfo.version}`
                            : '检测 QQ 运行所需的系统库'}
                    </DialogDescription>
                </DialogHeader>

                <div className="flex-1 overflow-y-auto space-y-4 py-3 scrollbar-hide">
                    <QqDependencyBody
                        phase={phase}
                        missing={missing}
                        satisfied={satisfied}
                        result={result}
                        errorMsg={errorMsg}
                        installCommand={installCommand}
                        onCopy={handleCopy}
                    />
                </div>

                <DialogFooter>
                    <QqDependencyFooter
                        phase={phase}
                        missingCount={missing.length}
                        onClose={handleClose}
                        onInstall={handleInstall}
                    />
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}

// ─── Body ────────────────────────────────────────────────────────────────────

function QqDependencyBody({
    phase,
    missing,
    satisfied,
    result,
    errorMsg,
    installCommand,
    onCopy,
}: {
    phase: Phase;
    missing: PackageStatus[];
    satisfied: PackageStatus[];
    result: InstallDependenciesResult | null;
    errorMsg: string;
    installCommand: string | null;
    onCopy: () => void;
}) {
    if (phase === 'installing') {
        return (
            <div className="flex flex-col items-center justify-center gap-4 py-12">
                <Loader2 size={32} className="animate-spin text-accent" />
                <p className="text-sm text-text-secondary">
                    正在安装 {missing.length} 个依赖包，请稍候...
                </p>
            </div>
        );
    }

    if (phase === 'done' && result) {
        return (
            <div className="space-y-4">
                <div className="flex items-center gap-2 p-3 rounded-lg bg-success/10">
                    <Check size={18} className="text-success" />
                    <span className="text-sm font-medium text-success">
                        成功安装 {result.installed.length} 个依赖包
                    </span>
                </div>
                {result.failed.length > 0 && (
                    <div className="space-y-2">
                        <p className="text-sm font-medium text-warning">
                            {result.failed.length} 个包安装失败：
                        </p>
                        {result.failed.map((f: FailedPackage) => (
                            <div key={f.name} className="p-2 rounded bg-surface-secondary text-xs">
                                <span className="font-medium">{f.name}</span>: {f.reason}
                            </div>
                        ))}
                    </div>
                )}
            </div>
        );
    }

    if (phase === 'error') {
        return (
            <div className="flex flex-col gap-3 p-4 rounded-lg bg-danger/10 border border-danger/20">
                <div className="flex items-center gap-2">
                    <X size={18} className="text-danger" />
                    <span className="text-sm font-medium text-danger">安装失败</span>
                </div>
                <p className="text-xs text-text-secondary">{errorMsg}</p>
            </div>
        );
    }

    return (
        <div className="space-y-4">
            {missing.length > 0 && (
                <div className="space-y-2">
                    <div className="flex items-center gap-2">
                        <PackagePlus size={16} className="text-warning" />
                        <span className="text-sm font-medium">
                            缺失 {missing.length} 个依赖
                        </span>
                        <Badge tone="warning" appearance="soft">{missing.length}</Badge>
                    </div>
                    <div className="flex flex-wrap gap-2">
                        {missing.map((p) => (
                            <Badge key={p.name} tone="neutral" appearance="soft">
                                {p.name}
                            </Badge>
                        ))}
                    </div>
                </div>
            )}

            {satisfied.length > 0 && (
                <div className="space-y-2">
                    <div className="flex items-center gap-2">
                        <Check size={16} className="text-success" />
                        <span className="text-sm font-medium">
                            已满足 {satisfied.length} 个依赖
                        </span>
                    </div>
                </div>
            )}

            {installCommand && missing.length > 0 && (
                <div className="space-y-2 p-3 rounded-lg bg-surface-secondary">
                    <p className="text-xs text-text-secondary">
                        也可手动执行安装命令：
                    </p>
                    <div className="flex items-center gap-2">
                        <code className="flex-1 text-xs bg-surface-tertiary p-2 rounded font-mono overflow-x-auto">
                            {installCommand}
                        </code>
                        <Button size="sm" variant="ghost" onClick={onCopy}>
                            <Copy size={14} />
                        </Button>
                    </div>
                </div>
            )}
        </div>
    );
}

// ─── Footer ──────────────────────────────────────────────────────────────────

function QqDependencyFooter({
    phase,
    missingCount,
    onClose,
    onInstall,
}: {
    phase: Phase;
    missingCount: number;
    onClose: () => void;
    onInstall: () => void;
}) {
    if (phase === 'installing') {
        return (
            <Button variant="ghost" size="sm" disabled>
                安装中...
            </Button>
        );
    }

    if (phase === 'done' || phase === 'error') {
        return (
            <Button variant="primary" size="sm" onClick={onClose}>
                关闭
            </Button>
        );
    }

    if (missingCount === 0) {
        return (
            <Button variant="primary" size="sm" onClick={onClose}>
                关闭
            </Button>
        );
    }

    return (
        <>
            <Button variant="ghost" size="sm" onClick={onClose}>
                取消
            </Button>
            <Button variant="primary" size="sm" onClick={onInstall}>
                自动安装 {missingCount} 个依赖
            </Button>
        </>
    );
}
