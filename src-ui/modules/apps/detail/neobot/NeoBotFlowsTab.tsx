import { useMemo, useState } from 'react';
import { Button, Select } from '../../../../shared/ui';
import { asRecord } from '../../../../core/domain/apps/neobotPanel';
import { lineDiff } from '../../../../core/domain/apps/textDiff';
import {
    json,
    paramsPath,
    record,
    records,
    text,
} from '../../../../core/domain/apps/neobotWorkspace';
import { usePanelJson } from '../../../../hooks/apps/useNeoBotPanel';
import { useNeoBotAction } from '../../../../hooks/apps/useNeoBotAction';
import {
    ConfirmAction,
    FullText,
    ObjectView,
    PanelPage,
    PanelSection,
    RecordTable,
    type NeoBotPageProps,
} from './workspaceParts';

export function NeoBotFlowsTab({ instanceId, onGoTab }: NeoBotPageProps) {
    const flows = usePanelJson(instanceId, 'flows', '/api/chat-flows', asRecord, true, 5000);
    const action = useNeoBotAction(instanceId);
    const [pipeline, setPipeline] = useState('');
    const [seq, setSeq] = useState('__latest');
    const [compareSeq, setCompareSeq] = useState('');
    const flowPath = paramsPath('/api/chat-flows/detail', { key: pipeline });
    const flow = usePanelJson(instanceId, flowPath, flowPath, asRecord, !!pipeline);
    const historyPath = paramsPath('/api/chat-flows/prompts', { key: pipeline });
    const history = usePanelJson(instanceId, historyPath, historyPath, asRecord, true, 5000);
    const metas = history.data?.kind === 'ok' ? records(history.data.data.items) : [];
    const activeSeq = seq === '__latest' ? text(metas.at(-1)?.seq) : seq;
    const promptPath = paramsPath('/api/chat-flows/prompt', { seq: activeSeq });
    const prompt = usePanelJson(instanceId, promptPath, promptPath, asRecord, !!activeSeq);
    const comparisonPath = paramsPath('/api/chat-flows/prompt', { seq: compareSeq });
    const comparison = usePanelJson(
        instanceId,
        comparisonPath,
        comparisonPath,
        asRecord,
        !!compareSeq,
    );
    const currentEntry = prompt.data?.kind === 'ok' ? prompt.data.data.entry : null;
    const previousEntry = comparison.data?.kind === 'ok' ? comparison.data.data.entry : null;
    const diff = useMemo(() => {
        if (!currentEntry || !previousEntry) return null;
        const before = json(previousEntry),
            after = json(currentEntry);
        if (before.split('\n').length * after.split('\n').length > 1_000_000) return null;
        return lineDiff(before, after);
    }, [currentEntry, previousEntry]);
    return (
        <div className="flex flex-col gap-4">
            <PanelPage query={flows} onGoTab={onGoTab}>
                {(data) => (
                    <PanelSection
                        title="对话流"
                        actions={
                            <Button
                                size="sm"
                                variant="ghost"
                                onClick={() => {
                                    setPipeline('');
                                    setSeq('__latest');
                                    setCompareSeq('');
                                }}
                            >
                                全部对话流
                            </Button>
                        }
                    >
                        <RecordTable
                            items={records(data.items)}
                            columns={[
                                ['display_name', '会话'],
                                ['pipeline_key', '标识'],
                                ['model', '模型'],
                                ['iterations', '轮次'],
                                ['prompt_chars', '提示词字数'],
                                ['stale', '过期'],
                            ]}
                            onSelect={(item) => {
                                setPipeline(text(item.pipeline_key));
                                setSeq('__latest');
                                setCompareSeq('');
                            }}
                        />
                    </PanelSection>
                )}
            </PanelPage>
            {pipeline && (
                <PanelPage query={flow} onGoTab={onGoTab}>
                    {(data) => (
                        <PanelSection title={`最近请求 · ${text(data.display_name) || pipeline}`}>
                            <ObjectView
                                data={{
                                    model: data.model,
                                    active: data.active,
                                    iterations: data.iterations,
                                    message_count: data.message_count,
                                    stale: data.stale,
                                    background_tasks: data.background_tasks,
                                }}
                            />
                            <details className="mt-4 text-xs text-text-secondary">
                                <summary className="cursor-pointer">系统提示词</summary>
                                <FullText value={data.system_prompt} />
                            </details>
                            <ol className="mt-4 flex flex-col gap-3">
                                {records(data.messages).map((message, i) => (
                                    <li key={i} className="border-l-2 border-brand/40 pl-3">
                                        <p className="mb-2 text-xs font-medium text-text">
                                            {text(message.role)} #{i + 1}
                                            {message.truncated === true ? ' · 上游快照已截断' : ''}
                                        </p>
                                        <FullText value={message.content} />
                                        {Array.isArray(message.tool_calls) && (
                                            <ObjectView data={{ tool_calls: message.tool_calls }} />
                                        )}
                                    </li>
                                ))}
                            </ol>
                        </PanelSection>
                    )}
                </PanelPage>
            )}
            <PanelPage query={history} onGoTab={onGoTab}>
                {(data) => (
                    <PanelSection
                        title="完整提示词历史"
                        actions={
                            <ConfirmAction
                                label="清空历史"
                                description="清空全部会话的内存提示词历史，无法恢复。"
                                disabled={action.isPending}
                                onConfirm={() =>
                                    void action
                                        .run({ path: '/api/chat-flows/prompts/clear' })
                                        .then((next) => {
                                            if (next) {
                                                setSeq('__latest');
                                                setCompareSeq('');
                                            }
                                        })
                                }
                            />
                        }
                    >
                        <p className="mb-3 text-xs text-text-tertiary">
                            历史在 NeoBot 内存中保存，重启后清空。驻留{' '}
                            {text(data.storage_bytes) || '0'} 字节。
                        </p>
                        {metas.length > 0 ? (
                            <div className="grid gap-3 sm:grid-cols-2">
                                <Select
                                    label="查看记录"
                                    value={seq}
                                    items={[
                                        { value: '__latest', label: '跟随最新' },
                                        ...metas.map((m) => ({
                                            value: text(m.seq),
                                            label: `#${text(m.seq)} · ${text(m.recorded_at)} · ${text(m.model)}`,
                                        })),
                                    ]}
                                    onValueChange={setSeq}
                                />
                                <Select
                                    label="对比记录"
                                    value={compareSeq || '__none'}
                                    items={[
                                        { value: '__none', label: '不对比' },
                                        ...metas.map((m) => ({
                                            value: text(m.seq),
                                            label: `#${text(m.seq)} · ${text(m.recorded_at)}`,
                                        })),
                                    ]}
                                    onValueChange={(s) => setCompareSeq(s === '__none' ? '' : s)}
                                />
                            </div>
                        ) : (
                            <p className="text-xs text-text-tertiary">暂无记录</p>
                        )}
                    </PanelSection>
                )}
            </PanelPage>
            {activeSeq && (
                <PanelPage query={prompt} onGoTab={onGoTab}>
                    {(data) => (
                        <PanelSection title={`完整请求 #${activeSeq}`}>
                            <ObjectView
                                data={{
                                    model: record(data.entry).model,
                                    recorded_at: record(data.entry).recorded_at,
                                    usage: record(data.entry).usage,
                                }}
                            />
                            <div className="mt-3">
                                <FullText label="完整模型请求" value={data.entry} />
                            </div>
                        </PanelSection>
                    )}
                </PanelPage>
            )}
            {compareSeq && (
                <PanelPage query={comparison} onGoTab={onGoTab}>
                    {(data) => (
                        <PanelSection title={`对比 #${compareSeq} → #${activeSeq}`}>
                            {diff ? (
                                <pre className="max-h-[32rem] overflow-auto whitespace-pre-wrap break-words rounded-sm bg-inset p-3 font-mono text-xs">
                                    {diff.map((line, i) => (
                                        <div
                                            key={i}
                                            className={
                                                line.kind === 'add'
                                                    ? 'text-brand'
                                                    : line.kind === 'del'
                                                      ? 'text-danger'
                                                      : 'text-text-tertiary'
                                            }
                                        >
                                            {line.kind === 'add'
                                                ? '+ '
                                                : line.kind === 'del'
                                                  ? '- '
                                                  : '  '}
                                            {line.text || ' '}
                                        </div>
                                    ))}
                                </pre>
                            ) : (
                                <FullText label="对比记录全文" value={data.entry} />
                            )}
                        </PanelSection>
                    )}
                </PanelPage>
            )}
        </div>
    );
}
