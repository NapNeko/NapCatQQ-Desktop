// 从本机 ~/.ssh/config 勾选导入远端档案；导入后不测连接。

import React, { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
    DialogClose,
    Button,
    Checkbox,
} from '../../shared/ui';
import { cn } from '../../shared/utils/cn';
import { serverService } from '../../core/services/server.service';
import { pushInfoBar } from '../../hooks/ui/globalInfoBarStore';
import { pushErrorBar } from '../../hooks/ui/pushErrorBar';
import { errorText } from '../../core/domain/errors';
import { SEE_LOGS_HINT } from '../../core/domain/ui/errorBarCopy';
import type { DiscoveredSshHost } from '../../core/ipc/generated/domain/DiscoveredSshHost';
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';

interface ImportSshConfigDialogProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
}

export const ImportSshConfigDialog: React.FC<ImportSshConfigDialogProps> = ({
    open,
    onOpenChange,
}) => {
    const queryClient = useQueryClient();
    const [selected, setSelected] = useState<Set<string>>(new Set());
    const [importing, setImporting] = useState(false);

    const { data, isLoading, isError, error, refetch } = useQuery({
        queryKey: ['ssh-config-hosts'],
        queryFn: () => serverService.discoverLocalSshHosts(),
        enabled: open,
        // 档案增删后「已添加」必须立刻变；不受 dev 全局 30s staleTime 拖累。
        staleTime: 0,
        refetchOnMount: 'always',
    });

    useEffect(() => {
        if (!open) return;
        void refetch();
    }, [open, refetch]);

    useEffect(() => {
        if (!open || !isError) return;
        pushErrorBar({
            key: 'ssh-config-read',
            title: '读取 SSH 配置失败',
            raw: errorText(error),
        });
    }, [error, isError, open]);

    const hosts = useMemo(() => {
        const raw = data ?? [];
        return [...raw].sort((a, b) => Number(b.selectable) - Number(a.selectable));
    }, [data]);
    const selectableAliases = useMemo(
        () => hosts.filter((h) => h.selectable).map((h) => h.alias),
        [hosts],
    );

    useEffect(() => {
        if (!open) return;
        setSelected(new Set(selectableAliases));
    }, [open, selectableAliases]);

    const selectedCount = selectableAliases.filter((a) => selected.has(a)).length;
    const allSelectableOn =
        selectableAliases.length > 0 && selectableAliases.every((a) => selected.has(a));

    const toggle = (alias: string, selectable: boolean) => {
        if (!selectable || importing) return;
        setSelected((prev) => {
            const next = new Set(prev);
            if (next.has(alias)) next.delete(alias);
            else next.add(alias);
            return next;
        });
    };

    const toggleAll = () => {
        if (importing) return;
        setSelected((prev) => {
            const allOn = selectableAliases.every((a) => prev.has(a));
            return allOn ? new Set() : new Set(selectableAliases);
        });
    };

    const handleImport = async () => {
        const toImport = hosts.filter((h) => h.selectable && selected.has(h.alias));
        if (toImport.length === 0) return;
        setImporting(true);
        const created: string[] = [];
        const failed: { alias: string; message: string }[] = [];
        try {
            for (const host of toImport) {
                try {
                    await serverService.add(toProfile(host));
                    created.push(host.alias);
                } catch (err) {
                    failed.push({ alias: host.alias, message: errorText(err) });
                }
            }
            await queryClient.invalidateQueries({ queryKey: ['servers'] });
            await queryClient.invalidateQueries({ queryKey: ['ssh-config-hosts'] });
            if (failed.length === 0) {
                pushInfoBar({
                    key: 'ssh-config-import',
                    tone: 'success',
                    title: '已加入远端列表',
                    content: `已添加 ${created.length} 台主机。可在卡片上测试连接。`,
                    autoDismissMs: 5000,
                });
                onOpenChange(false);
            } else {
                console.error('[ssh-import] failed hosts', failed);
                if (created.length > 0) {
                    pushInfoBar({
                        key: 'ssh-config-import',
                        tone: 'warning',
                        title: '部分主机没有加进去',
                        content: `成功 ${created.length} 台，失败 ${failed.length} 台。${SEE_LOGS_HINT}`,
                    });
                } else {
                    pushErrorBar({
                        key: 'ssh-config-import',
                        title: '没有加进去',
                        raw: `失败 ${failed.length} 台`,
                    });
                }
                if (created.length > 0) {
                    void refetch();
                    setSelected(new Set());
                }
            }
        } finally {
            setImporting(false);
        }
    };

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent size="md" dismissOnOutsideClick={false}>
                <DialogHeader>
                    <DialogTitle>从 SSH 配置导入</DialogTitle>
                    <DialogDescription>
                        把本机已经能连上的机器加进远端列表，不用再手填地址。
                    </DialogDescription>
                </DialogHeader>

                {isLoading ? (
                    <p className="py-5 text-center text-sm text-text-tertiary">
                        正在读取本机 SSH 配置…
                    </p>
                ) : isError ? (
                    <p className="py-5 text-center text-sm text-text-secondary">读取失败，详情见日志</p>
                ) : hosts.length === 0 ? (
                    <p className="py-5 text-center text-sm text-text-secondary">
                        本机还没有可用的 SSH 主机条目。仍可手动添加服务器。
                    </p>
                ) : (
                    <HostPickList
                        hosts={hosts}
                        selected={selected}
                        importing={importing}
                        selectedCount={selectedCount}
                        selectableCount={selectableAliases.length}
                        allSelectableOn={allSelectableOn}
                        onToggle={toggle}
                        onToggleAll={toggleAll}
                    />
                )}

                <DialogFooter>
                    <DialogClose asChild>
                        <Button size="sm" variant="ghost" type="button" disabled={importing}>
                            取消
                        </Button>
                    </DialogClose>
                    <Button
                        size="sm"
                        variant="primary"
                        type="button"
                        disabled={importing || selectedCount === 0}
                        onClick={() => void handleImport()}
                    >
                        {importing ? '导入中…' : `导入 ${selectedCount} 台`}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
};

