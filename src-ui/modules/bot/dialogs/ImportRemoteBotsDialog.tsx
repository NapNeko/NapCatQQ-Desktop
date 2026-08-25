// 从远端库存勾选导入已有 Bot；不拷 WebUI 密钥。

import React, { useEffect, useMemo, useState } from 'react';
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
} from '../../../shared/ui';
import { cn } from '../../../shared/utils/cn';
import { botService } from '../../../core/services/bot.service';
import { serverService } from '../../../core/services/server.service';
import { createDefaultBotConfig } from '../../../core/domain/bot/config-defaults';
import {
    importableBackendLabel,
    importableDeploymentLabel,
    importableRemoteBotKey,
    importableSourceLabel,
} from '../../../core/domain/bot/importable-remote';
import { applyImportedNetwork } from '../../../core/domain/bot/imported-network';
import { pushInfoBar } from '../../../hooks/ui/globalInfoBarStore';
import { errorText } from '../../../core/domain/errors';
import { botSnapshotsKey } from '../../../hooks/bot/useBotSnapshots';
import { botConfigsKeyPrefix } from '../../../hooks/bot/useBotConfigsMap';
import { requestDesktopConsent } from '../../../hooks/desktop/desktopConsentHost';
import type { ImportableRemoteBot } from '../../../core/ipc/generated/domain/ImportableRemoteBot';
import type { BotConfig } from '../../../core/ipc/generated/domain/BotConfig';

interface Props {
    open: boolean;
    onOpenChange: (open: boolean) => void;
}

