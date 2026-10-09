import { Button } from '../../../../shared/ui';
import { asRecord } from '../../../../core/domain/apps/neobotPanel';
import { records, text } from '../../../../core/domain/apps/neobotWorkspace';
import { usePanelJson } from '../../../../hooks/apps/useNeoBotPanel';
import { FullText, PanelPage, PanelSection, type NeoBotPageProps } from './workspaceParts';

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
                                onClick={() => void query.refetch()}
                            >
                                重新分析
                            </Button>
                        }
                    >
                        <p className="text-xs text-text-secondary">
                            {text(data.rule) || text(data.error) || '按上游分析器的口径统计'}
                        </p>
                    </PanelSection>
                    {records(data.agents).map((agent, i) => (
                        <PanelSection
                            key={text(agent.name) || i}
                            title={text(agent.name) || 'Agent'}
                        >
                            <p className="mb-3 text-xs text-text-secondary">
                                {text(agent.model)} · {text(agent.total_chars)} 字符 ·{' '}
                                {text(agent.total_tokens)} Token
                            </p>
                            {text(agent.error) && (
                                <p className="mb-3 text-xs text-warning">{text(agent.error)}</p>
                            )}
                            <div className="flex flex-col gap-3">
                                {records(agent.parts).map((part, index) => (
                                    <details
                                        key={index}
                                        className="rounded-sm border border-border-subtle p-3"
                                    >
                                        <summary className="cursor-pointer text-xs text-text-secondary">
                                            {text(part.label)} · {text(part.chars)} 字符 ·{' '}
                                            {text(part.tokens)} Token
                                            {part.truncated ? ' · 上游展示已截断' : ''}
                                        </summary>
                                        <div className="mt-3">
                                            <FullText value={part.text} />
                                        </div>
                                    </details>
                                ))}
                            </div>
                        </PanelSection>
                    ))}
                </div>
            )}
        </PanelPage>
    );
}
