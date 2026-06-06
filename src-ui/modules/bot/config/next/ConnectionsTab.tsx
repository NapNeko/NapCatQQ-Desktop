// 协议连接通道列表 + Dialog 弹窗编辑 + 底部浮动新增条。
//
// UX 决策：
//   - 列表占满 tab 主体；点行内"编辑" / 底部 chip 都走 Dialog 弹窗
//   - 底部新增条通过 portal 挂到 BotConfigPage 的 #connections-dock，永远贴底
//   - 删除走 destructive Dialog 二次确认

import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { Plus, Trash2, Pencil, Lock } from 'lucide-react';
import {
    ActionMotionIcon,
    EMPHASIS_MOTION,
} from '../../../../shared/ui/motion';
import {
    Button,
    Badge,
    Tooltip,
    TooltipProvider,
    TooltipTrigger,
    TooltipContent,
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
    FormSection,
} from '../../../../shared/ui';
import {
    CONNECTION_KINDS,
    CONNECTION_GROUP_KEY,
    type ConnectionKind,
    type ConnectionConfig,
    createDefaultConnection,
    summarizeConnection,
    collectAllNames,
    getKindMeta,
} from '../../../../core/domain/bot/connections';
import type { ConnectConfig } from '../../../../core/ipc/generated/domain/ConnectConfig';
import type { BackendType } from '../../../../core/ipc/generated/domain/BackendType';
import { ConnectionEditor } from './ConnectionEditor';

interface ConnectionsTabProps {
    data: ConnectConfig;
    onChange: (patch: Partial<ConnectConfig>) => void;
    backendType: BackendType;
}

type EditingKey =
    | { type: 'edit'; kind: ConnectionKind; idx: number; draft: ConnectionConfig }
    | { type: 'create'; kind: ConnectionKind; draft: ConnectionConfig }
    | null;

interface DeleteTarget {
    kind: ConnectionKind;
    idx: number;
    name: string;
}

const KIND_BADGE: Record<ConnectionKind, string> = {
    httpServer: 'HTTP',
    httpSseServer: 'SSE',
    httpClient: 'Webhook',
    websocketServer: 'WS-Server',
    websocketClient: 'WS-Client',
};

