import { useState } from 'react';
import { Button, Select, TextAreaField } from '../../../../shared/ui';
import { asRecord } from '../../../../core/domain/apps/neobotPanel';
import {
    defaultsFor,
    fieldErrors,
    json,
    parseFields,
    record,
    text,
    type PanelObject,
} from '../../../../core/domain/apps/neobotWorkspace';
import { usePanelJson } from '../../../../hooks/apps/useNeoBotPanel';
import { useNeoBotAction } from '../../../../hooks/apps/useNeoBotAction';
import { useNeoBotDraftState } from '../../../../hooks/apps/useNeoBotDraftState';
import { NeoBotSchemaFields } from './NeoBotSchemaFields';
import { ConfirmAction, PanelPage, PanelSection, type NeoBotPageProps } from './workspaceParts';

export function NeoBotConfigEditor({
    instanceId,
    doc,
    path,
    title,
}: {
    instanceId: string;
    doc: PanelObject;
    path: string;
    title: string;
}) {
    const action = useNeoBotAction(instanceId);
    const [base, setBase] = useNeoBotDraftState(`${path}:base`, doc);
    const [draft, setDraft] = useNeoBotDraftState(`${path}:draft`, record(doc.config));
    const [source, setSource] = useNeoBotDraftState(`${path}:source`, text(doc.source));
    const [mode, setMode] = useNeoBotDraftState(
        `${path}:mode`,
        doc.form_supported === false ? 'toml' : 'form',
    );
    const [generation, setGeneration] = useState(0);
    const [invalid, setInvalid] = useState<Set<string>>(new Set());
    const [errors, setErrors] = useState('');
    const [history, setHistory] = useState<PanelObject[]>([]);
    const fields = parseFields(base.schema);
    const dirty =
        invalid.size > 0 ||
        (mode === 'form' ? json(draft) !== json(base.config) : source !== text(base.source));
    const apply = (next: PanelObject) => {
        setBase(next);
        setDraft(record(next.config));
        setSource(text(next.source));
        setInvalid(new Set());
        setErrors('');
        setGeneration((n) => n + 1);
    };
    const reload = async () => {
        const next = await action.run({ path, method: 'GET', quiet: true });
        if (next) apply(next);
    };
    const body = {
        mode,
        revision: base.revision,
        ...(mode === 'form' ? { config: draft } : { source }),
    };
    const save = async (hotReload: boolean) => {
        const next = await action.run({ path, body: { ...body, reload: hotReload } });
        if (next) {
            setHistory((prev) => [base, ...prev].slice(0, 10));
            apply(next);
        }
    };
    const validate = async () => {
        const next = await action.run({ path: '/api/config/validate', body, quiet: true });
        if (next) setErrors(fieldErrors(next) || '校验通过');
    };
    const switchMode = (next: string) => {
        setMode(next);
        setDraft(record(base.config));
        setSource(text(base.source));
        setInvalid(new Set());
    };
    const writable = base.editable !== false && base.readonly !== true && base.can_manage !== false;
    return (
        <PanelSection
            title={title}
            actions={
                <>
                    <ConfirmAction
                        label="重新读取"
                        description="放弃当前草稿，读取服务器最新配置？"
                        disabled={action.isPending}
                        onConfirm={() => void reload()}
                    />
                    <Button
                        size="sm"
                        variant="ghost"
                        disabled={!dirty || action.isPending}
                        onClick={() => apply(base)}
                    >
                        撤销草稿
                    </Button>
                </>
            }
        >
            <div className="mb-4 flex flex-wrap items-end gap-3">
                <Select
                    label="编辑方式"
                    value={mode}
                    disabled={dirty || action.isPending}
                    onValueChange={switchMode}
                    items={[
                        ...(base.form_supported !== false && fields.length
                            ? [{ value: 'form', label: '表单' }]
                            : []),
                        ...(base.source_available !== false
                            ? [{ value: 'toml', label: 'TOML' }]
                            : []),
                    ]}
                />
                {history.length > 0 && (
                    <Select
                        label="本页保存历史"
                        value={undefined}
                        placeholder="选择一份草稿"
                        disabled={action.isPending}
                        items={history.map((h, i) => ({
                            value: String(i),
                            label: `保存前 #${i + 1} · ${text(h.revision)}`,
                        }))}
                        onValueChange={(i) => {
                            const h = history[Number(i)];
                            if (h) {
                                setDraft(record(h.config));
                                setSource(text(h.source));
                                setInvalid(new Set());
                                setGeneration((n) => n + 1);
                            }
                        }}
                    />
                )}
                {mode === 'form' && (
                    <ConfirmAction
                        label="恢复默认草稿"
                        description="用出厂默认值替换当前表单，保存后才会生效。"
                        disabled={!writable || action.isPending}
                        onConfirm={() => {
                            setDraft(defaultsFor(fields));
                            setInvalid(new Set());
                            setGeneration((n) => n + 1);
                        }}
                    />
                )}
            </div>
            {dirty && (
                <p className="mb-3 text-xs text-warning">
                    有未保存的修改，撤销或保存后可切换编辑方式。
                </p>
            )}
            {mode === 'form' ? (
                <NeoBotSchemaFields
                    key={`${text(base.revision)}:${generation}`}
                    fields={fields}
                    value={draft}
                    disabled={!writable || action.isPending}
                    onChange={setDraft}
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
                <TextAreaField
                    label="配置 TOML"
                    value={source}
                    disabled={!writable || action.isPending}
                    minRows={16}
                    maxRows={30}
                    mono
                    onValueChange={setSource}
                />
            )}
            {errors && (
                <p role="status" className="mt-3 whitespace-pre-wrap text-xs text-text-secondary">
                    {errors}
                </p>
            )}
            <div className="mt-4 flex flex-wrap gap-2">
                {path === '/api/config' && (
                    <Button
                        size="sm"
                        variant="secondary"
                        disabled={action.isPending || invalid.size > 0}
                        onClick={() => void validate()}
                    >
                        校验
                    </Button>
                )}
                <Button
                    size="sm"
                    variant="secondary"
                    disabled={!writable || !dirty || action.isPending || invalid.size > 0}
                    onClick={() => void save(false)}
                >
                    保存
                </Button>
                <Button
                    size="sm"
                    variant="primary"
                    disabled={!writable || !dirty || action.isPending || invalid.size > 0}
                    onClick={() => void save(true)}
                >
                    保存并重载
                </Button>
                {path === '/api/config' && (
                    <Button
                        size="sm"
                        variant="ghost"
                        disabled={action.isPending}
                        onClick={() => void action.run({ path: '/api/config/reload' })}
                    >
                        重载已保存配置
                    </Button>
                )}
            </div>
        </PanelSection>
    );
}

export function NeoBotConfigTab({ instanceId, onGoTab }: NeoBotPageProps) {
    const query = usePanelJson(instanceId, 'config', '/api/config', asRecord);
    return (
        <PanelPage query={query} onGoTab={onGoTab}>
            {(doc) => (
                <NeoBotConfigEditor
                    key={instanceId}
                    instanceId={instanceId}
                    doc={doc}
                    path="/api/config"
                    title="本体配置"
                />
            )}
        </PanelPage>
    );
}
