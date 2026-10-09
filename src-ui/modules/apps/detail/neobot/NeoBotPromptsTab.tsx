import { useState } from 'react';
import { Button, Select, TextAreaField, TextField } from '../../../../shared/ui';
import { parseNeoBotPrompts, type NeoBotPrompts } from '../../../../core/domain/apps/neobotPanels';
import { strings, text, type PanelObject } from '../../../../core/domain/apps/neobotWorkspace';
import { usePanelJson } from '../../../../hooks/apps/useNeoBotPanel';
import { useNeoBotAction } from '../../../../hooks/apps/useNeoBotAction';
import { useNeoBotDraftState } from '../../../../hooks/apps/useNeoBotDraftState';
import {
    ConfirmAction,
    FullText,
    PanelPage,
    PanelSection,
    type NeoBotPageProps,
} from './workspaceParts';

function PromptEditor({ instanceId, data }: { instanceId: string; data: NeoBotPrompts }) {
    const action = useNeoBotAction(instanceId);
    const items = data.sections.flatMap((s) =>
        s.keys.map((k) => ({ ...k, section: s.name, id: `${s.name}/${k.path}` })),
    );
    const [selection, setSelection] = useNeoBotDraftState<string | null>('prompts:selection', null);
    const [edits, setEdits] = useNeoBotDraftState<Record<string, string>>('prompts:edits', {});
    const [values, setValues] = useState<Record<string, string>>({});
    const [preview, setPreview] = useState<PanelObject | null>(null);
    const current = items.find((k) => k.id === selection) ?? items[0];
    if (!current) return <p className="text-xs text-text-tertiary">暂无提示词模板</p>;
    const draft = edits[current.id] ?? current.value;
    const dirty = draft !== current.value;
    const target = { section: current.section, path: current.path };
    const clearEdit = () =>
        setEdits((prev) => {
            const next = { ...prev };
            delete next[current.id];
            return next;
        });
    return (
        <PanelSection
            title="提示词模板"
            actions={
                <>
                    <Button
                        size="sm"
                        variant="ghost"
                        disabled={!dirty || action.isPending}
                        onClick={clearEdit}
                    >
                        撤销草稿
                    </Button>
                    <ConfirmAction
                        label="恢复默认"
                        description="删除当前模板的自定义覆盖？"
                        disabled={!data.editable || action.isPending}
                        onConfirm={() =>
                            void action
                                .run({ path: '/api/prompts/reset', body: target })
                                .then((next) => {
                                    if (next) {
                                        clearEdit();
                                        setPreview(null);
                                    }
                                })
                        }
                    />
                    <Button
                        size="sm"
                        variant="primary"
                        disabled={!data.editable || !dirty || action.isPending}
                        onClick={() =>
                            void action
                                .run({
                                    path: '/api/prompts/save',
                                    body: { ...target, value: draft },
                                })
                                .then((next) => {
                                    if (next) clearEdit();
                                })
                        }
                    >
                        保存模板
                    </Button>
                </>
            }
        >
            <Select
                label="模板"
                value={current.id}
                disabled={action.isPending}
                items={items.map((k) => ({
                    value: k.id,
                    label: `${k.section} · ${k.label}${edits[k.id] !== undefined ? '（草稿）' : k.overridden ? '（已自定义）' : ''}`,
                }))}
                onValueChange={(id) => {
                    setSelection(id);
                    setPreview(null);
                }}
            />
            {!data.editable && <p className="mt-3 text-xs text-warning">当前会话只有查看权限</p>}
            <TextAreaField
                className="mt-4"
                label="模板内容"
                value={draft}
                disabled={!data.editable || action.isPending}
                minRows={12}
                maxRows={24}
                mono
                onValueChange={(value) => {
                    setEdits({ ...edits, [current.id]: value });
                    setPreview(null);
                }}
            />
            <div className="mt-4 grid gap-3 sm:grid-cols-2">
                {current.placeholders.map((key) => (
                    <TextField
                        key={key}
                        label={`预览变量：${key}`}
                        value={values[key] ?? ''}
                        onValueChange={(value) => {
                            setValues({ ...values, [key]: value });
                            setPreview(null);
                        }}
                    />
                ))}
            </div>
            <Button
                className="mt-4"
                size="sm"
                variant="secondary"
                disabled={action.isPending}
                onClick={() =>
                    void action
                        .run({
                            path: '/api/prompts/preview',
                            body: { ...target, template: draft, values },
                            quiet: true,
                        })
                        .then(setPreview)
                }
            >
                预览渲染
            </Button>
            {preview && (
                <div className="mt-4">
                    <FullText label="渲染结果" value={text(preview.rendered)} />
                    {strings(preview.unresolved).length > 0 && (
                        <p className="mt-2 text-xs text-warning">
                            未替换：{strings(preview.unresolved).join('、')}
                        </p>
                    )}
                </div>
            )}
        </PanelSection>
    );
}

export function NeoBotPromptsTab({ instanceId, onGoTab }: NeoBotPageProps) {
    const query = usePanelJson(instanceId, 'prompts', '/api/prompts', parseNeoBotPrompts);
    return (
        <PanelPage query={query} onGoTab={onGoTab}>
            {(data) => <PromptEditor key={instanceId} instanceId={instanceId} data={data} />}
        </PanelPage>
    );
}