export const ImportRemoteBotsDialog: React.FC<Props> = ({ open, onOpenChange }) => {
    const queryClient = useQueryClient();
    const [selected, setSelected] = useState<Set<string>>(new Set());
    const [importing, setImporting] = useState(false);
    const [refreshing, setRefreshing] = useState(false);
    const [takeOverWebuiPwd, setTakeOverWebuiPwd] = useState(false);

    const { data, isLoading, isError, error, refetch } = useQuery({
        queryKey: ['importable-remote-bots'],
        queryFn: () => botService.listImportableRemoteBots(),
        enabled: open,
        staleTime: 0,
        refetchOnMount: 'always',
    });

    useEffect(() => {
        if (!open) return;
        void refetch();
    }, [open, refetch]);

    const rows = data ?? [];
    const selectableKeys = useMemo(
        () => rows.filter((r) => r.selectable).map(importableRemoteBotKey),
        [rows],
    );

    useEffect(() => {
        if (!open) return;
        setSelected(new Set(selectableKeys));
    }, [open, selectableKeys]);

    const selectedCount = selectableKeys.filter((k) => selected.has(k)).length;
    const allSelectableOn =
        selectableKeys.length > 0 && selectableKeys.every((k) => selected.has(k));

    const toggle = (row: ImportableRemoteBot) => {
        if (!row.selectable || importing) return;
        const key = importableRemoteBotKey(row);
        setSelected((prev) => {
            const next = new Set(prev);
            if (next.has(key)) {
                next.delete(key);
                return next;
            }
            for (const other of rows) {
                if (other.qqId === row.qqId) {
                    next.delete(importableRemoteBotKey(other));
                }
            }
            next.add(key);
            return next;
        });
    };

    const toggleAll = () => {
        if (importing) return;
        setSelected((prev) => {
            const allOn = selectableKeys.every((k) => prev.has(k));
            return allOn ? new Set() : new Set(selectableKeys);
        });
    };

    const handleRefreshInventories = async () => {
        setRefreshing(true);
        try {
            const servers = await serverService.list();
            const failed: string[] = [];
            for (const s of servers) {
                try {
                    await serverService.refreshInventory(s.id);
                } catch (err) {
                    failed.push(`${s.name || s.host}：${errorText(err)}`);
                }
            }
            await queryClient.invalidateQueries({ queryKey: ['servers'] });
            await refetch();
            if (failed.length > 0) {
                pushInfoBar({
                    key: 'import-remote-bots-refresh',
                    tone: 'warning',
                    title: '部分主机重新发现失败',
                    content: failed.join(' '),
                });
            }
        } finally {
            setRefreshing(false);
        }
    };

    const handleImport = () => {
        const toImport = rows.filter(
            (r) => r.selectable && selected.has(importableRemoteBotKey(r)),
        );
        if (toImport.length === 0) return;
        void requestDesktopConsent(() => {
            void runImport(toImport);
        });
    };

    const runImport = async (toImport: ImportableRemoteBot[]) => {
        setImporting(true);
        const created: number[] = [];
        const failed: { qq: number; message: string }[] = [];
        const seenQq = new Set<number>();
        try {
            let migratedNetworks = 0;
            const networkMigrateFailed: string[] = [];
            for (const row of toImport) {
                if (seenQq.has(row.qqId)) continue;
                seenQq.add(row.qqId);
                try {
                    const cfg = toBotConfig(row, takeOverWebuiPwd);
                    // 失败不阻断导入，落默认空配置并在提示里说明
                    try {
                        const imported = await botService.fetchImportedNetwork(
                            row.serverId,
                            String(row.qqId),
                            row.backend,
                            row.deployment,
                            row.dockerName,
                        );
                        if (imported) {
                            applyImportedNetwork(cfg, imported);
                            migratedNetworks += 1;
                        }
                    } catch (err) {
                        networkMigrateFailed.push(
                            `${row.qqId}：${errorText(err)}`,
                        );
                    }
                    await botService.upsertConfig(cfg);
                    created.push(row.qqId);
                } catch (err) {
                    failed.push({ qq: row.qqId, message: errorText(err) });
                }
            }
            let attached = 0;
            if (created.length > 0) {
                try {
                    const attachedIds = await botService.reconcileRuntimes(
                        created.map((qq) => String(qq)),
                    );
                    attached = attachedIds.length;
                } catch (err) {
                    pushInfoBar({
                        key: 'import-remote-bots-reconcile',
                        tone: 'warning',
                        title: '已导入，但未能接管运行态',
                        content: errorText(err),
                    });
                }
            }
            await queryClient.invalidateQueries({ queryKey: botSnapshotsKey });
            await queryClient.invalidateQueries({ queryKey: botConfigsKeyPrefix });
            await refetch();
            if (failed.length === 0) {
                const attachNote =
                    attached > 0
                        ? `其中 ${attached} 个远端仍在运行，已接到控制台。`
                        : '远端未在跑的实例保持停止，可稍后启动。';
                const parts: string[] = [attachNote];
                if (migratedNetworks > 0) {
                    parts.push(`已迁移 ${migratedNetworks} 个远端网络配置。`);
                }
                if (networkMigrateFailed.length > 0) {
                    parts.push(`网络配置迁移失败：${networkMigrateFailed.join('；')}`);
                }
                pushInfoBar({
                    key: 'import-remote-bots',
                    tone: 'success',
                    title: '已导入 Bot',
                    content: `已登记 ${created.length} 个实例。${parts.join(' ')}`,
                    autoDismissMs: 8000,
                });
                onOpenChange(false);
            } else {
                pushInfoBar({
                    key: 'import-remote-bots',
                    tone: created.length > 0 ? 'warning' : 'danger',
                    title: created.length > 0 ? '部分 Bot 没有导入' : '没有导入',
                    content:
                        (created.length > 0 ? `成功 ${created.length} 个。` : '') +
                        failed.map((f) => `${f.qq}：${f.message}`).join(' '),
                });
            }
        } finally {
            setImporting(false);
        }
    };

    const busy = importing || refreshing;

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent size="md" dismissOnOutsideClick={false}>
                <DialogHeader>
                    <DialogTitle>导入已有 Bot</DialogTitle>
                    <DialogDescription>
                        从已发现的 NapCat / SnowLuma 安装和 Docker 容器里登记 QQ。
                        不会改写远端 WebUI 密码；若实例还在跑，导入后会接到控制台。
                    </DialogDescription>
                </DialogHeader>

                {isLoading ? (
                    <p className="py-5 text-center text-sm text-text-tertiary">
                        正在对照远端库存…
                    </p>
                ) : isError ? (
                    <p className="py-5 text-center text-sm text-danger">{errorText(error)}</p>
                ) : rows.length === 0 ? (
                    <p className="py-5 text-center text-sm text-text-secondary">
                        还没有发现可导入的 Bot。请先在远端页添加主机并刷新库存，或点下面重新发现。
                    </p>
                ) : (
                    <BotPickList
                        rows={rows}
                        selected={selected}
                        importing={busy}
                        selectedCount={selectedCount}
                        selectableCount={selectableKeys.length}
                        allSelectableOn={allSelectableOn}
                        onToggle={toggle}
                        onToggleAll={toggleAll}
                    />
                )}

                {rows.length > 0 && (
                    <div className="rounded-md bg-inset/50 px-3 py-2">
                        <Checkbox
                            id="import-bots-webui-takeover"
                            checked={takeOverWebuiPwd}
                            disabled={busy || selectedCount === 0}
                            onCheckedChange={(v) => setTakeOverWebuiPwd(v === true)}
                            label="接管 WebUI 密码"
                            hint="勾选后，所选远端 Native SnowLuma 下次启动会覆盖 WebUI 密码；不勾选则不改远端配置。"
                        />
                    </div>
                )}

                <DialogFooter>
                    <Button
                        size="sm"
                        variant="ghost"
                        type="button"
                        disabled={busy}
                        onClick={() => void handleRefreshInventories()}
                    >
                        {refreshing ? '发现中…' : '重新发现'}
                    </Button>
                    <DialogClose asChild>
                        <Button size="sm" variant="ghost" type="button" disabled={busy}>
                            取消
                        </Button>
                    </DialogClose>
                    <Button
                        size="sm"
                        variant="primary"
                        type="button"
                        disabled={busy || selectedCount === 0}
                        onClick={handleImport}
                    >
                        {importing ? '导入中…' : `导入 ${selectedCount} 个`}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
};

