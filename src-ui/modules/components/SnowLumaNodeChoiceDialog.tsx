import React, { useEffect, useState } from 'react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from '../../shared/ui';
import { cn } from '../../shared/utils/cn';
import type { NodeEnvironmentCandidate } from '../../core/ipc/types';

interface Props {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    candidates: NodeEnvironmentCandidate[];
    onConfirm: (selectedPath: string | null) => void;
}

export const SnowLumaNodeChoiceDialog: React.FC<Props> = ({
    open,
    onOpenChange,
    candidates,
    onConfirm,
}) => {
    // 'component' 表示安装独立组件；'reuse' 表示复用已有环境
    const [mode, setMode] = useState<'component' | 'reuse'>('component');
    const validCandidates = candidates.filter((c) => c.isValid);
    const [selectedPath, setSelectedPath] = useState<string>('');

    useEffect(() => {
        if (open) {
            setMode('component');
            if (validCandidates.length > 0) {
                setSelectedPath(validCandidates[0].path);
            }
        }
    }, [open, validCandidates.length]);

    const handleConfirm = () => {
        if (mode === 'component') {
            onConfirm(null);
        } else {
            onConfirm(selectedPath || null);
        }
        onOpenChange(false);
    };

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent size="md" dismissOnOutsideClick={false}>
                <DialogHeader>
                    <DialogTitle>选择 Node.js 运行环境</DialogTitle>
                    <DialogDescription>
                        Lite 版本需要 Node.js（^22.13.0 || &gt;=23.4.0）运行。
                    </DialogDescription>
                </DialogHeader>
                <div className="flex flex-col gap-2.5">
                    {/* 选项 1：自动安装 Node.js */}
                    <button
                        type="button"
                        onClick={() => setMode('component')}
                        className={cn(
                            'rounded-sm border px-3 py-2.5 text-left transition-colors',
                            mode === 'component'
                                ? 'border-brand bg-brand-soft ring-1 ring-brand/35'
                                : 'border-border-subtle bg-inset hover:border-border',
                        )}
                    >
                        <div className="text-sm font-medium text-text">自动安装 Node.js（默认）</div>
                    </button>

                    {/* 选项 2：复用本机已检测到的 Node.js 环境 */}
                    {validCandidates.length > 0 && (
                        <div
                            onClick={() => setMode('reuse')}
                            className={cn(
                                'rounded-sm border px-3 py-2.5 text-left transition-colors cursor-pointer',
                                mode === 'reuse'
                                    ? 'border-brand bg-brand-soft ring-1 ring-brand/35'
                                    : 'border-border-subtle bg-inset hover:border-border',
                            )}
                        >
                            <div className="text-sm font-medium text-text">复用本机已有的 Node.js 环境</div>

                            {mode === 'reuse' && (
                                <div className="mt-2 flex flex-col gap-1.5 pt-1 border-t border-border-subtle">
                                    {validCandidates.map((cand) => (
                                        <label
                                            key={cand.path}
                                            className="flex items-center gap-2 text-xs text-text cursor-pointer hover:text-brand"
                                        >
                                            <input
                                                type="radio"
                                                name="node_candidate"
                                                checked={selectedPath === cand.path}
                                                onChange={() => setSelectedPath(cand.path)}
                                                className="accent-brand"
                                            />
                                            <span className="font-mono text-2xs px-1 py-0.5 rounded bg-surface border border-border-subtle">
                                                {cand.label}
                                            </span>
                                            <span className="truncate text-text-secondary" title={cand.path}>
                                                {cand.path}
                                            </span>
                                        </label>
                                    ))}
                                </div>
                            )}
                        </div>
                    )}
                </div>
                <DialogFooter>
                    <Button variant="secondary" onClick={() => onOpenChange(false)}>
                        取消
                    </Button>
                    <Button onClick={handleConfirm}>
                        开始安装
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
};
