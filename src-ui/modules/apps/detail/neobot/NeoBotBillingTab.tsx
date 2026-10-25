import { useState } from 'react';
import { Button, Select, TextField } from '../../../../shared/ui';
import { asRecord } from '../../../../core/domain/apps/neobotPanel';
import {
    record,
    strings,
    text,
    type PanelObject,
} from '../../../../core/domain/apps/neobotWorkspace';
import { usePanelJson } from '../../../../hooks/apps/useNeoBotPanel';
import { useNeoBotAction } from '../../../../hooks/apps/useNeoBotAction';
import { JsonDraftField } from './NeoBotSchemaFields';
import {
    FieldList,
    InlineNote,
    PanelPage,
    PanelSection,
    type NeoBotPageProps,
} from './workspaceParts';
import { PanelDisclosure } from './PanelDisclosure';

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
    const [resultKind, setResultKind] = useState<'preview' | 'reload'>('preview');
    const timeInvalid =
        occurredAt.trim() !== '' &&
        (!/T.+(?:Z|[+-]\d{2}:\d{2})$/i.test(occurredAt.trim()) ||
            !Number.isFinite(Date.parse(occurredAt)));
    const tokensInvalid = ['input_tokens', 'output_tokens'].some(
        (key) =>
            key in usage &&
            (typeof usage[key] !== 'number' ||
                !Number.isFinite(usage[key]) ||
                Number(usage[key]) < 0),
    );
    const problem = !model.trim()
        ? '填写模型引用名后才能试算'
        : timeInvalid
          ? '试算时间需为带时区的 ISO 时间，例如 2026-10-24T12:00:00+08:00'
          : !validConfig || !validUsage
            ? '修正配置与用量中的 JSON 格式'
            : tokensInvalid
              ? '输入与输出 Token 需为非负数'
              : '';
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
                                disabled={action.isPending || data.writable === false}
                                onClick={() => {
                                    setResult(null);
                                    setResultKind('reload');
                                    void action
                                        .run({
                                            path: '/api/config/billing/reload',
                                            body: script ? { scripts: [script] } : {},
                                        })
                                        .then(setResult);
                                }}
                            >
                                重载{script ? '所选' : '全部'}脚本
                            </Button>
                        }
                    >
                        <div className="flex flex-col gap-3">
                            <Select
                                label="计费脚本"
                                value={script || '__builtin'}
                                disabled={action.isPending}
                                items={[
                                    { value: '__builtin', label: '内建计费' },
                                    ...strings(data.scripts)
                                        .filter(Boolean)
                                        .map((s) => ({ value: s, label: s })),
                                ]}
                                onValueChange={(s) => {
                                    setScript(s === '__builtin' ? '' : s);
                                    setResult(null);
                                }}
                            />
                            <PanelDisclosure label="脚本与计费配置">
                                <FieldList data={data} columns={2} />
                            </PanelDisclosure>
                        </div>
                    </PanelSection>
                    <PanelSection title="计费试算">
                        <div className="grid gap-3 sm:grid-cols-2">
                            <TextField
                                label="模型引用名"
                                value={model}
                                disabled={action.isPending}
                                onValueChange={(value) => {
                                    setModel(value);
                                    setResult(null);
                                }}
                            />
                            <TextField
                                label="试算时间（选填，含时区的 ISO 时间）"
                                value={occurredAt}
                                disabled={action.isPending}
                                onValueChange={(value) => {
                                    setOccurredAt(value);
                                    setResult(null);
                                }}
                            />
                        </div>
                        <div className="mt-4 grid gap-3 sm:grid-cols-2">
                            <JsonDraftField
                                label="计费配置"
                                value={config}
                                disabled={action.isPending}
                                onChange={(v) => {
                                    setConfig(record(v));
                                    setResult(null);
                                }}
                                onValidity={setValidConfig}
                            />
                            <JsonDraftField
                                label="Token 用量"
                                value={usage}
                                disabled={action.isPending}
                                onChange={(v) => {
                                    setUsage(record(v));
                                    setResult(null);
                                }}
                                onValidity={setValidUsage}
                            />
                        </div>
                        {problem && (
                            <div className="mt-3">
                                <InlineNote>{problem}</InlineNote>
                            </div>
                        )}
                        <Button
                            className="mt-4"
                            size="sm"
                            variant="primary"
                            disabled={action.isPending || problem !== ''}
                            onClick={() => {
                                setResult(null);
                                setResultKind('preview');
                                void action
                                    .run({
                                        path: '/api/config/billing/preview',
                                        body: {
                                            model_key: model.trim(),
                                            billing_script: script,
                                            billing_config: config,
                                            usage,
                                            ...(occurredAt ? { occurred_at: occurredAt } : {}),
                                        },
                                        quiet: true,
                                        allowNegative: true,
                                    })
                                    .then(setResult);
                            }}
                        >
                            {action.isPending && resultKind === 'preview' ? '试算中' : '试算'}
                        </Button>
                    </PanelSection>
                    {result && (
                        <InlineNote tone={result.ok === false ? 'warning' : 'success'}>
                            <p>
                                {text(result.error) ||
                                    text(result.message) ||
                                    (resultKind === 'preview' ? '试算完成' : '脚本已重载')}
                            </p>
                            <div className="mt-3">
                                <FieldList data={result} hide={['message', 'error']} columns={2} />
                            </div>
                        </InlineNote>
                    )}
                </div>
            )}
        </PanelPage>
    );
}
