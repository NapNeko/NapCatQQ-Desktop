import { useState } from 'react';
import { Button, TextAreaField, TextField } from '../../../../shared/ui';
import { ExpandChevron, ExpandPresence } from '../../../../shared/ui/motion';
import { asRecord } from '../../../../core/domain/apps/neobotPanel';
import {
    NeoBotPanelError,
    paramsPath,
    record,
    records,
    strings,
    text,
    type PanelObject,
} from '../../../../core/domain/apps/neobotWorkspace';
import { usePanelJson } from '../../../../hooks/apps/useNeoBotPanel';
import { useNeoBotAction } from '../../../../hooks/apps/useNeoBotAction';
import { useNeoBotDraftState } from '../../../../hooks/apps/useNeoBotDraftState';
import { panelQueryState } from '../../../../core/domain/apps/neobotPanelState';
import { PanelSectionState } from './PanelStateView';
import {
    ConfirmAction,
    FullText,
    InlineNote,
    PanelPage,
    PanelSection,
    PanelTable,
    type NeoBotPageProps,
} from './workspaceParts';

interface ArchiveProps extends NeoBotPageProps {
    table: string;
    itemKey: string;
    canDelete: boolean;
    canCompress: boolean;
    onCompress: () => void;
}

function ItemEditor({ doc, ...props }: ArchiveProps & { doc: PanelObject }) {
    const { instanceId, table, itemKey } = props;
    const action = useNeoBotAction(instanceId);
    const scope = `archive:${table}/${itemKey}`;
    const [base, setBase] = useNeoBotDraftState(`${scope}:base`, doc);
    const [value, setValue] = useNeoBotDraftState(`${scope}:value`, text(doc.value));
    const [tags, setTags] = useNeoBotDraftState(`${scope}:tags`, strings(doc.tags).join(', '));
    const [deleted, setDeleted] = useState(false);
    const [showHistory, setShowHistory] = useState(false);
    const [snapshotId, setSnapshotId] = useState('');
    const snapshotsPath = paramsPath('/api/archives/snapshots', { table, key: itemKey });
    const snapshots = usePanelJson(instanceId, snapshotsPath, snapshotsPath, asRecord, showHistory);
    const snapshotPath = paramsPath('/api/archives/snapshot', { id: snapshotId });
    const snapshot = usePanelJson(instanceId, snapshotPath, snapshotPath, asRecord, !!snapshotId);
    const current =
        action.error instanceof NeoBotPanelError ? record(record(action.error.data).current) : {};
    const apply = (next: PanelObject) => {
        setBase(next);
        setValue(text(next.value));
        setTags(strings(next.tags).join(', '));
        action.reset();
    };
    const dirty = value !== text(base.value) || tags !== strings(base.tags).join(', ');
    if (deleted) return <p className="text-xs text-text-secondary">档案已删除</p>;
    return (
        <PanelSection
            title={`${table} / ${itemKey}`}
            meta={dirty ? '有未保存的修改' : base.editable === true ? '可编辑' : '只读'}
            actions={
                <>
                    <ConfirmAction
                        label="读取最新内容"
                        description="放弃当前编辑草稿并读取最新版本？"
                        disabled={action.isPending}
                        onConfirm={() =>
                            void action
                                .run({
                                    path: paramsPath('/api/archives/item', { table, key: itemKey }),
                                    method: 'GET',
                                    quiet: true,
                                })
                                .then((next) => {
                                    if (next) apply(next);
                                })
                        }
                    />
                    <ConfirmAction
                        label="删除档案"
                        variant="danger"
                        description="永久删除该档案，无法恢复。"
                        disabled={!props.canDelete || action.isPending}
                        onConfirm={() =>
                            void action
                                .run({
                                    path: '/api/archives/item',
                                    method: 'DELETE',
                                    body: { table, key: itemKey, version: base.version },
                                })
                                .then((next) => {
                                    if (next) setDeleted(true);
                                })
                        }
                    />
                </>
            }
        >
            {base.note ? (
                <div className="mb-4">
                    <InlineNote tone="warning">{text(base.note)}</InlineNote>
                </div>
            ) : null}
            <TextAreaField
                label="完整内容"
                value={value}
                minRows={12}
                maxRows={30}
                disabled={base.editable !== true || action.isPending}
                onValueChange={setValue}
            />
            <TextField
                className="mt-3"
                label="标签（逗号分隔）"
                value={tags}
                disabled={base.editable !== true || action.isPending}
                onValueChange={setTags}
            />
            <div className="mt-4 flex flex-wrap gap-2">
                <Button
                    size="sm"
                    variant="primary"
                    disabled={base.editable !== true || !dirty || !value.trim() || action.isPending}
                    onClick={() =>
                        void action
                            .run({
                                path: '/api/archives/item',
                                method: 'PUT',
                                body: {
                                    table,
                                    key: itemKey,
                                    version: base.version,
                                    value,
                                    tags: tags
                                        .split(/[,，]/)
                                        .map((s) => s.trim())
                                        .filter(Boolean),
                                },
                            })
                            .then((next) => {
                                if (next) apply(next);
                            })
                    }
                >
                    保存档案
                </Button>
                <ConfirmAction
                    label="AI 压缩此条"
                    description="调用模型压缩此条档案，压缩前会保存快照并产生模型用量。未保存草稿不参与压缩。"
                    disabled={!props.canCompress || dirty || action.isPending}
                    onConfirm={props.onCompress}
                />
                <Button
                    size="sm"
                    variant="secondary"
                    aria-expanded={showHistory}
                    onClick={() => setShowHistory((value) => !value)}
                >
                    压缩快照
                    <ExpandChevron open={showHistory} size={12} />
                </Button>
            </div>
            {Object.keys(current).length > 0 && (
                <div className="mt-4">
                    <div className="mb-3">
                        <InlineNote tone="warning">
                            服务器版本已变化，草稿已保留，可先对照最新内容
                        </InlineNote>
                    </div>
                    <FullText label="冲突时服务器内容" value={text(current.value)} />
                </div>
            )}
            <ExpandPresence visible={showHistory}>
                <div className="mt-4">
                    {snapshots.data?.kind === 'ok' ? (
                        <PanelTable
                            rows={records(snapshots.data.data.items)}
                            columns={[
                                { key: 'id', label: '快照', mono: true },
                                { key: 'created_at', label: '时间' },
                                { key: 'chars_before', label: '压缩前字数', align: 'right' },
                                { key: 'chars_after', label: '压缩后字数', align: 'right' },
                            ]}
                            selectedKey={snapshotId}
                            rowKey={(s) => text(s.id)}
                            onRowClick={(s) => setSnapshotId(text(s.id))}
                            empty="还没有压缩快照，首次压缩后会保留一份"
                        />
                    ) : (
                        <PanelSectionState
                            state={panelQueryState(snapshots)}
                            onRetry={() => void snapshots.refetch()}
                        />
                    )}
                </div>
            </ExpandPresence>
            {snapshotId && (
                <div className="mt-4">
                    {snapshot.data?.kind === 'ok' ? (
                        <FullText
                            label="快照全文"
                            value={record(snapshot.data.data.snapshot).value}
                        />
                    ) : (
                        <PanelSectionState
                            state={panelQueryState(snapshot)}
                            onRetry={() => void snapshot.refetch()}
                        />
                    )}
                </div>
            )}
        </PanelSection>
    );
}

export function ArchiveEditor(props: ArchiveProps) {
    const path = paramsPath('/api/archives/item', { table: props.table, key: props.itemKey });
    const query = usePanelJson(props.instanceId, path, path, asRecord);
    return (
        <PanelPage query={query} onGoTab={props.onGoTab}>
            {(doc) => (
                <ItemEditor
                    key={`${props.instanceId}:${props.table}:${props.itemKey}`}
                    {...props}
                    doc={doc}
                />
            )}
        </PanelPage>
    );
}
