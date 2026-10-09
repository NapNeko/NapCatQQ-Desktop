import { useState } from 'react';
import { Button, Checkbox, Select } from '../../../../shared/ui';
import {
    defaultsFor,
    json,
    modelAssignments,
    parseFields,
    record,
    records,
    strings,
    text,
    type PanelObject,
} from '../../../../core/domain/apps/neobotWorkspace';
import { useNeoBotAction } from '../../../../hooks/apps/useNeoBotAction';
import { useNeoBotDraftState } from '../../../../hooks/apps/useNeoBotDraftState';
import { JsonDraftField, NeoBotSchemaFields } from './NeoBotSchemaFields';
import { ConfirmAction, ObjectView, PanelSection, type NeoBotPageProps } from './workspaceParts';

export function ModelsEditor({ instanceId, doc, onGoTab }: NeoBotPageProps & { doc: PanelObject }) {
    const action = useNeoBotAction(instanceId);
    const [base, setBase] = useNeoBotDraftState('models:base', doc);
    const [assignments, setAssignments] = useNeoBotDraftState(
        'models:roles',
        modelAssignments(doc.assignments),
    );
    const [entry, setEntry] = useNeoBotDraftState<PanelObject | null>('models:entry', null);
    const [invalid, setInvalid] = useState<Set<string>>(new Set());
    const [probe, setProbe] = useState<PanelObject | null>(null);
    const [providerModels, setProviderModels] = useState<string[]>([]);
    const [pulledProvider, setPulledProvider] = useState('');
    const fields = parseFields(base.entry_schema);
    const library = records(base.library);
    const roles = records(base.roles_meta);
    const writable = base.can_manage !== false;
    const busy = action.isPending;
    const roleDirty = json(assignments) !== json(modelAssignments(base.assignments));
    const refresh = async (resetRoles = false) => {
        const next = await action.run({ path: '/api/config/models', method: 'GET', quiet: true });
        if (next) {
            setBase(next);
            if (resetRoles) setAssignments(modelAssignments(next.assignments));
        }
    };
    const saveEntry = async () => {
        const result = await action.run({
            path: '/api/config/models/library',
            body: { action: 'upsert', entry, revision: base.revision, reload: true },
        });
        if (result) {
            setEntry(null);
            setInvalid(new Set());
            await refresh();
        }
    };
    const models = library
        .map((m) => ({
            value: text(m.model_ref),
            label: `${text(m.model_ref)} · ${text(m.display_name) || text(m.model_name)}`,
        }))
        .filter((m) => m.value);
    return (
        <div className="flex flex-col gap-4">
            <PanelSection
                title="模型分配"
                actions={
                    <>
                        <Button size="sm" variant="ghost" onClick={() => onGoTab('env')}>
                            供应商与密钥
                        </Button>
                        <Button
                            size="sm"
                            variant="primary"
                            disabled={!writable || busy || !roleDirty}
                            onClick={() =>
                                void action
                                    .run({
                                        path: '/api/config/models/assignments',
                                        body: {
                                            assignments,
                                            revision: base.revision,
                                            reload: true,
                                        },
                                    })
                                    .then((next) => {
                                        if (next) void refresh(true);
                                    })
                            }
                        >
                            保存分配并重载
                        </Button>
                    </>
                }
            >
                <div className="grid gap-3 sm:grid-cols-2">
                    {roles.map((role) => {
                        const key = text(role.role);
                        return role.multi ? (
                            <div key={key} className="flex flex-col gap-2">
                                <span className="text-xs text-text-secondary">
                                    {text(role.label) || key}
                                </span>
                                {models.map((m) => (
                                    <Checkbox
                                        key={m.value}
                                        label={m.label}
                                        checked={strings(assignments.creator_image_models).includes(
                                            m.value,
                                        )}
                                        disabled={!writable || busy}
                                        onCheckedChange={(checked) => {
                                            const current = strings(
                                                assignments.creator_image_models,
                                            );
                                            setAssignments({
                                                ...assignments,
                                                creator_image_models: checked
                                                    ? [...current, m.value]
                                                    : current.filter((x) => x !== m.value),
                                            });
                                        }}
                                    />
                                ))}
                            </div>
                        ) : (
                            <Select
                                key={key}
                                label={text(role.label) || key}
                                value={text(assignments[key]) || '__none'}
                                disabled={!writable || busy}
                                items={[{ value: '__none', label: '未分配' }, ...models]}
                                onValueChange={(value) =>
                                    setAssignments({
                                        ...assignments,
                                        [key]: value === '__none' ? '' : value,
                                    })
                                }
                            />
                        );
                    })}
                </div>
            </PanelSection>
            <PanelSection
                title={`模型库 · ${library.length}`}
                actions={
                    <>
                        <ConfirmAction
                            label="重新读取"
                            description="放弃当前模型与分配草稿？"
                            disabled={busy}
                            onConfirm={() => {
                                setEntry(null);
                                setInvalid(new Set());
                                void refresh(true);
                            }}
                        />
                        <Button
                            size="sm"
                            variant="primary"
                            disabled={!writable || busy || entry !== null}
                            onClick={() => {
                                setEntry(defaultsFor(fields));
                                setProviderModels([]);
                            }}
                        >
                            添加模型
                        </Button>
                    </>
                }
            >
                <div className="flex flex-col gap-3">
                    {library.map((m) => (
                        <div
                            key={text(m.model_ref)}
                            className="rounded-sm border border-border-subtle p-3"
                        >
                            <div className="flex flex-wrap items-center justify-between gap-2">
                                <div>
                                    <p className="text-xs font-medium text-text">
                                        {text(m.display_name) || text(m.model_ref)}
                                    </p>
                                    <p className="mt-1 text-2xs text-text-tertiary">
                                        {text(m.provider)} · {text(m.model_name)} ·{' '}
                                        {text(m.type_label) || text(m.model_type)}
                                    </p>
                                </div>
                                <div className="flex flex-wrap gap-2">
                                    <Button
                                        size="sm"
                                        variant="secondary"
                                        disabled={busy || entry !== null}
                                        onClick={() => {
                                            setEntry(structuredClone(record(m.entry ?? m)));
                                            setProviderModels([]);
                                        }}
                                    >
                                        编辑
                                    </Button>
                                    <Button
                                        size="sm"
                                        variant="ghost"
                                        disabled={!writable || busy}
                                        onClick={() =>
                                            void action
                                                .run({
                                                    path: '/api/config/models/test',
                                                    body: { model_ref: m.model_ref },
                                                    quiet: true,
                                                    allowNegative: true,
                                                })
                                                .then(setProbe)
                                        }
                                    >
                                        测试
                                    </Button>
                                    <ConfirmAction
                                        label="删除模型"
                                        description={`删除 ${text(m.model_ref)}？仍有角色引用时会拒绝删除。`}
                                        disabled={!writable || busy || entry !== null}
                                        onConfirm={() =>
                                            void action
                                                .run({
                                                    path: '/api/config/models/library',
                                                    body: {
                                                        action: 'delete',
                                                        model_ref: m.model_ref,
                                                        revision: base.revision,
                                                        reload: true,
                                                    },
                                                })
                                                .then((next) => {
                                                    if (next) void refresh();
                                                })
                                        }
                                    />
                                </div>
                            </div>
                            {m.api_key_configured === false && (
                                <p className="mt-2 text-xs text-warning">供应商尚未设置密钥</p>
                            )}
                        </div>
                    ))}
                </div>
            </PanelSection>
            {entry && (
                <PanelSection
                    title={text(entry.model_ref) ? `编辑 ${text(entry.model_ref)}` : '新模型'}
                    actions={
                        <Button
                            size="sm"
                            variant="ghost"
                            disabled={busy}
                            onClick={() => {
                                setEntry(null);
                                setInvalid(new Set());
                            }}
                        >
                            取消编辑
                        </Button>
                    }
                >
                    <div className="mb-4 flex flex-wrap gap-2">
                        <Button
                            size="sm"
                            variant="secondary"
                            disabled={!writable || busy || !text(entry.provider)}
                            onClick={() =>
                                void action
                                    .run({
                                        path: '/api/config/models/provider-models',
                                        body: {
                                            provider: entry.provider,
                                            use_system_proxy: entry.use_system_proxy,
                                        },
                                        quiet: true,
                                        allowNegative: true,
                                    })
                                    .then((result) => {
                                        if (result) {
                                            setProviderModels(strings(result.models));
                                            setPulledProvider(text(entry.provider));
                                            setProbe(result);
                                        }
                                    })
                            }
                        >
                            拉取供应商模型
                        </Button>
                        <Button
                            size="sm"
                            variant="secondary"
                            disabled={!writable || busy || invalid.size > 0}
                            onClick={() =>
                                void action
                                    .run({
                                        path: '/api/config/models/test',
                                        body: { entry },
                                        quiet: true,
                                        allowNegative: true,
                                    })
                                    .then(setProbe)
                            }
                        >
                            测试草稿
                        </Button>
                    </div>
                    {providerModels.length > 0 && pulledProvider === text(entry.provider) && (
                        <Select
                            className="mb-4"
                            label="供应商可用模型"
                            value={text(entry.model_name)}
                            items={providerModels
                                .filter(Boolean)
                                .map((m) => ({ value: m, label: m }))}
                            onValueChange={(model_name) => setEntry({ ...entry, model_name })}
                        />
                    )}
                    {fields.length ? (
                        <NeoBotSchemaFields
                            fields={fields.filter((f) => f.name !== 'model_ref')}
                            value={entry}
                            disabled={!writable || busy}
                            onChange={setEntry}
                            onValidity={(key, valid) =>
                                setInvalid((prev) => {
                                    const next = new Set(prev);
                                    if (valid) next.delete(key);
                                    else next.add(key);
                                    return next;
                                })
                            }
                        />
                    ) : (
                        <JsonDraftField
                            label="模型配置"
                            value={entry}
                            onChange={(next) => setEntry(record(next))}
                            onValidity={(valid) =>
                                setInvalid(valid ? new Set() : new Set(['entry']))
                            }
                        />
                    )}
                    <Button
                        className="mt-4"
                        size="sm"
                        variant="primary"
                        disabled={!writable || busy || invalid.size > 0}
                        onClick={() => void saveEntry()}
                    >
                        保存并重载
                    </Button>
                </PanelSection>
            )}
            {probe && (
                <PanelSection title="探测结果">
                    <ObjectView data={probe} />
                </PanelSection>
            )}
        </div>
    );
}
