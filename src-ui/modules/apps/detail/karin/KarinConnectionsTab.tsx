// Karin「连接」：和 Bot 连接页同一套卡片 + Dialog 编辑 + 底栏新增。

import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { Pencil, Plus, Trash2 } from 'lucide-react';
import {
    Badge,
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    FormSection,
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from '../../../../shared/ui';
import { ActionMotionIcon, EMPHASIS_MOTION } from '../../../../shared/ui/motion';
import { KarinConnectionEditor, kindTitle, type KarinConnDraft } from './KarinConnectionEditor';
import {
    ADDABLE,
    KIND_BADGE,
    blankForwardWs,
    blankOneBotHttp,
    listKarinConnections,
    type KarinConnRow,
} from './karinConnectionsModel';
import type { KarinTabProps } from './KarinBasicTab';

const ADD_DOCK_ID = 'karin-connections-add-dock';

type Editing =
    | { type: 'edit'; kind: 'webui' | 'reverseWs' | 'console' }
    | { type: 'edit'; kind: 'forwardWs' | 'onebotHttp'; idx: number }
    | { type: 'create'; kind: 'forwardWs' | 'onebotHttp' }
    | null;

interface DeleteTarget {
    kind: 'forwardWs' | 'onebotHttp';
    idx: number;
    name: string;
}

export const KarinConnectionsTab: React.FC<KarinTabProps> = ({
    config,
    onChange,
    errors,
    linked,
    disabled,
}) => {
    const [editing, setEditing] = useState<Editing>(null);
    const [editingMount, setEditingMount] = useState<Editing>(null);
    const [deleteTarget, setDeleteTarget] = useState<DeleteTarget | null>(null);

    useEffect(() => {
        if (editing !== null) setEditingMount(editing);
    }, [editing]);

    const rows = listKarinConnections(config, linked);

    const startEdit = (row: KarinConnRow) => {
        if (disabled) return;
        if (row.kind === 'forwardWs' || row.kind === 'onebotHttp') {
            setEditing({ type: 'edit', kind: row.kind, idx: row.idx });
            return;
        }
        setEditing({ type: 'edit', kind: row.kind });
    };

    const applyDraft = (next: KarinConnDraft) => {
        if (next.kind === 'webui') {
            onChange({ ...config, env: { ...config.env, ...next.env } });
        } else if (next.kind === 'reverseWs') {
            onChange({
                ...config,
                env: { ...config.env, ws_server_auth_key: next.envKey },
                adapter: {
                    ...config.adapter,
                    onebot: { ...config.adapter.onebot, ws_server: next.ws_server },
                },
            });
        } else if (next.kind === 'console') {
            onChange({ ...config, adapter: { ...config.adapter, console: next.console } });
        } else if (next.kind === 'forwardWs') {
            const list = config.adapter.onebot.ws_client.slice();
            if (editing?.type === 'create') list.push(next.row);
            else if (editing?.type === 'edit' && editing.kind === 'forwardWs') list[editing.idx] = next.row;
            onChange({
                ...config,
                adapter: { ...config.adapter, onebot: { ...config.adapter.onebot, ws_client: list } },
            });
        } else {
            const list = config.adapter.onebot.http_server.slice();
            if (editing?.type === 'create') list.push(next.row);
            else if (editing?.type === 'edit' && editing.kind === 'onebotHttp') list[editing.idx] = next.row;
            onChange({
                ...config,
                adapter: { ...config.adapter, onebot: { ...config.adapter.onebot, http_server: list } },
            });
        }
        setEditing(null);
    };

    const confirmDelete = () => {
        if (!deleteTarget) return;
        if (deleteTarget.kind === 'forwardWs') {
            const ws_client = config.adapter.onebot.ws_client.filter((_, i) => i !== deleteTarget.idx);
            onChange({
                ...config,
                adapter: { ...config.adapter, onebot: { ...config.adapter.onebot, ws_client } },
            });
        } else {
            const http_server = config.adapter.onebot.http_server.filter((_, i) => i !== deleteTarget.idx);
            onChange({
                ...config,
                adapter: { ...config.adapter, onebot: { ...config.adapter.onebot, http_server } },
            });
        }
        setDeleteTarget(null);
    };

    const draft = editingMount ? draftFromEditing(config, editingMount) : null;

    return (
        <>
            <div className="flex flex-col gap-8 pb-2">
                <FormSection
                    title="已配置的连接"
                    actions={<span className="text-2xs text-text-tertiary">{rows.length} 项</span>}
                >
                    <div className="flex flex-col gap-2">
                        {rows.map((row) => (
                            <ConnectionRow
                                key={row.key}
                                row={row}
                                disabled={disabled}
                                onStartEdit={() => startEdit(row)}
                                onDelete={
                                    row.kind === 'forwardWs' || row.kind === 'onebotHttp'
                                        ? () =>
                                              setDeleteTarget({
                                                  kind: row.kind,
                                                  idx: row.idx,
                                                  name: row.title,
                                              })
                                        : undefined
                                }
                            />
                        ))}
                    </div>
                </FormSection>
            </div>

            <AddBarPortal
                disabled={!!disabled}
                onPick={(kind) => setEditing({ type: 'create', kind })}
            />

            <Dialog
                open={editing !== null}
                onOpenChange={(o) => {
                    if (!o) setEditing(null);
                }}
            >
                <DialogContent
                    size="lg"
                    dismissOnOutsideClick={true}
                    onExited={() => setEditingMount(null)}
                    onPointerDownOutside={(e) => {
                        const target = e.target as HTMLElement;
                        if (target.hasAttribute('data-dialog-overlay')) e.preventDefault();
                    }}
                    onInteractOutside={(e) => {
                        const target = e.target as HTMLElement;
                        if (target.hasAttribute('data-dialog-overlay')) e.preventDefault();
                    }}
                >
                    {editingMount && draft && (
                        <>
                            <DialogHeader>
                                <DialogTitle>
                                    {editingMount.type === 'create' ? '新增' : '编辑'} {kindTitle(editingMount.kind)}
                                </DialogTitle>
                            </DialogHeader>
                            <KarinConnectionEditor
                                draft={draft}
                                linked={linked}
                                fieldErrors={errors}
                                onSave={applyDraft}
                                onCancel={() => setEditing(null)}
                            />
                        </>
                    )}
                </DialogContent>
            </Dialog>

            <Dialog open={deleteTarget !== null} onOpenChange={(o) => !o && setDeleteTarget(null)}>
                <DialogContent size="sm">
                    <DialogHeader>
                        <DialogTitle>删除连接？</DialogTitle>
                        <DialogDescription>
                            即将删除连接 “{deleteTarget?.name}”，此操作不可撤销。
                        </DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                        <Button variant="ghost" size="sm" onClick={() => setDeleteTarget(null)}>
                            取消
                        </Button>
                        <Button variant="danger" size="sm" onClick={confirmDelete}>
                            确认删除
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </>
    );
};

function draftFromEditing(config: KarinInstanceConfigLike, editing: Exclude<Editing, null>): KarinConnDraft {
    if (editing.kind === 'webui') {
        return {
            kind: 'webui',
            env: {
                http_enable: config.env.http_enable,
                http_port: config.env.http_port,
                http_host: config.env.http_host,
                http_auth_key: config.env.http_auth_key,
            },
        };
    }
    if (editing.kind === 'reverseWs') {
        return {
            kind: 'reverseWs',
            envKey: config.env.ws_server_auth_key,
            ws_server: { ...config.adapter.onebot.ws_server },
        };
    }
    if (editing.kind === 'console') {
        return { kind: 'console', console: { ...config.adapter.console } };
    }
    if (editing.type === 'create') {
        return editing.kind === 'forwardWs'
            ? { kind: 'forwardWs', row: blankForwardWs() }
            : { kind: 'onebotHttp', row: blankOneBotHttp() };
    }
    if (editing.type === 'edit' && editing.kind === 'forwardWs') {
        const row = config.adapter.onebot.ws_client[editing.idx];
        return { kind: 'forwardWs', row: row ? { ...row } : blankForwardWs() };
    }
    if (editing.type === 'edit' && editing.kind === 'onebotHttp') {
        const row = config.adapter.onebot.http_server[editing.idx];
        return { kind: 'onebotHttp', row: row ? { ...row } : blankOneBotHttp() };
    }
    return { kind: 'forwardWs', row: blankForwardWs() };
}

type KarinInstanceConfigLike = KarinTabProps['config'];

function ConnectionRow({
    row,
    disabled,
    onStartEdit,
    onDelete,
}: {
    row: KarinConnRow;
    disabled?: boolean;
    onStartEdit: () => void;
    onDelete?: () => void;
}) {
    return (
        <div className="group rounded-md border border-border-subtle/80 bg-field/40 transition-all hover:border-border hover:bg-field/70">
            <div className="flex items-center justify-between gap-3 px-3.5 py-2.5">
                <button
                    type="button"
                    disabled={disabled}
                    onClick={onStartEdit}
                    className="flex min-w-0 flex-1 flex-col gap-1 text-left cursor-pointer select-none disabled:cursor-not-allowed"
                >
                    <div className="flex min-w-0 items-center gap-2">
                        <Badge tone="info" appearance="soft" className="shrink-0 font-mono text-[11px]">
                            {KIND_BADGE[row.kind]}
                        </Badge>
                        <span className="truncate text-sm font-semibold text-text">{row.title}</span>
                        {row.kind === 'reverseWs' && row.linked && (
                            <Badge tone="brand" appearance="soft" className="shrink-0">
                                对接
                            </Badge>
                        )}
                        {row.kind === 'console' ? (
                            row.localOnly ? (
                                <Badge tone="neutral" appearance="soft" className="shrink-0">
                                    仅本机
                                </Badge>
                            ) : (
                                <Badge tone="info" appearance="soft" className="shrink-0">
                                    开放
                                </Badge>
                            )
                        ) : row.enable ? (
                            <Badge tone="success" appearance="soft" dot className="shrink-0">
                                启用
                            </Badge>
                        ) : (
                            <Badge tone="neutral" appearance="soft" className="shrink-0">
                                禁用
                            </Badge>
                        )}
                    </div>
                    <div className="flex items-center gap-2 font-mono text-2xs text-text-tertiary">
                        <span className="truncate text-text-secondary" title={row.summary}>
                            {row.summary}
                        </span>
                    </div>
                </button>
                <div className="flex shrink-0 items-center gap-1 opacity-80 transition-opacity group-hover:opacity-100">
                    <Tooltip>
                        <TooltipTrigger asChild>
                            <Button variant="ghost" size="icon" className="h-7 w-7" disabled={disabled} onClick={onStartEdit}>
                                <ActionMotionIcon icon={Pencil} size={13} strokeWidth={2.2} />
                            </Button>
                        </TooltipTrigger>
                        <TooltipContent>编辑</TooltipContent>
                    </Tooltip>
                    {onDelete && (
                        <Tooltip>
                            <TooltipTrigger asChild>
                                <Button
                                    variant="ghost"
                                    size="icon"
                                    className="h-7 w-7 text-danger hover:text-danger"
                                    disabled={disabled}
                                    onClick={onDelete}
                                >
                                    <ActionMotionIcon icon={Trash2} size={13} strokeWidth={2.2} />
                                </Button>
                            </TooltipTrigger>
                            <TooltipContent>删除连接</TooltipContent>
                        </Tooltip>
                    )}
                </div>
            </div>
        </div>
    );
}

function AddBarPortal({
    disabled,
    onPick,
}: {
    disabled: boolean;
    onPick: (kind: 'forwardWs' | 'onebotHttp') => void;
}) {
    const [dock, setDock] = useState<HTMLElement | null>(null);
    useEffect(() => {
        setDock(document.getElementById(ADD_DOCK_ID));
    }, []);
    if (!dock) return null;
    return createPortal(
        <div className="flex justify-center px-6 pb-3">
            <div className="inline-flex items-center gap-1 rounded-pill bg-elevated/95 px-2 py-1 shadow-popover ring-1 ring-border-subtle backdrop-blur-sm">
                <span className="px-1 text-2xs font-medium uppercase tracking-wide text-text-tertiary">
                    新增
                </span>
                {ADDABLE.map((meta) => (
                    <Button
                        key={meta.kind}
                        variant="ghost"
                        size="sm"
                        disabled={disabled}
                        onClick={() => onPick(meta.kind)}
                    >
                        <ActionMotionIcon icon={Plus} size={12} strokeWidth={2.4} motion={EMPHASIS_MOTION} />
                        <span>{meta.title}</span>
                    </Button>
                ))}
            </div>
        </div>,
        dock,
    );
}
