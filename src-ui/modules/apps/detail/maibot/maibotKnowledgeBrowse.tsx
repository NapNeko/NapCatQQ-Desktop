// 知识库「浏览」：按类型看记下的段落、实体、关系、事实，或按来源看（撤销一次导入就在这）；
// 搜关键词、点开看来龙去脉；删之前先预览连带删掉什么，删错了到「最近删除」里恢复。

import { useState, type ReactNode } from 'react';
import { Brain, FileText, History, Trash2, Undo2 } from 'lucide-react';
import {
    Badge,
    Button,
    Checkbox,
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    Spinner,
} from '../../../../shared/ui';
import type { MaiBotMemoryRecordKind } from '../../../../core/ipc/types';
import {
    useMaiBotMemoryDelete,
    useMaiBotMemoryDeleteOps,
    useMaiBotMemoryRecord,
    useMaiBotMemoryRecords,
    useMaiBotMemorySources,
} from '../../../../hooks/apps/useMaiBotMemory';
import { EmptyHint } from '../entityParts';
import {
    ResourcePane,
    SearchBox,
    Segmented,
    SelectionBar,
    useDebounced,
    useSelection,
} from '../resourceParts';
import { relativeTime } from './maibotPromptParts';
import {
    KIND_LABEL,
    RecordDetailDialog,
    RecordRow,
    deletableKind,
    describeCounts,
    useMemoryDelete,
} from './maibotKnowledgeParts';

type BrowseKind = 'all' | MaiBotMemoryRecordKind | 'source';
const LIMIT = 100;

export const KnowledgeBrowse: React.FC<{
    instanceId: string;
    switcher: ReactNode;
    onImport: () => void;
}> = ({ instanceId, switcher, onImport }) => {
    const [kind, setKind] = useState<BrowseKind>('all');
    const [search, setSearch] = useState('');
    const [showDeleted, setShowDeleted] = useState(false);
    const [open, setOpen] = useState<{ kind: MaiBotMemoryRecordKind; id: string } | null>(null);
    const [opsOpen, setOpsOpen] = useState(false);
    const sel = useSelection<string>();
    const q = useDebounced(search.trim());
    const isSource = kind === 'source';

    const records = useMaiBotMemoryRecords(
        instanceId,
        {
            search: q,
            kinds: kind === 'all' || isSource ? [] : [kind],
            include_inactive: showDeleted,
            limit: LIMIT,
        },
        !isSource,
    );
    const sources = useMaiBotMemorySources(instanceId, isSource);
    const detail = useMaiBotMemoryRecord(instanceId, open?.kind ?? 'paragraph', open?.id ?? null);
    const del = useMemoryDelete(instanceId, () => {
        sel.clear();
        setOpen(null);
    });

    const items = records.data?.items ?? [];
    const sourceItems = (sources.data ?? []).filter(
        (s) => !q || s.source.toLowerCase().includes(q.toLowerCase()),
    );
    const batchKind = kind !== 'all' && kind !== 'source' && deletableKind(kind) ? kind : null;
    const pageIds = items.filter((r) => r.active).map((r) => r.id);
    const pageAllPicked = pageIds.length > 0 && pageIds.every((id) => sel.has(id));
    const switchKind = (k: BrowseKind) => {
        setKind(k);
        sel.clear();
    };

    const toolbar = (
        <>
            {switcher}
            <SearchBox
                className="w-44"
                placeholder={isSource ? '搜来源' : '搜记忆'}
                value={search}
                onChange={setSearch}
            />
            <Segmented
                items={[
                    { value: 'all', label: '全部' },
                    { value: 'paragraph', label: '段落' },
                    { value: 'entity', label: '实体' },
                    { value: 'relation', label: '关系' },
                    { value: 'fact', label: '事实' },
                    { value: 'source', label: '来源' },
                ]}
                value={kind}
                onChange={switchKind}
            />
            <span className="flex-1" />
            {!isSource && (
                <Checkbox label="含已删的" checked={showDeleted} onCheckedChange={setShowDeleted} />
            )}
            <Button size="sm" variant="ghost" onClick={() => setOpsOpen(true)}>
                <History size={13} />
                最近删除
            </Button>
        </>
    );

    const loading = isSource
        ? sources.isFetching && !sources.data
        : records.isFetching && !records.data;
    const empty = isSource ? sourceItems.length === 0 : items.length === 0;
    const count = isSource ? sourceItems.length : items.length;

    return (
        <ResourcePane
            toolbar={toolbar}
            footer={
                <p className="text-xs text-text-tertiary">
                    共 {count} {isSource ? '个来源' : '条'}
                    {!isSource && count >= LIMIT && '，只列最近的，搜一下能找到更早的'}
                </p>
            }
            overlay={
                batchKind && (
                    <SelectionBar
                        count={sel.picked.size}
                        onClear={sel.clear}
                        onSelectAll={pageAllPicked ? undefined : () => sel.setAll(pageIds, true)}
                    >
                        <Button
                            size="sm"
                            variant="ghost"
                            className="text-danger hover:bg-danger-soft hover:text-danger"
                            disabled={del.busy}
                            onClick={() =>
                                del.start(
                                    { kind: batchKind, ids: [...sel.picked] },
                                    `这 ${sel.picked.size} 条${KIND_LABEL[batchKind]}`,
                                )
                            }
                        >
                            <Trash2 size={13} />
                            删除
                        </Button>
                    </SelectionBar>
                )
            }
        >
            {loading ? (
                <div className="flex h-40 items-center justify-center">
                    <Spinner size="md" tone="brand" label="正在读取" />
                </div>
            ) : empty ? (
                <EmptyHint
                    icon={Brain}
                    title={
                        q || kind !== 'all'
                            ? '没有对得上的记忆'
                            : '长期记忆里还没有东西。导点资料进来，或者多聊一阵，麦麦会自己记。'
                    }
                    action={
                        !q && kind === 'all' ? (
                            <Button size="sm" variant="secondary" onClick={onImport}>
                                去导入
                            </Button>
                        ) : undefined
                    }
                />
            ) : isSource ? (
                <div className="flex flex-col gap-1.5">
                    {sourceItems.map((s) => (
                        <div
                            key={s.source}
                            className="group flex items-center gap-3 rounded-md border border-border-subtle bg-surface px-3 py-2.5 hover:border-border"
                        >
                            <span className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-sm bg-inset text-text-secondary">
                                <FileText size={14} />
                            </span>
                            <span className="min-w-0 flex-1">
                                <span className="block truncate text-[13.5px] text-text">
                                    {s.source}
                                </span>
                                <span className="mt-0.5 block text-xs text-text-tertiary">
                                    {s.paragraphs} 段
                                    {s.last_updated ? ` · ${relativeTime(s.last_updated)}` : ''}
                                </span>
                            </span>
                            <Button
                                size="sm"
                                variant="ghost"
                                className="text-danger opacity-0 transition-opacity hover:bg-danger-soft hover:text-danger focus-visible:opacity-100 group-hover:opacity-100"
                                disabled={del.busy}
                                onClick={() =>
                                    del.start(
                                        { kind: 'source', ids: [s.source] },
                                        `「${s.source}」下的 ${s.paragraphs} 段`,
                                    )
                                }
                            >
                                <Trash2 size={13} />
                                整批删掉
                            </Button>
                        </div>
                    ))}
                </div>
            ) : (
                <div className="flex flex-col gap-1.5">
                    {items.map((r) => (
                        <RecordRow
                            key={`${r.kind}:${r.id}`}
                            record={r}
                            selectable={!!batchKind && r.active}
                            selected={sel.has(r.id)}
                            busy={del.busy}
                            onPick={(shift) => sel.pick(r.id, shift, pageIds)}
                            onOpen={() => setOpen({ kind: r.kind, id: r.id })}
                            onDelete={() =>
                                deletableKind(r.kind) &&
                                del.start(
                                    { kind: r.kind, ids: [r.id] },
                                    `这条${KIND_LABEL[r.kind]}`,
                                )
                            }
                        />
                    ))}
                </div>
            )}

            <RecordDetailDialog
                open={open !== null}
                detail={detail.data}
                loading={detail.isFetching}
                busy={del.busy}
                onOpen={(r) => setOpen({ kind: r.kind, id: r.id })}
                onDelete={(r) =>
                    deletableKind(r.kind) &&
                    del.start({ kind: r.kind, ids: [r.id] }, `这条${KIND_LABEL[r.kind]}`)
                }
                onClose={() => setOpen(null)}
            />
            {del.confirm}
            <DeleteOpsDialog
                instanceId={instanceId}
                open={opsOpen}
                onClose={() => setOpsOpen(false)}
            />
        </ResourcePane>
    );
};