export function ConnectionsTab({ data, onChange, backendType }: ConnectionsTabProps) {
    const [editing, setEditing] = useState<EditingKey>(null);
    const [deleteTarget, setDeleteTarget] = useState<DeleteTarget | null>(null);

    const startCreate = (kind: ConnectionKind) => {
        const draft = createDefaultConnection(kind);
        setEditing({ type: 'create', kind, draft });
    };

    const startEdit = (kind: ConnectionKind, idx: number) => {
        const list = data[CONNECTION_GROUP_KEY[kind]] as ConnectionConfig[];
        const item = list[idx];
        if (!item) return;
        setEditing({ type: 'edit', kind, idx, draft: item });
    };

    const cancelEdit = () => setEditing(null);

    const saveEdit = (next: ConnectionConfig) => {
        if (!editing) return;
        const groupKey = CONNECTION_GROUP_KEY[editing.kind];
        const list = (data[groupKey] as ConnectionConfig[]).slice();
        if (editing.type === 'create') {
            list.push(next);
        } else {
            list[editing.idx] = next;
        }
        onChange({ [groupKey]: list } as Partial<ConnectConfig>);
        setEditing(null);
    };

    const confirmDelete = () => {
        if (!deleteTarget) return;
        const groupKey = CONNECTION_GROUP_KEY[deleteTarget.kind];
        const list = (data[groupKey] as ConnectionConfig[]).slice();
        list.splice(deleteTarget.idx, 1);
        onChange({ [groupKey]: list } as Partial<ConnectConfig>);
        setDeleteTarget(null);
    };

    const total = totalCount(data);

    // 编辑态下 existingNames 不能含自己，否则保存时会自校验"重名"
    const existingNamesForEdit =
        editing?.type === 'edit'
            ? collectAllNames(data).filter((n) => n !== editing.draft.name)
            : collectAllNames(data);

    return (
        <TooltipProvider delayDuration={200}>
            {total === 0 ? (
                // 空态：磁贴上下左右居中在剩余空间里
                <div className="flex min-h-[60vh] flex-1 items-center justify-center">
                    <div className="w-full max-w-xs rounded-md border border-dashed border-border-subtle bg-canvas/60 px-5 py-6 text-center">
                        <p className="text-sm font-medium text-text-secondary">
                            暂无任何协议连接
                        </p>
                        <p className="mt-1 text-2xs text-text-tertiary leading-relaxed">
                            Bot 启动后将无法与外部 OneBot 客户端通信，
                            <br />
                            请通过下方“新增”添加一个
                        </p>
                    </div>
                </div>
            ) : (
                <div className="flex flex-col gap-8 pb-2">
                    <FormSection
                        title="已配置的连接"
                        description="Bot 启动后这些通道会同时开启；可以多类型组合"
                        actions={
                            <span className="text-2xs text-text-tertiary">{total} 项</span>
                        }
                    >
                        <ConnectionList
                            data={data}
                            onStartEdit={startEdit}
                            onDelete={(kind, idx, name) => setDeleteTarget({ kind, idx, name })}
                        />
                    </FormSection>
                </div>
            )}

            {/* 浮动新增条：portal 到 BotConfigPage 提供的 dock，永远贴底 */}
            <FloatingAddBarPortal backendType={backendType} onPick={startCreate} />

            {/* 新建 / 编辑 共用同一个 Dialog */}
            <Dialog
                open={editing !== null}
                onOpenChange={(o) => {
                    if (!o) cancelEdit();
                }}
            >
                <DialogContent
                    className="max-w-2xl"
                    onEscapeKeyDown={(e) => e.preventDefault()}
                    onPointerDownOutside={(e) => e.preventDefault()}
                >
                    {editing && (
                        <>
                            <DialogHeader>
                                <DialogTitle>
                                    {editing.type === 'create' ? '新增' : '编辑'}
                                    {' '}
                                    {getKindMeta(editing.kind).title}
                                </DialogTitle>
                                <DialogDescription>
                                    {getKindMeta(editing.kind).description}
                                </DialogDescription>
                            </DialogHeader>
                            <ConnectionEditor
                                kind={editing.kind}
                                initialData={editing.draft}
                                existingNames={existingNamesForEdit}
                                backendType={backendType}
                                onSave={saveEdit}
                                onCancel={cancelEdit}
                            />
                        </>
                    )}
                </DialogContent>
            </Dialog>

            {/* 删除二次确认 */}
            <Dialog open={deleteTarget !== null} onOpenChange={(o) => !o && setDeleteTarget(null)}>
                <DialogContent className="max-w-sm">
                    <DialogHeader>
                        <DialogTitle>删除连接？</DialogTitle>
                        <DialogDescription>
                            即将删除连接 "{deleteTarget?.name}"，此操作不可撤销。
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
        </TooltipProvider>
    );
}

function totalCount(c: ConnectConfig): number {
    return (
        c.httpServers.length +
        c.httpSseServers.length +
        c.httpClients.length +
        c.websocketServers.length +
        c.websocketClients.length
    );
}

interface FloatingAddBarProps {
    backendType: BackendType;
    onPick: (kind: ConnectionKind) => void;
}

function FloatingAddBarPortal({ backendType, onPick }: FloatingAddBarProps) {
    const [dock, setDock] = useState<HTMLElement | null>(null);
    useEffect(() => {
        setDock(document.getElementById('connections-add-dock'));
    }, []);
    if (!dock) return null;
    return createPortal(<FloatingAddBar backendType={backendType} onPick={onPick} />, dock);
}

