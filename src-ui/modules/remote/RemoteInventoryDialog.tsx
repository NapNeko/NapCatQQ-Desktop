import React, { useEffect, useState } from 'react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    TextField,
} from '../../shared/ui';
import type { RemoteInventory } from '../../core/ipc/generated/domain/RemoteInventory';
import type { RemotePathOverrides } from '../../core/ipc/generated/domain/RemotePathOverrides';
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';
import {
    inventoryKindLabel,
    inventorySourceLabel,
    inventorySummary,
    isInventoryItemSelected,
} from '../../core/domain/remote-host/inventory-labels';

interface RemoteInventoryDialogProps {
    open: boolean;
    server: ServerProfile | null;
    isRefreshing: boolean;
    isSaving: boolean;
    error: string | null;
    onOpenChange: (open: boolean) => void;
    onRefresh: () => void;
    onSaveOverrides: (overrides: RemotePathOverrides) => void;
}

export const RemoteInventoryDialog: React.FC<RemoteInventoryDialogProps> = ({
    open,
    server,
    isRefreshing,
    isSaving,
    error,
    onOpenChange,
    onRefresh,
    onSaveOverrides,
}) => {
    const inv: RemoteInventory | null | undefined = server?.inventory;
    const [qqInstallBase, setQqInstallBase] = useState('');
    const [napcatRoot, setNapcatRoot] = useState('');
    const [snowlumaDir, setSnowlumaDir] = useState('');
    const [nodeBin, setNodeBin] = useState('');
    const [ncdWatchRoot, setNcdWatchRoot] = useState('');
    const [overrideError, setOverrideError] = useState<string | null>(null);

    useEffect(() => {
        if (!open || !server) return;
        const o = server.pathOverrides;
        setQqInstallBase(o?.qqInstallBase ?? '');
        setNapcatRoot(o?.napcatRoot ?? '');
        setSnowlumaDir(o?.snowlumaDir ?? '');
        setNodeBin(o?.nodeBin ?? '');
        setNcdWatchRoot(o?.ncdWatchRoot ?? '');
        setOverrideError(null);
    }, [open, server]);

    const handleSave = () => {
        const overrides: RemotePathOverrides = {
            qqInstallBase: qqInstallBase.trim() || null,
            napcatRoot: napcatRoot.trim() || null,
            snowlumaDir: snowlumaDir.trim() || null,
            nodeBin: nodeBin.trim() || null,
            ncdWatchRoot: ncdWatchRoot.trim() || null,
        };
        const invalid = Object.values(overrides).find(
            (v) => typeof v === 'string' && v.includes(' '),
        );
        if (invalid) {
            setOverrideError('路径不能包含空格');
            return;
        }
        setOverrideError(null);
        onSaveOverrides(overrides);
    };

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent size="lg" dismissOnOutsideClick={false}>
                <DialogHeader>
                    <DialogTitle>远端安装路径</DialogTitle>
                    <DialogDescription>
                        {inventorySummary(inv)}
                        {inv?.probedAt ? ` · 探测于 ${inv.probedAt}` : ''}
                    </DialogDescription>
                </DialogHeader>

                <div className="flex max-h-[60vh] flex-col gap-4 overflow-y-auto">
                    {error ? <p className="text-sm text-text-secondary">重新发现失败，详情见日志</p> : null}
                    {overrideError ? <p className="text-sm text-danger">{overrideError}</p> : null}

                    <section className="flex flex-col gap-2">
                        <h3 className="text-xs font-medium uppercase tracking-wide text-text-tertiary">
                            发现结果
                        </h3>
                        {!inv || inv.items.length === 0 ? (
                            <p className="text-sm text-text-secondary">
                                未发现安装，组件安装将落到桌面默认路径（$HOME/Napcat、
                                $HOME/snowluma-remote）。
                            </p>
                        ) : (
                            <ul className="flex flex-col gap-1.5">
                                {inv.items.map((item) => {
                                    const selected = isInventoryItemSelected(
                                        item.kind,
                                        item.root,
                                        inv.selected,
                                    );
                                    return (
                                        <li
                                            key={`${item.kind}:${item.root}`}
                                            className="rounded-sm border border-border-subtle bg-inset px-3 py-2 text-sm"
                                        >
                                            <div className="flex flex-wrap items-center gap-2">
                                                <span className="font-medium text-text">
                                                    {inventoryKindLabel(item.kind)}
                                                </span>
                                                <span className="text-2xs text-text-tertiary">
                                                    {inventorySourceLabel(item.source)}
                                                </span>
                                                {selected ? (
                                                    <span className="text-2xs text-success">
                                                        当前使用
                                                    </span>
                                                ) : null}
                                                {!item.verified ? (
                                                    <span className="text-2xs text-danger">
                                                        指纹未通过
                                                    </span>
                                                ) : null}
                                            </div>
                                            <p className="mt-0.5 truncate font-mono text-2xs text-text-secondary">
                                                {item.root}
                                            </p>
                                        </li>
                                    );
                                })}
                            </ul>
                        )}
                    </section>

                    <section className="flex flex-col gap-2">
                        <h3 className="text-xs font-medium uppercase tracking-wide text-text-tertiary">
                            高级：路径覆盖
                        </h3>
                        <p className="text-2xs text-text-secondary">
                            留空表示不覆盖。保存后会重新发现并校验；无效路径会标红，不会静默改回默认。
                        </p>
                        <TextField
                            label="QQ 安装根"
                            placeholder="$HOME/Napcat 或 /"
                            value={qqInstallBase}
                            onValueChange={setQqInstallBase}
                        />
                        <TextField
                            label="NapCat 目录"
                            placeholder="…/app_launcher/napcat"
                            value={napcatRoot}
                            onValueChange={setNapcatRoot}
                        />
                        <TextField
                            label="SnowLuma 目录"
                            placeholder="…/workspace/snowluma"
                            value={snowlumaDir}
                            onValueChange={setSnowlumaDir}
                        />
                        <TextField
                            label="node 可执行文件"
                            placeholder="…/node/bin/node"
                            value={nodeBin}
                            onValueChange={setNodeBin}
                        />
                        <TextField
                            label="ncd-watch 根目录"
                            placeholder="$HOME/ncd-watch"
                            value={ncdWatchRoot}
                            onValueChange={setNcdWatchRoot}
                        />
                    </section>
                </div>

                <DialogFooter>
                    <Button variant="secondary" onClick={onRefresh} disabled={isRefreshing}>
                        {isRefreshing ? '正在发现…' : '重新发现'}
                    </Button>
                    <Button onClick={handleSave} disabled={isSaving || isRefreshing}>
                        {isSaving ? '保存中…' : '保存覆盖'}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
};