const MODE_LABEL: Readonly<Record<string, string>> = {
    paragraph: '段落',
    entity: '实体',
    relation: '关系',
    source: '来源',
    mixed: '一批',
};

/** 最近删掉的，一次删除一行；段落、实体是软删，过一天左右上游会真的清掉 */
const DeleteOpsDialog: React.FC<{ instanceId: string; open: boolean; onClose: () => void }> = ({
    instanceId,
    open,
    onClose,
}) => {
    const ops = useMaiBotMemoryDeleteOps(instanceId, open);
    const restore = useMaiBotMemoryDelete(instanceId);
    const list = ops.data ?? [];
    return (
        <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
            <DialogContent size="md">
                <DialogHeader>
                    <DialogTitle>最近删除</DialogTitle>
                    <p className="text-xs text-text-tertiary">
                        删掉的段落、实体过一天左右会被清掉，之前都能恢复。
                    </p>
                </DialogHeader>
                {ops.isFetching && !ops.data ? (
                    <div className="flex h-32 items-center justify-center">
                        <Spinner size="md" tone="brand" label="正在读取" />
                    </div>
                ) : list.length === 0 ? (
                    <p className="py-8 text-center text-sm text-text-tertiary">最近没删过东西</p>
                ) : (
                    <ul className="flex max-h-[50vh] flex-col gap-1.5 overflow-y-auto pr-1">
                        {list.map((op) => {
                            const restored = op.status === 'restored' || !!op.restored_at;
                            return (
                                <li
                                    key={op.id}
                                    className="flex items-center gap-3 rounded-md bg-inset/60 px-3 py-2"
                                >
                                    <span className="min-w-0 flex-1">
                                        <span className="block truncate text-[13px] text-text">
                                            按{MODE_LABEL[op.mode] ?? op.mode}删了{' '}
                                            {describeCounts(op.counts)}
                                        </span>
                                        <span className="block text-2xs text-text-tertiary">
                                            {relativeTime(op.created_at)}
                                        </span>
                                    </span>
                                    {restored ? (
                                        <Badge tone="neutral">已恢复</Badge>
                                    ) : (
                                        <Button
                                            size="sm"
                                            variant="ghost"
                                            disabled={restore.isPending}
                                            onClick={() =>
                                                restore.mutate({
                                                    op: 'restore',
                                                    operation_id: op.id,
                                                })
                                            }
                                        >
                                            <Undo2 size={13} />
                                            恢复
                                        </Button>
                                    )}
                                </li>
                            );
                        })}
                    </ul>
                )}
            </DialogContent>
        </Dialog>
    );
};