function toBotConfig(
    row: ImportableRemoteBot,
    takeOverWebuiPwd: boolean,
): BotConfig {
    const base = createDefaultBotConfig();
    const backendLabel = importableBackendLabel(row.backend);
    return {
        ...base,
        bot: {
            ...base.bot,
            name: `${row.serverName} ${backendLabel} ${row.qqId}`,
            QQID: row.qqId,
            runtime_target: row.serverId,
            backend_type: row.backend,
            deploymentType: row.deployment,
            snowlumaStartMode:
                row.backend === 'snowluma' ? { mode: 'cold_start' } : undefined,
            webuiPasswordTakeover: takeOverWebuiPwd,
        },
    };
}

function BotPickList({
    rows,
    selected,
    importing,
    selectedCount,
    selectableCount,
    allSelectableOn,
    onToggle,
    onToggleAll,
}: {
    rows: ImportableRemoteBot[];
    selected: Set<string>;
    importing: boolean;
    selectedCount: number;
    selectableCount: number;
    allSelectableOn: boolean;
    onToggle: (row: ImportableRemoteBot) => void;
    onToggleAll: () => void;
}) {
    return (
        <div className="flex flex-col gap-2">
            {selectableCount > 1 ? (
                <div className="flex items-center justify-between gap-2 px-0.5">
                    <Checkbox
                        id="import-bots-select-all"
                        checked={allSelectableOn}
                        disabled={importing}
                        onCheckedChange={() => onToggleAll()}
                        label="全选"
                    />
                    <span className="text-2xs text-text-tertiary">
                        已选 {selectedCount} / {selectableCount}
                    </span>
                </div>
            ) : null}
            <ul className="max-h-[min(52vh,26rem)] overflow-y-auto">
                {rows.map((row, index) => {
                    const key = importableRemoteBotKey(row);
                    const on = selected.has(key);
                    const id = `import-bot-${key.replace(/[^a-zA-Z0-9_-]/g, '-')}`;
                    return (
                        <li
                            key={key}
                            className={cn(index > 0 && 'border-t border-border-subtle/70')}
                        >
                            <label
                                htmlFor={id}
                                className={cn(
                                    'flex items-center gap-2.5 py-2',
                                    row.selectable
                                        ? 'cursor-pointer'
                                        : 'cursor-not-allowed opacity-55',
                                )}
                            >
                                <Checkbox
                                    id={id}
                                    checked={on}
                                    disabled={!row.selectable || importing}
                                    onCheckedChange={() => onToggle(row)}
                                />
                                <span className="min-w-0 flex-1">
                                    <span className="flex min-w-0 items-baseline justify-between gap-2">
                                        <span className="truncate text-sm font-medium text-text">
                                            {row.qqId}
                                        </span>
                                        {row.skipReason ? (
                                            <span className="shrink-0 text-2xs text-text-tertiary">
                                                {row.skipReason}
                                            </span>
                                        ) : (
                                            <span className="shrink-0 text-2xs text-text-secondary">
                                                {importableBackendLabel(row.backend)} ·{' '}
                                                {importableDeploymentLabel(row.deployment)}
                                            </span>
                                        )}
                                    </span>
                                    <span className="mt-px block truncate text-2xs text-text-tertiary">
                                        {row.serverName}
                                        {' · '}
                                        {importableSourceLabel(row.source)}
                                        {row.dockerName ? ` · ${row.dockerName}` : ''}
                                    </span>
                                </span>
                            </label>
                        </li>
                    );
                })}
            </ul>
        </div>
    );
}
