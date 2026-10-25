import { Button } from '../../../../shared/ui';
import { asRecord } from '../../../../core/domain/apps/neobotPanel';
import { records, text } from '../../../../core/domain/apps/neobotWorkspace';
import { usePanelJson } from '../../../../hooks/apps/useNeoBotPanel';
import {
    FullText,
    InlineNote,
    Metric,
    MetricGrid,
    PanelEmpty,
    PanelPage,
    PanelSection,
    type NeoBotPageProps,
} from './workspaceParts';
import { PanelDisclosure } from './PanelDisclosure';

export function NeoBotAnalysisTab({ instanceId, onGoTab }: NeoBotPageProps) {
    const query = usePanelJson(instanceId, 'analysis', '/api/analysis/prompts', asRecord);
    return (
        <PanelPage query={query} onGoTab={onGoTab}>
            {(data) => (
                <div className="flex flex-col gap-4">
                    <PanelSection
                        title="提示词分析"
                        actions={
                            <Button
                                size="sm"
                                variant="secondary"
                                disabled={query.isFetching}
                                onClick={() => void query.refetch()}
                            >
                                {query.isFetching ? '分析中' : '重新分析'}
                            </Button>
                        }
                    >
                        <div className="flex flex-col gap-3">
                            {text(data.error) && (
                                <InlineNote tone="warning">{text(data.error)}</InlineNote>
                            )}
                            {records(data.agents).length ? (
                                <MetricGrid>
                                    <Metric label="分析对象" value={records(data.agents).length} />
                                    <Metric
                                        label="总字符"
                                        value={records(data.agents)
                                            .reduce(
                                                (sum, agent) =>
                                                    sum + (Number(agent.total_chars) || 0),
                                                0,
                                            )
                                            .toLocaleString()}
                                    />
                                    <Metric
                                        label="总 Token"
                                        value={records(data.agents)
                                            .reduce(
                                                (sum, agent) =>
                                                    sum + (Number(agent.total_tokens) || 0),
                                                0,
                                            )
                                            .toLocaleString()}
                                        tone="brand"
                                    />
                                </MetricGrid>
                            ) : (
                                <PanelEmpty
                                    action={
                                        <Button
                                            size="sm"
                                            variant="ghost"
                                            onClick={() => onGoTab('prompts')}
                                        >
                                            配置提示词
                                        </Button>
                                    }
                                >
                                    还没有可分析的提示词
                                </PanelEmpty>
                            )}
                            {text(data.rule) && (
                                <PanelDisclosure label="统计口径">
                                    <p className="text-xs text-text-secondary">{text(data.rule)}</p>
                                </PanelDisclosure>
                            )}
                        </div>
                    </PanelSection>
                    {records(data.agents).map((agent, i) => (
                        <PanelSection
                            key={text(agent.name) || i}
                            title={text(agent.name) || '分析对象'}
                            meta={`${text(agent.model) || '未指定模型'} · ${text(agent.total_chars) || '0'} 字符 · ${text(agent.total_tokens) || '0'} Token`}
                        >
                            {text(agent.error) && (
                                <div className="mb-3">
                                    <InlineNote tone="warning">{text(agent.error)}</InlineNote>
                                </div>
                            )}
                            <div className="flex flex-col gap-3">
                                {!records(agent.parts).length && (
                                    <PanelEmpty>这个对象还没有提示词分项，可重新分析</PanelEmpty>
                                )}
                                {records(agent.parts).map((part, index) => (
                                    <PanelDisclosure
                                        key={index}
                                        label={
                                            <>
                                                {text(part.label) || '未命名分项'} ·{' '}
                                                {text(part.chars)} 字符 · {text(part.tokens)} Token
                                                {part.truncated ? ' · 上游展示已截断' : ''}
                                            </>
                                        }
                                    >
                                        <FullText value={part.text} />
                                    </PanelDisclosure>
                                ))}
                            </div>
                        </PanelSection>
                    ))}
                </div>
            )}
        </PanelPage>
    );
}
