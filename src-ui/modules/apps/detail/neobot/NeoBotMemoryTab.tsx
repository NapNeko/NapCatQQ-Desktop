import { useState } from 'react';
import { Button, Checkbox, Select, TextField } from '../../../../shared/ui';
import { asRecord } from '../../../../core/domain/apps/neobotPanel';
import {
    paramsPath,
    records,
    text,
    type PanelObject,
} from '../../../../core/domain/apps/neobotWorkspace';
import { usePanelJson } from '../../../../hooks/apps/useNeoBotPanel';
import { useNeoBotAction } from '../../../../hooks/apps/useNeoBotAction';
import { useNeoBotDraftState } from '../../../../hooks/apps/useNeoBotDraftState';
import { ArchiveEditor } from './NeoBotArchiveEditor';
import { panelPageGate, panelQueryState } from '../../../../core/domain/apps/neobotPanelState';
import { PanelGateView, PanelSectionState } from './PanelStateView';
import {
    ConfirmAction,
    FieldList,
    PanelEmpty,
    PanelQuerySection,
    PanelSection,
    PanelTable,
    type NeoBotPageProps,
} from './workspaceParts';

export function NeoBotMemoryTab({ instanceId, onGoTab }: NeoBotPageProps) {
    const tables = usePanelJson(instanceId, 'archives', '/api/archives', asRecord);
    const action = useNeoBotAction(instanceId);
    const data = tables.data?.kind === 'ok' ? tables.data.data : {};
    const [table, setTable] = useNeoBotDraftState('memory:table', '');
    const activeTable = table || text(records(data.items)[0]?.table_name);
    const [filter, setFilter] = useState({ key: '', value: '', tags: '', over: false });
    const [applied, setApplied] = useState(filter);
    const [offset, setOffset] = useState(0);
    const [selected, setSelected] = useNeoBotDraftState<PanelObject | null>(
        'memory:selected',
        null,
    );
    const [task, setTask] = useNeoBotDraftState<PanelObject | null>('memory:task', null);
    const [target, setTarget] = useState('1000');
    const [overLimit, setOverLimit] = useState<PanelObject | null>(null);
    const listPath = paramsPath('/api/archives/items', {
        table: activeTable,
        key_query: applied.key,
        value_query: applied.value,
        tags: applied.tags,
        over_limit: applied.over ? 1 : 0,
        limit: 50,
        offset,
    });
    const items = usePanelJson(instanceId, listPath, listPath, asRecord, !!activeTable);
    const taskId = text(task?.task_id);
    const statusPath = paramsPath('/api/archives/summarize', { task_id: taskId });
    const status = usePanelJson(
        instanceId,
        statusPath,
        statusPath,
        asRecord,
        !!taskId,
        ['running', 'queued'].includes(text(task?.status)) ? 1500 : false,
    );
    const currentTask = status.data?.kind === 'ok' ? status.data.data : task;
    const running =
        !!taskId && (!currentTask || ['running', 'queued'].includes(text(currentTask.status)));
    const compress = async (key?: string) => {
        const next = await action.run({
            path: key ? '/api/archives/summarize' : '/api/archives/summarize/over-limit',
            body: { table: activeTable, ...(key ? { key } : {}), target_chars: Number(target) },
        });
        if (next) setTask(next);
    };
    const targetValid =
        Number.isInteger(Number(target)) &&
        Number(target) >= (Number(data.min_target_chars) || 100) &&
        Number(target) <= (Number(data.max_target_chars) || Number.MAX_SAFE_INTEGER);
    const queries = [tables, ...(activeTable ? [items] : []), ...(taskId ? [status] : [])];
    const gate = panelPageGate(queries.map(panelQueryState));
    if (gate)
        return (
            <PanelGateView
                gate={gate}
                onRetry={() => queries.forEach((query) => void query.refetch())}
            />
        );
    return (
        <div className="flex flex-col gap-4">
            <PanelQuerySection query={tables} title="记忆归档">
                {(archive) =>
                    records(archive.items).length === 0 ? (
                        <PanelEmpty>还没有记忆档案，先与机器人聊一会儿。</PanelEmpty>
                    ) : (
                        <>
                            <Select
                                label="档案表"
                                value={activeTable}
                                items={records(data.items).map((t) => ({
                                    value: text(t.table_name),
                                    label: `${text(t.table_name)} · ${text(t.count)} 条 · ${text(t.over_limit_count) || '0'} 条超限`,
                                }))}
                                onValueChange={(next) => {
                                    setTable(next);
                                    setOffset(0);
                                    setSelected(null);
                                }}
                            />
                            <div className="mt-4 grid gap-3 sm:grid-cols-3">
                                <TextField
                                    label="键名"
                                    value={filter.key}
                                    onValueChange={(key) => setFilter({ ...filter, key })}
                                />
                                <TextField
                                    label="内容"
                                    value={filter.value}
                                    onValueChange={(value) => setFilter({ ...filter, value })}
                                />
                                <TextField
                                    label="标签"
                                    value={filter.tags}
                                    onValueChange={(tags) => setFilter({ ...filter, tags })}
                                />
                            </div>
                            <div className="mt-3 flex flex-wrap items-center gap-3">
                                <Checkbox
                                    label="仅超限"
                                    checked={filter.over}
                                    onCheckedChange={(over) =>
                                        setFilter({ ...filter, over: over === true })
                                    }
                                />
                                <Button
                                    size="sm"
                                    variant="secondary"
                                    onClick={() => {
                                        setApplied(filter);
                                        setOffset(0);
                                        setSelected(null);
                                    }}
                                >
                                    查询
                                </Button>
                                <Button
                                    size="sm"
                                    variant="ghost"
                                    disabled={action.isPending}
                                    onClick={() =>
                                        void action
                                            .run({
                                                path: '/api/archives/over-limit',
                                                method: 'GET',
                                                quiet: true,
                                            })
                                            .then(setOverLimit)
                                    }
                                >
                                    超限清单
                                </Button>
                            </div>
                        </>
                    )
                }
            </PanelQuerySection>
            {activeTable && (
                <PanelQuerySection query={items} title={`条目 · 第 ${offset / 50 + 1} 页`}>
                    {(list) => (
                        <>
                            <div className="mb-3 flex justify-end gap-2">
                                <Button
                                    size="sm"
                                    variant="secondary"
                                    disabled={offset === 0}
                                    onClick={() => {
                                        setOffset(Math.max(0, offset - 50));
                                        setSelected(null);
                                    }}
                                >
                                    上一页
                                </Button>
                                <Button
                                    size="sm"
                                    variant="secondary"
                                    disabled={!list.has_more}
                                    onClick={() => {
                                        setOffset(offset + 50);
                                        setSelected(null);
                                    }}
                                >
                                    下一页
                                </Button>
                            </div>
                            <PanelTable
                                rows={records(list.items)}
                                rowKey={(item) => text(item.key)}
                                selectedKey={text(selected?.key)}
                                columns={[
                                    {
                                        key: 'key',
                                        label: '键名',
                                        render: (row) => (
                                            <Button
                                                size="sm"
                                                variant="ghost"
                                                onClick={() => setSelected(row)}
                                            >
                                                {text(row.key)}
                                            </Button>
                                        ),
                                    },
                                    { key: 'preview', label: '内容预览' },
                                    { key: 'total_chars', label: '字数', align: 'right' },
                                    { key: 'tags', label: '标签' },
                                ]}
                                empty="没有符合条件的记忆，调整上方筛选后重新查询。"
                            />
                        </>
                    )}
                </PanelQuerySection>
            )}
            {activeTable && data.summarize_available === true && (
                <PanelSection title="AI 压缩">
                    <div className="flex flex-wrap items-end gap-3">
                        <TextField
                            label="目标字数"
                            type="number"
                            value={target}
                            onValueChange={setTarget}
                            disabled={!!running}
                        />
                        <ConfirmAction
                            label="压缩当前表超限条目"
                            description="会调用配置的模型并产生用量，压缩前会保存快照。"
                            disabled={
                                action.isPending ||
                                !!running ||
                                data.can_manage !== true ||
                                !targetValid
                            }
                            onConfirm={() => void compress()}
                        />
                    </div>
                    {currentTask && (
                        <div className="mt-4">
                            {taskId && panelQueryState(status)?.kind !== 'ok' && (
                                <PanelSectionState
                                    state={panelQueryState(status)}
                                    onRetry={() => void status.refetch()}
                                />
                            )}
                            <FieldList
                                data={currentTask}
                                columns={2}
                                labels={{
                                    task_id: '任务',
                                    status: '状态',
                                    total: '条目数',
                                    completed: '已完成',
                                    failed: '失败',
                                    message: '结果',
                                    error: '错误',
                                }}
                            />
                            {!running && (
                                <Button
                                    className="mt-3"
                                    size="sm"
                                    variant="secondary"
                                    onClick={() => {
                                        void items.refetch();
                                        void tables.refetch();
                                    }}
                                >
                                    刷新压缩后的条目
                                </Button>
                            )}
                        </div>
                    )}
                </PanelSection>
            )}
            {overLimit && (
                <PanelSection title="超限清单">
                    <FieldList data={overLimit} columns={2} />
                </PanelSection>
            )}
            {selected && (
                <ArchiveEditor
                    key={`${activeTable}/${text(selected.key)}`}
                    instanceId={instanceId}
                    table={activeTable}
                    itemKey={text(selected.key)}
                    onGoTab={onGoTab}
                    canDelete={data.delete_enabled === true && data.can_manage === true}
                    canCompress={
                        data.can_manage === true &&
                        data.summarize_available === true &&
                        !running &&
                        targetValid
                    }
                    onCompress={() => void compress(text(selected.key))}
                />
            )}
        </div>
    );
}
