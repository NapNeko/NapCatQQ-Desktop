import { useState } from 'react';
import { useNeoBotDraftState } from '../../../../hooks/apps/useNeoBotDraftState';
import { Button, Checkbox, Select, TextAreaField, TextField } from '../../../../shared/ui';
import { asRecord } from '../../../../core/domain/apps/neobotPanel';
import {
    record,
    records,
    text,
    type PanelObject,
} from '../../../../core/domain/apps/neobotWorkspace';
import { usePanelJson } from '../../../../hooks/apps/useNeoBotPanel';
import { useNeoBotAction } from '../../../../hooks/apps/useNeoBotAction';
import { JsonDraftField } from './NeoBotSchemaFields';
import { ConfirmAction, PanelPage, PanelSection, type NeoBotPageProps } from './workspaceParts';

export function NeoBotScheduledTab({ instanceId, onGoTab }: NeoBotPageProps) {
    const query = usePanelJson(
        instanceId,
        'scheduled',
        '/api/scheduled-tasks?include_disabled=1&limit=200',
        asRecord,
    );
    const action = useNeoBotAction(instanceId);
    const [draft, setDraft] = useNeoBotDraftState<PanelObject | null>('scheduled:draft', null);
    const [valid, setValid] = useState(true);
    const patch = (next: PanelObject) => setDraft((prev) => ({ ...prev, ...next }));
    const run = (body: PanelObject) => action.run({ path: '/api/scheduled-tasks/action', body });
    return (
        <PanelPage query={query} onGoTab={onGoTab}>
            {(data) => {
                const editable =
                    data.editable !== false &&
                    data.can_manage !== false &&
                    data.available !== false;
                return (
                    <div className="flex flex-col gap-4">
                        <PanelSection
                            title="定时任务"
                            actions={
                                <>
                                    <Button
                                        size="sm"
                                        variant="ghost"
                                        disabled={action.isPending}
                                        onClick={() => void query.refetch()}
                                    >
                                        刷新
                                    </Button>
                                    <Button
                                        size="sm"
                                        variant="primary"
                                        disabled={!editable || action.isPending || draft !== null}
                                        onClick={() => {
                                            setDraft({
                                                title: '',
                                                detail: '',
                                                recurrence: 'once',
                                                start_at: '',
                                                end_at: '',
                                                bindings: [{ kind: 'group', id: '' }],
                                                one_shot_notification: true,
                                                metadata: {},
                                            });
                                            setValid(true);
                                        }}
                                    >
                                        新建任务
                                    </Button>
                                </>
                            }
                        >
                            {text(data.error) && (
                                <p className="mb-3 text-xs text-warning">{text(data.error)}</p>
                            )}
                            <div className="flex flex-col gap-3">
                                {records(data.tasks).map((task) => (
                                    <div
                                        key={text(task.task_id)}
                                        className="rounded-sm border border-border-subtle p-3"
                                    >
                                        <div className="flex items-baseline justify-between gap-3">
                                            <h4 className="text-xs font-medium text-text">
                                                {text(task.title)}
                                            </h4>
                                            <span className="text-2xs text-text-tertiary">
                                                {text(task.state)}
                                            </span>
                                        </div>
                                        <p className="mt-2 text-xs text-text-secondary">
                                            {text(task.detail)}
                                        </p>
                                        <p className="mt-2 text-2xs text-text-tertiary">
                                            {text(task.recurrence)} · 下次{' '}
                                            {text(task.next_run) || '—'} ·{' '}
                                            {records(task.bindings)
                                                .map(
                                                    (b) =>
                                                        `${b.kind === 'group' ? '群' : '私聊'} ${text(b.id)}`,
                                                )
                                                .join('、')}
                                        </p>
                                        <div className="mt-3 flex flex-wrap gap-2">
                                            <Button
                                                size="sm"
                                                variant="secondary"
                                                disabled={
                                                    !editable || action.isPending || draft !== null
                                                }
                                                onClick={() => {
                                                    setDraft({
                                                        task_uuid: task.task_id,
                                                        title: task.title,
                                                        detail: task.detail ?? '',
                                                        recurrence: task.recurrence,
                                                        start_at: task.start_at_local ?? '',
                                                        end_at: task.end_at_local ?? '',
                                                        bindings: task.bindings ?? [],
                                                        metadata: task.metadata ?? {},
                                                        one_shot_notification:
                                                            task.one_shot_notification !== false,
                                                    });
                                                    setValid(true);
                                                }}
                                            >
                                                编辑
                                            </Button>
                                            <Button
                                                size="sm"
                                                variant="secondary"
                                                disabled={!editable || action.isPending}
                                                onClick={() =>
                                                    void run({
                                                        action: 'set_state',
                                                        task_uuid: task.task_id,
                                                        state: task.enabled ? 'disabled' : 'active',
                                                    })
                                                }
                                            >
                                                {task.enabled ? '停用' : '启用'}
                                            </Button>
                                            <Button
                                                size="sm"
                                                variant="ghost"
                                                disabled={!editable || action.isPending}
                                                onClick={() =>
                                                    void run({
                                                        action: 'set_notification_policy',
                                                        task_uuid: task.task_id,
                                                        one_shot_notification:
                                                            task.one_shot_notification !== true,
                                                    })
                                                }
                                            >
                                                {task.one_shot_notification
                                                    ? '切为持续通知'
                                                    : '切为单次通知'}
                                            </Button>
                                            <ConfirmAction
                                                label="删除任务"
                                                description={`删除「${text(task.title)}」后不再执行。`}
                                                disabled={!editable || action.isPending}
                                                onConfirm={() =>
                                                    void run({
                                                        action: 'delete',
                                                        task_uuid: task.task_id,
                                                    })
                                                }
                                            />
                                        </div>
                                    </div>
                                ))}
                            </div>
                        </PanelSection>
                        {draft && (
                            <PanelSection title={draft.task_uuid ? '编辑任务' : '新建任务'}>
                                <div className="flex flex-col gap-3">
                                    <TextField
                                        label="标题"
                                        value={text(draft.title)}
                                        disabled={action.isPending}
                                        onValueChange={(title) => patch({ title })}
                                    />
                                    <TextAreaField
                                        label="任务内容"
                                        value={text(draft.detail)}
                                        disabled={action.isPending}
                                        onValueChange={(detail) => patch({ detail })}
                                    />
                                    <Select
                                        label="重复"
                                        value={text(draft.recurrence)}
                                        disabled={action.isPending}
                                        items={[
                                            { value: 'once', label: '一次' },
                                            { value: 'daily', label: '每天' },
                                            { value: 'weekly', label: '每周' },
                                            { value: 'monthly', label: '每月' },
                                            { value: 'yearly', label: '每年' },
                                        ]}
                                        onValueChange={(recurrence) => patch({ recurrence })}
                                    />
                                    <div className="grid gap-3 sm:grid-cols-2">
                                        <TextField
                                            label="开始（NeoBot 主机时间）"
                                            type="datetime-local"
                                            value={text(draft.start_at)}
                                            disabled={action.isPending}
                                            onValueChange={(start_at) => patch({ start_at })}
                                        />
                                        <TextField
                                            label="结束（NeoBot 主机时间）"
                                            type="datetime-local"
                                            value={text(draft.end_at)}
                                            disabled={action.isPending}
                                            onValueChange={(end_at) => patch({ end_at })}
                                        />
                                    </div>
                                    {records(draft.bindings).map((b, index) => (
                                        <div key={index} className="flex items-end gap-2">
                                            <Select
                                                label="聊天类型"
                                                value={text(b.kind)}
                                                disabled={action.isPending}
                                                items={[
                                                    { value: 'group', label: '群' },
                                                    { value: 'private', label: '私聊' },
                                                ]}
                                                onValueChange={(kind) =>
                                                    patch({
                                                        bindings: records(draft.bindings).map(
                                                            (x, i) =>
                                                                i === index ? { ...x, kind } : x,
                                                        ),
                                                    })
                                                }
                                            />
                                            <TextField
                                                className="flex-1"
                                                label="群号 / QQ"
                                                value={text(b.id)}
                                                disabled={action.isPending}
                                                onValueChange={(id) =>
                                                    patch({
                                                        bindings: records(draft.bindings).map(
                                                            (x, i) =>
                                                                i === index ? { ...x, id } : x,
                                                        ),
                                                    })
                                                }
                                            />
                                            <Button
                                                size="sm"
                                                variant="ghost"
                                                disabled={action.isPending}
                                                onClick={() =>
                                                    patch({
                                                        bindings: records(draft.bindings).filter(
                                                            (_, i) => i !== index,
                                                        ),
                                                    })
                                                }
                                            >
                                                移除
                                            </Button>
                                        </div>
                                    ))}
                                    <Button
                                        size="sm"
                                        variant="secondary"
                                        disabled={action.isPending}
                                        onClick={() =>
                                            patch({
                                                bindings: [
                                                    ...records(draft.bindings),
                                                    { kind: 'group', id: '' },
                                                ],
                                            })
                                        }
                                    >
                                        添加聊天绑定
                                    </Button>
                                    <Checkbox
                                        label="每个时间窗仅通知一次"
                                        checked={draft.one_shot_notification === true}
                                        disabled={action.isPending}
                                        onCheckedChange={(checked) =>
                                            patch({ one_shot_notification: checked === true })
                                        }
                                    />
                                    <details>
                                        <summary className="cursor-pointer text-xs text-text-secondary">
                                            高级元数据
                                        </summary>
                                        <JsonDraftField
                                            label="元数据"
                                            value={record(draft.metadata)}
                                            disabled={action.isPending}
                                            onValidity={setValid}
                                            onChange={(metadata) => patch({ metadata })}
                                        />
                                    </details>
                                </div>
                                <div className="mt-4 flex gap-2">
                                    <Button
                                        size="sm"
                                        variant="primary"
                                        disabled={
                                            !editable ||
                                            action.isPending ||
                                            !valid ||
                                            !text(draft.title).trim() ||
                                            !text(draft.start_at) ||
                                            !text(draft.end_at) ||
                                            !records(draft.bindings).some((b) => text(b.id).trim())
                                        }
                                        onClick={() =>
                                            void run({
                                                ...draft,
                                                action: draft.task_uuid ? 'update' : 'create',
                                                bindings: records(draft.bindings).filter((b) =>
                                                    text(b.id).trim(),
                                                ),
                                            }).then((next) => {
                                                if (next) setDraft(null);
                                            })
                                        }
                                    >
                                        保存任务
                                    </Button>
                                    <Button
                                        size="sm"
                                        variant="ghost"
                                        disabled={action.isPending}
                                        onClick={() => setDraft(null)}
                                    >
                                        取消
                                    </Button>
                                </div>
                            </PanelSection>
                        )}
                    </div>
                );
            }}
        </PanelPage>
    );
}
