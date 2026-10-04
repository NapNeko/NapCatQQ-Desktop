// NeoBot 详情「记忆」页：档案表清单与超限情况。
//
// NeoBot 的记忆是分表存的；面板给出每表的条目数与超限条目数。
// 摘要（AI 压缩）是长任务，桌面端这里先只展示，不触发。

import { parseNeoBotArchives } from './neobotPanels';
import { PanelStateView } from './PanelStateView';
import { usePanelJson } from './useNeoBotPanel';

export const NeoBotMemoryTab: React.FC<{
    instanceId: string;
    onGoTab: (tab: string) => void;
}> = ({ instanceId, onGoTab }) => {
    const query = usePanelJson(instanceId, 'archives', '/api/archives', parseNeoBotArchives);

    return (
        <PanelStateView
            state={query.data}
            isError={query.isError}
            errorMessage={query.error?.message}
            onRetry={() => void query.refetch()}
            onGoTab={onGoTab}
        >
            {(data) => (
                <div className="flex flex-col gap-3">
                    <p className="text-xs leading-relaxed text-text-secondary">
                        记忆按表存放。条目超过长度上限的要压缩，否则会一直占着上下文。
                        {data.summarizeAvailable
                            ? '面板里可以对超限的表跑 AI 压缩。'
                            : '当前没有可用的压缩服务，超限的表只能在面板里手动处理。'}
                    </p>
                    {data.tables.length === 0 ? (
                        <p className="text-xs text-text-tertiary">还没有任何档案表。</p>
                    ) : (
                        <ul className="flex flex-col gap-1.5">
                            {data.tables.map((t) => (
                                <li
                                    key={t.name}
                                    className="flex items-baseline justify-between gap-2 rounded-sm border border-border-subtle bg-inset/40 px-3 py-2"
                                >
                                    <span className="truncate font-mono text-xs text-text">
                                        {t.name}
                                    </span>
                                    <span className="flex shrink-0 items-baseline gap-2 text-2xs">
                                        <span className="text-text-secondary">{t.count} 条</span>
                                        {t.overLimit > 0 && (
                                            <span className="text-warning">
                                                {t.overLimit} 条超限
                                            </span>
                                        )}
                                    </span>
                                </li>
                            ))}
                        </ul>
                    )}
                </div>
            )}
        </PanelStateView>
    );
};
