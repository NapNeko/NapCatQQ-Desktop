import { useState } from 'react';
import { Button, Select, TextField } from '../../../../shared/ui';
import { asRecord } from '../../../../core/domain/apps/neobotPanel';
import { record, strings, type PanelObject } from '../../../../core/domain/apps/neobotWorkspace';
import { usePanelJson } from '../../../../hooks/apps/useNeoBotPanel';
import { useNeoBotAction } from '../../../../hooks/apps/useNeoBotAction';
import { JsonDraftField } from './NeoBotSchemaFields';
import { ObjectView, PanelPage, PanelSection, type NeoBotPageProps } from './workspaceParts';

export function NeoBotBillingTab({ instanceId, onGoTab }: NeoBotPageProps) {
    const query = usePanelJson(instanceId, 'billing', '/api/config/billing', asRecord);
    const action = useNeoBotAction(instanceId);
    const [model, setModel] = useState('');
    const [script, setScript] = useState('');
    const [occurredAt, setOccurredAt] = useState('');
    const [config, setConfig] = useState<PanelObject>({});
    const [usage, setUsage] = useState<PanelObject>({ input_tokens: 1000, output_tokens: 1000 });
    const [validConfig, setValidConfig] = useState(true);
    const [validUsage, setValidUsage] = useState(true);
    const [result, setResult] = useState<PanelObject | null>(null);
    return (
        <PanelPage query={query} onGoTab={onGoTab}>
            {(data) => (
                <div className="flex flex-col gap-4">
                    <PanelSection
                        title="计费脚本"
                        actions={
                            <Button
                                size="sm"
                                variant="secondary"
                                disabled={action.isPending}
                                onClick={() =>
                                    void action
                                        .run({
                                            path: '/api/config/billing/reload',
                                            body: script ? { scripts: [script] } : {},
                                        })
                                        .then(setResult)
                                }
                            >
                                重载{script ? '所选' : '全部'}脚本
                            </Button>
                        }
                    >
                        <ObjectView data={data} />
                    </PanelSection>
                    <PanelSection title="计费试算">
                        <div className="grid gap-3 sm:grid-cols-2">
                            <TextField label="模型引用名" value={model} onValueChange={setModel} />
                            <Select
                                label="计费脚本"
                                value={script || '__builtin'}
                                items={[
                                    { value: '__builtin', label: '内建计费' },
                                    ...strings(data.scripts)
                                        .filter(Boolean)
                                        .map((s) => ({ value: s, label: s })),
                                ]}
                                onValueChange={(s) => setScript(s === '__builtin' ? '' : s)}
                            />
                            <TextField
                                label="试算时间（选填，含时区的 ISO 时间）"
                                value={occurredAt}
                                onValueChange={setOccurredAt}
                            />
                        </div>
                        <div className="mt-4 grid gap-3 sm:grid-cols-2">
                            <JsonDraftField
                                label="计费配置"
                                value={config}
                                onChange={(v) => setConfig(record(v))}
                                onValidity={setValidConfig}
                            />
                            <JsonDraftField
                                label="Token 用量"
                                value={usage}
                                onChange={(v) => setUsage(record(v))}
                                onValidity={setValidUsage}
                            />
                        </div>
                        <Button
                            className="mt-4"
                            size="sm"
                            variant="primary"
                            disabled={action.isPending || !validConfig || !validUsage}
                            onClick={() =>
                                void action
                                    .run({
                                        path: '/api/config/billing/preview',
                                        body: {
                                            model_key: model,
                                            billing_script: script,
                                            billing_config: config,
                                            usage,
                                            ...(occurredAt ? { occurred_at: occurredAt } : {}),
                                        },
                                        quiet: true,
                                        allowNegative: true,
                                    })
                                    .then(setResult)
                            }
                        >
                            试算
                        </Button>
                    </PanelSection>
                    {result && (
                        <PanelSection title="计费结果">
                            <ObjectView data={result} />
                        </PanelSection>
                    )}
                </div>
            )}
        </PanelPage>
    );
}