function FloatingAddBar({ backendType, onPick }: FloatingAddBarProps) {
    return (
        <div className="flex justify-center px-6 pb-3">
            <div className="inline-flex items-center gap-1 rounded-pill bg-elevated/95 px-2 py-1 shadow-popover ring-1 ring-border-subtle backdrop-blur-sm">
                <span className="px-1 text-2xs font-medium uppercase tracking-wide text-text-tertiary">
                    新增
                </span>
                {CONNECTION_KINDS.map((meta) => {
                    const supported = meta.supportedBackends.includes(backendType);
                    const btn = (
                        <Button
                            key={meta.kind}
                            variant="ghost"
                            size="sm"
                            disabled={!supported}
                            onClick={() => supported && onPick(meta.kind)}
                        >
                            {supported ? (
                                <ActionMotionIcon
                                    icon={Plus}
                                    size={12}
                                    strokeWidth={2.4}
                                    motion={EMPHASIS_MOTION}
                                />
                            ) : (
                                <ActionMotionIcon icon={Lock} size={12} />
                            )}
                            <span>{meta.title}</span>
                        </Button>
                    );
                    if (supported) return btn;
                    return (
                        <Tooltip key={meta.kind}>
                            <TooltipTrigger asChild>
                                <span>{btn}</span>
                            </TooltipTrigger>
                            <TooltipContent>
                                当前底座（{backendType}）不支持此类型
                            </TooltipContent>
                        </Tooltip>
                    );
                })}
            </div>
        </div>
    );
}

interface ConnectionListProps {
    data: ConnectConfig;
    onStartEdit: (kind: ConnectionKind, idx: number) => void;
    onDelete: (kind: ConnectionKind, idx: number, name: string) => void;
}

function ConnectionList({ data, onStartEdit, onDelete }: ConnectionListProps) {
    return (
        <div className="flex flex-col gap-2">
            {CONNECTION_KINDS.flatMap((meta) =>
                (data[CONNECTION_GROUP_KEY[meta.kind]] as ConnectionConfig[]).map((item, idx) => (
                    <ConnectionRow
                        key={`${meta.kind}-${idx}-${item.name}`}
                        kind={meta.kind}
                        item={item}
                        onStartEdit={() => onStartEdit(meta.kind, idx)}
                        onDelete={() => onDelete(meta.kind, idx, item.name)}
                    />
                )),
            )}
        </div>
    );
}

interface ConnectionRowProps {
    kind: ConnectionKind;
    item: ConnectionConfig;
    onStartEdit: () => void;
    onDelete: () => void;
}

function ConnectionRow({ kind, item, onStartEdit, onDelete }: ConnectionRowProps) {
    const summary = summarizeConnection(kind, item);

    return (
        <div className="rounded-sm bg-field ring-1 ring-border-subtle">
            <div className="flex items-center justify-between gap-3 px-3 py-2.5">
                <button
                    type="button"
                    onClick={onStartEdit}
                    className="flex min-w-0 flex-1 items-center gap-2 text-left"
                >
                    <Badge tone="brand" appearance="soft" className="font-mono">
                        {KIND_BADGE[kind]}
                    </Badge>
                    <span className="truncate text-sm font-medium text-text">{item.name}</span>
                    {item.enable ? (
                        <Badge tone="success" appearance="soft" dot>
                            启用
                        </Badge>
                    ) : (
                        <Badge tone="neutral" appearance="soft">
                            禁用
                        </Badge>
                    )}
                    <span className="ml-1 truncate text-xs text-text-tertiary font-mono">
                        {summary}
                    </span>
                </button>
                <div className="flex shrink-0 items-center gap-1">
                    <Tooltip>
                        <TooltipTrigger asChild>
                            <Button
                                variant="ghost"
                                size="icon"
                                className="h-7 w-7"
                                onClick={onStartEdit}
                            >
                                <ActionMotionIcon icon={Pencil} size={13} strokeWidth={2.2} />
                            </Button>
                        </TooltipTrigger>
                        <TooltipContent>编辑</TooltipContent>
                    </Tooltip>
                    <Tooltip>
                        <TooltipTrigger asChild>
                            <Button
                                variant="ghost"
                                size="icon"
                                className="h-7 w-7 text-danger hover:text-danger"
                                onClick={onDelete}
                            >
                                <ActionMotionIcon icon={Trash2} size={13} strokeWidth={2.2} />
                            </Button>
                        </TooltipTrigger>
                        <TooltipContent>删除连接</TooltipContent>
                    </Tooltip>
                </div>
            </div>
        </div>
    );
}
