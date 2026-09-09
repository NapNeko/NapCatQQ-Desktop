// 远端 Linux 目录选择：面包屑 + 列表，只确认当前目录。

import { useEffect, useMemo, useState } from 'react';
import { ChevronRight, File, Folder, FolderUp, RefreshCw } from 'lucide-react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Spinner,
} from '../ui';
import { ActionMotionIcon, refreshMotion } from '../ui/motion';
import { cn } from '../utils/cn';
import { useRemoteDirectory } from '../../hooks/remote/useRemoteDirectory';
import {
    joinPosix,
    normalizePosix,
    parentPosix,
    posixSegments,
} from '../../core/domain/remote-host/posixPath';
import { errorText } from '../../core/domain/errors';
import { pushErrorBar } from '../../hooks/ui/pushErrorBar';

const LIST_ERROR_KEY = 'remote-dir-list';

export interface RemoteDirectoryPickerProps {
    open: boolean;
    remoteId: string | null;
    /** 打开时进入的目录；不是绝对路径则用 initialRoot */
    initialPath?: string;
    /** 没有可用 initialPath 时的起点，默认 / */
    initialRoot?: string;
    title?: string;
    onClose: () => void;
    onSelect: (path: string) => void;
}

function startPath(initialPath: string | undefined, initialRoot: string | undefined): string {
    const raw = initialPath?.trim();
    if (raw?.startsWith('/')) return normalizePosix(raw);
    return normalizePosix(initialRoot || '/');
}

export function RemoteDirectoryPicker({
    open,
    remoteId,
    initialPath,
    initialRoot,
    title = '选择目录',
    onClose,
    onSelect,
}: RemoteDirectoryPickerProps) {
    const [browsePath, setBrowsePath] = useState(() => startPath(initialPath, initialRoot));

    useEffect(() => {
        if (!open) return;
        setBrowsePath(startPath(initialPath, initialRoot));
    }, [open, initialPath, initialRoot]);

    const listing = useRemoteDirectory(remoteId, browsePath, open && !!remoteId);
    const atRoot = browsePath === '/';

    useEffect(() => {
        if (!open || !listing.error) return;
        pushErrorBar({
            key: LIST_ERROR_KEY,
            title: '读取目录失败',
            raw: errorText(listing.error),
        });
    }, [open, listing.error]);

    const rows = useMemo(() => {
        const entries = listing.data ?? [];
        const visible = entries.filter((e) => !e.name.startsWith('.'));
        const dirs = visible.filter((e) => e.is_dir).sort((a, b) => a.name.localeCompare(b.name));
        const files = visible.filter((e) => !e.is_dir).sort((a, b) => a.name.localeCompare(b.name));
        return [...dirs, ...files];
    }, [listing.data]);

    const crumbs = posixSegments(browsePath);
    const canSelect = open && !!remoteId && !listing.isLoading && !listing.error;

    return (
        <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
            <DialogContent size="lg" dismissOnOutsideClick={false}>
                <DialogHeader>
                    <DialogTitle>{title}</DialogTitle>
                </DialogHeader>

                <div className="flex items-center gap-1.5">
                    <nav
                        className="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto text-xs"
                        aria-label="当前路径"
                    >
                        <Crumb label="/" current={atRoot} onClick={() => setBrowsePath('/')} />
                        {crumbs.map((seg, i) => {
                            const path = `/${crumbs.slice(0, i + 1).join('/')}`;
                            const last = i === crumbs.length - 1;
                            return (
                                <span key={path} className="flex min-w-0 items-center gap-0.5">
                                    <ChevronRight size={12} className="shrink-0 text-text-tertiary" />
                                    <Crumb
                                        label={seg}
                                        current={last}
                                        onClick={() => setBrowsePath(path)}
                                    />
                                </span>
                            );
                        })}
                    </nav>
                    <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7 shrink-0"
                        disabled={atRoot || listing.isFetching}
                        aria-label="上一级"
                        onClick={() => setBrowsePath(parentPosix(browsePath))}
                    >
                        <FolderUp size={14} />
                    </Button>
                    <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7 shrink-0"
                        disabled={listing.isFetching}
                        aria-label="刷新"
                        onClick={() => void listing.refetch()}
                    >
                        <ActionMotionIcon
                            icon={RefreshCw}
                            size={14}
                            motion={refreshMotion(listing.isFetching)}
                        />
                    </Button>
                </div>

                <div className="flex h-72 flex-col overflow-hidden rounded-sm border border-border-subtle bg-inset/30">
                    {listing.isLoading ? (
                        <div className="flex flex-1 flex-col items-center justify-center gap-3">
                            <Spinner size="lg" tone="brand" label="正在读取目录…" />
                            <p className="text-sm text-text-tertiary">正在读取目录…</p>
                        </div>
                    ) : listing.error ? (
                        <div className="flex flex-1 flex-col items-center justify-center gap-2">
                            <p className="text-sm text-text-secondary">读取失败</p>
                            <Button size="sm" variant="secondary" onClick={() => void listing.refetch()}>
                                重试
                            </Button>
                        </div>
                    ) : rows.length === 0 ? (
                        <div className="flex flex-1 items-center justify-center">
                            <p className="text-sm text-text-tertiary">空目录</p>
                        </div>
                    ) : (
                        <ul className="min-h-0 flex-1 overflow-y-auto p-1">
                            {rows.map((entry) => (
                                <li key={`${entry.is_dir ? 'd' : 'f'}:${entry.name}`}>
                                    {entry.is_dir ? (
                                        <button
                                            type="button"
                                            className={cn(
                                                'flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-sm text-text',
                                                'hover:bg-inset',
                                            )}
                                            onClick={() => setBrowsePath(joinPosix(browsePath, entry.name))}
                                            onDoubleClick={() =>
                                                setBrowsePath(joinPosix(browsePath, entry.name))
                                            }
                                        >
                                            <Folder
                                                size={15}
                                                strokeWidth={2}
                                                className="shrink-0 text-text-secondary"
                                            />
                                            <span className="min-w-0 truncate">{entry.name}</span>
                                        </button>
                                    ) : (
                                        <div className="flex items-center gap-2 px-2 py-1.5 text-sm text-text-tertiary">
                                            <File size={15} strokeWidth={2} className="shrink-0" />
                                            <span className="min-w-0 truncate">{entry.name}</span>
                                        </div>
                                    )}
                                </li>
                            ))}
                        </ul>
                    )}
                </div>

                <DialogFooter className="justify-between">
                    <p className="min-w-0 truncate text-xs text-text-tertiary" title={browsePath}>
                        {browsePath}
                    </p>
                    <div className="flex shrink-0 items-center gap-2">
                        <Button variant="ghost" size="sm" onClick={onClose}>
                            取消
                        </Button>
                        <Button
                            variant="primary"
                            size="sm"
                            disabled={!canSelect}
                            onClick={() => onSelect(browsePath)}
                        >
                            选择此目录
                        </Button>
                    </div>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}

function Crumb({
    label,
    current,
    onClick,
}: {
    label: string;
    current: boolean;
    onClick: () => void;
}) {
    if (current) {
        return <span className="shrink-0 font-medium text-text">{label}</span>;
    }
    return (
        <button
            type="button"
            className="shrink-0 text-text-secondary hover:text-text"
            onClick={onClick}
        >
            {label}
        </button>
    );
}
