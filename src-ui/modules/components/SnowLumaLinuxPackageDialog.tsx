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
import type { SnowLumaLinuxPackage } from '../../core/ipc/types';

interface Props {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    onConfirm: (pkg: SnowLumaLinuxPackage) => void;
}

export const SnowLumaLinuxPackageDialog: React.FC<Props> = ({
    open,
    onOpenChange,
    onConfirm,
}) => {
    const [pkg, setPkg] = useState<SnowLumaLinuxPackage>('full');

    useEffect(() => {
        if (open) setPkg('full');
    }, [open]);

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent size="md" dismissOnOutsideClick={false}>
                <DialogHeader>
                    <DialogTitle>选择 SnowLuma 安装包</DialogTitle>
                    <DialogDescription>
                        完整版自带 Node，开箱即用。Lite 更小，会先自动安装 Node.js 组件（以后也可单独给别的用途用）。
                    </DialogDescription>
                </DialogHeader>
                <div className="flex flex-col gap-2">
                    <PackageChoice
                        selected={pkg === 'full'}
                        title="完整版（推荐）"
                        detail="自带 Node 22.13，约 45MB。导入的现有安装若已带 ./node，更新也会走这一路。"
                        onSelect={() => setPkg('full')}
                    />
                    <PackageChoice
                        selected={pkg === 'lite'}
                        title="Lite"
                        detail="约 4MB，不含 Node。编排会先安装 Node.js 组件（22.13+），再装 SnowLuma。"
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
                        开始安装
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
                    ? 'border-brand bg-brand-soft'
                    : 'border-border-subtle bg-inset hover:border-border',
            )}
        >
            <div className="text-sm font-medium text-text">{title}</div>
            <p className="mt-0.5 text-2xs leading-snug text-text-secondary">{detail}</p>
        </button>
    );
}
