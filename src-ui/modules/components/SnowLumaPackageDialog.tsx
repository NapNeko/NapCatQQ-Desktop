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
import type { SnowLumaPackage } from '../../core/ipc/types';

interface Props {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    onConfirm: (pkg: SnowLumaPackage) => void;
}

export const SnowLumaPackageDialog: React.FC<Props> = ({
    open,
    onOpenChange,
    onConfirm,
}) => {
    const [pkg, setPkg] = useState<SnowLumaPackage>('full');

    useEffect(() => {
        if (open) setPkg('full');
    }, [open]);

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent size="md" dismissOnOutsideClick={false}>
                <DialogHeader>
                    <DialogTitle>选择 SnowLuma 安装包</DialogTitle>
                    <DialogDescription>选一种包形态后开始安装。</DialogDescription>
                </DialogHeader>
                <div className="flex flex-col gap-2">
                    <PackageChoice
                        selected={pkg === 'full'}
                        title="完整版（推荐）"
                        detail="自带 SnowLuma 所需的运行时，开箱即用。升级时会沿用当前包类型。"
                        onSelect={() => setPkg('full')}
                    />
                    <PackageChoice
                        selected={pkg === 'lite'}
                        title="Lite 精简版"
                        detail="体积小，不含 Node。安装任务会自动安装 Node.js 作为运行依赖。"
                        onSelect={() => setPkg('lite')}
                    />
                </div>
                <DialogFooter>
                    <Button variant="secondary" onClick={() => onOpenChange(false)}>
                        取消
                    </Button>
                    <Button
                        onClick={() => {
                            onConfirm(pkg);
                            onOpenChange(false);
                        }}
                    >
                        下一步
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
};

function PackageChoice({
    selected,
    title,
    detail,
    onSelect,
}: {
    selected: boolean;
    title: string;
    detail: string;
    onSelect: () => void;
}) {
    return (
        <button
            type="button"
            onClick={onSelect}
            className={cn(
                'rounded-sm border px-3 py-2.5 text-left transition-colors',
                selected
                    ? 'border-brand bg-brand-soft ring-1 ring-brand/35'
                    : 'border-border-subtle bg-inset hover:border-border',
            )}
        >
            <div className="text-sm font-medium text-text">{title}</div>
            <p className="mt-0.5 text-2xs leading-snug text-text-secondary">{detail}</p>
        </button>
    );
}