function HostPickList({
    hosts,
    selected,
    importing,
    selectedCount,
    selectableCount,
    allSelectableOn,
    onToggle,
    onToggleAll,
}: {
    hosts: DiscoveredSshHost[];
    selected: Set<string>;
    importing: boolean;
    selectedCount: number;
    selectableCount: number;
    allSelectableOn: boolean;
    onToggle: (alias: string, selectable: boolean) => void;
    onToggleAll: () => void;
}) {
    const listRef = useRef<HTMLUListElement>(null);
    const fade = useScrollEdges(listRef, hosts.length);

    return (
        <div className="flex flex-col gap-1.5">
            {selectableCount > 1 && (
                <div className="flex items-center justify-between gap-2 px-0.5">
                    <Checkbox
                        id="ssh-import-select-all"
                        checked={allSelectableOn}
                        disabled={importing}
                        onCheckedChange={onToggleAll}
                        label="全选"
                    />
                    <span className="text-2xs text-text-tertiary">
                        已选 {selectedCount} / {selectableCount}
                    </span>
                </div>
            )}
            <div className="relative">
                <ul
                    ref={listRef}
                    className="scrollbar-hide max-h-[min(52vh,26rem)] overflow-y-auto overscroll-contain"
                >
                    {hosts.map((host, index) => (
                        <HostPickRow
                            key={host.alias}
                            host={host}
                            selected={host.selectable && selected.has(host.alias)}
                            importing={importing}
                            divided={index > 0}
                            onToggle={() => onToggle(host.alias, host.selectable)}
                        />
                    ))}
                </ul>
                {fade.top && (
                    <div
                        aria-hidden
                        className="pointer-events-none absolute inset-x-0 top-0 h-6 bg-gradient-to-b from-elevated to-transparent"
                    />
                )}
                {fade.bottom && (
                    <div
                        aria-hidden
                        className="pointer-events-none absolute inset-x-0 bottom-0 h-7 bg-gradient-to-t from-elevated to-transparent"
                    />
                )}
            </div>
        </div>
    );
}

function HostPickRow({
    host,
    selected,
    importing,
    divided,
    onToggle,
}: {
    host: DiscoveredSshHost;
    selected: boolean;
    importing: boolean;
    divided: boolean;
    onToggle: () => void;
}) {
    const id = `ssh-import-${host.alias}`;
    const endpoint = `${host.username}@${host.host}:${host.port}`;
    const keyName = host.identityFile ? fileName(host.identityFile) : null;
    const sideNote = host.skipReason
        ? shortSkipReason(host.skipReason)
        : host.identityFileMissing
            ? '密钥缺失'
            : null;

    return (
        <li className={cn(divided && 'border-t border-border-subtle/70')}>
            <label
                htmlFor={id}
                className={cn(
                    'flex items-center gap-2.5 py-2',
                    host.selectable ? 'cursor-pointer' : 'cursor-not-allowed opacity-55',
                )}
            >
                <Checkbox
                    id={id}
                    checked={selected}
                    disabled={!host.selectable || importing}
                    onCheckedChange={onToggle}
                />
                <span className="min-w-0 flex-1">
                    <span className="flex min-w-0 items-baseline justify-between gap-2">
                        <span className="truncate text-sm font-medium text-text">{host.alias}</span>
                        {sideNote && (
                            <span className="shrink-0 text-2xs text-text-tertiary">{sideNote}</span>
                        )}
                    </span>
                    <span className="mt-px block truncate font-mono text-2xs text-text-tertiary">
                        {endpoint}
                        {keyName && !host.skipReason ? ` · ${keyName}` : ''}
                    </span>
                </span>
            </label>
        </li>
    );
}

function useScrollEdges(ref: React.RefObject<HTMLElement | null>, dep: number) {
    const [edges, setEdges] = useState({ top: false, bottom: false });

    useLayoutEffect(() => {
        const el = ref.current;
        if (!el) return;

        const update = () => {
            const top = el.scrollTop > 2;
            const bottom = el.scrollTop + el.clientHeight < el.scrollHeight - 2;
            setEdges((prev) =>
                prev.top === top && prev.bottom === bottom ? prev : { top, bottom },
            );
        };

        update();
        el.addEventListener('scroll', update, { passive: true });
        const ro = new ResizeObserver(update);
        ro.observe(el);
        return () => {
            el.removeEventListener('scroll', update);
            ro.disconnect();
        };
    }, [ref, dep]);

    return edges;
}

function shortSkipReason(reason: string): string {
    if (reason.includes('跳板')) return '不支持跳板';
    if (reason.includes('通配')) return '通配别名';
    if (reason.includes('已添加')) return '已添加';
    return reason;
}

function toProfile(host: DiscoveredSshHost): ServerProfile {
    return {
        id: '',
        name: host.alias,
        host: host.host,
        port: host.port,
        username: host.username,
        authMethod: host.authMethod,
        privateKeyPath: host.identityFile ?? null,
        rememberCredential: true,
        state: 'disconnected',
        webuiUrl: null,
        pathOverrides: null,
        inventory: null,
    };
}

function fileName(path: string): string {
    const idx = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
    return idx >= 0 ? path.slice(idx + 1) : path;
}
