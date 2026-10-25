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
import { panelPageGate, panelQueryState } from '../../../../core/domain/apps/neobotPanelState';
import { PanelGateView } from './PanelStateView';
import {
    ConfirmAction,
    FullText,
    FieldList,
    PanelEmpty,
    PanelQuerySection,
    PanelTable,
    type NeoBotPageProps,
} from './workspaceParts';
import { PanelDisclosure } from './PanelDisclosure';

export function NeoBotFlowsTab({ instanceId }: NeoBotPageProps) {
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
    const queries = [
        flows,
        history,
        ...(pipeline ? [flow] : []),
        ...(activeSeq ? [prompt] : []),
        ...(compareSeq ? [comparison] : []),
    ];
    const gate = panelPageGate(queries.map(panelQueryState));
    const retryAll = () => queries.forEach((query) => void query.refetch());
    if (gate) return <PanelGateView gate={gate} onRetry={retryAll} />;
    return (
        <div className="flex flex-col gap-4">
            <PanelQuerySection
                query={flows}
                title="对话流"
                meta="每 5 秒刷新"
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
                {(data) => (
                    <PanelTable
                        rows={records(data.items)}
                        rowKey={(row) => text(row.pipeline_key)}
                        selectedKey={pipeline}
                        columns={[
                            {
                                key: 'display_name',
                                label: '会话',
                                render: (row) => (
                                    <Button
                                        size="sm"
                                        variant="ghost"
                                        onClick={() => {
                                            setPipeline(text(row.pipeline_key));
                                            setSeq('__latest');
                                            setCompareSeq('');
                                        }}
                                    >
                                        {text(row.display_name) || text(row.pipeline_key)}
                                    </Button>
                                ),
                            },
                            { key: 'model', label: '模型' },
                            { key: 'iterations', label: '轮次', align: 'right' },
                            { key: 'prompt_chars', label: '提示词字数', align: 'right' },
                            {
                                key: 'stale',
                                label: '状态',
                                render: (row) => (row.stale === true ? '已过期' : '当前'),
                            },
                        ]}
                        empty="还没有对话流，在 QQ 中向机器人发送消息后查看。"
                    />
                )}
            </PanelQuerySection>
            {pipeline && (
                <PanelQuerySection query={flow} title="最近请求">
                    {(data) => (
                        <>
                            <FieldList
                                columns={2}
                                labels={{
                                    model: '模型',
                                    active: '正在处理',
                                    iterations: '轮次',
                                    message_count: '消息数',
                                    stale: '已过期',
                                    background_tasks: '后台任务',
                                }}
                                data={{
                                    model: data.model,
                                    active: data.active,
                                    iterations: data.iterations,
                                    message_count: data.message_count,
                                    stale: data.stale,
                                    background_tasks: data.background_tasks,
                                }}
                            />
                            <PanelDisclosure label="系统提示词" className="mt-4">
                                <FullText value={data.system_prompt} />
                            </PanelDisclosure>
                            <ol className="mt-4 flex flex-col gap-3">
                                {records(data.messages).map((message, i) => (
                                    <li key={i} className="border-l-2 border-brand/40 pl-3">
                                        <p className="mb-2 text-xs font-medium text-text">
                                            {(
                                                {
                                                    system: '系统',
                                                    user: '用户',
                                                    assistant: '助手',
                                                    tool: '工具',
                                                } as Record<string, string>
                                            )[text(message.role)] ?? text(message.role)}{' '}
                                            #{i + 1}
                                            {message.truncated === true ? ' · 上游快照已截断' : ''}
                                        </p>
                                        <FullText value={message.content} />
                                        {Array.isArray(message.tool_calls) && (
                                            <PanelDisclosure label="工具调用">
                                                <FullText value={message.tool_calls} />
                                            </PanelDisclosure>
                                        )}
                                    </li>
                                ))}
                            </ol>
                        </>
                    )}
                </PanelQuerySection>
            )}
            <PanelQuerySection
                query={history}
                title="完整提示词历史"
                meta="重启 NeoBot 后清空"
                actions={
                    <ConfirmAction
                        label="清空历史"
                        variant="danger"
                        description="清空全部会话的内存提示词历史，无法恢复。"
                        disabled={action.isPending || metas.length === 0}
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
                {() => (
                    <>
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
                            <PanelEmpty>还没有提示词记录，先向机器人发送一条消息。</PanelEmpty>
                        )}
                    </>
                )}
            </PanelQuerySection>
            {activeSeq && (
                <PanelQuerySection query={prompt} title={`完整请求 #${activeSeq}`}>
                    {(data) => (
                        <>
                            <FieldList
                                columns={2}
                                labels={{ model: '模型', recorded_at: '记录时间', usage: '用量' }}
                                data={{
                                    model: record(data.entry).model,
                                    recorded_at: record(data.entry).recorded_at,
                                    usage: record(data.entry).usage,
                                }}
                            />
                            <div className="mt-3">
                                <FullText label="完整模型请求" value={data.entry} />
                            </div>
                        </>
                    )}
                </PanelQuerySection>
            )}
            {compareSeq && (
                <PanelQuerySection query={comparison} title={`对比 #${compareSeq} → #${activeSeq}`}>
                    {(data) => (
                        <>
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
                        </>
                    )}
                </PanelQuerySection>
            )}
        </div>
    );
}
