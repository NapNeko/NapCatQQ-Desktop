// NeoBot 详情「提示词」页：各分区的可编辑项与实际取值。
//
// 面板把「默认值 / 自定义值 / 合并后的实际取值」三者都给了，这里展示实际取值，
// 并把「被自定义覆盖过」标出来 —— 排查「为什么它这么说话」时就要看这个。

import { parseNeoBotPrompts } from './neobotPanels';
import { PanelStateView } from './PanelStateView';
import { usePanelJson } from './useNeoBotPanel';

export const NeoBotPromptsTab: React.FC<{
    instanceId: string;
    onGoTab: (tab: string) => void;
}> = ({ instanceId, onGoTab }) => {
    const query = usePanelJson(instanceId, 'prompts', '/api/prompts', parseNeoBotPrompts);

    return (
        <PanelStateView
            state={query.data}
            isError={query.isError}
            errorMessage={query.error?.message}
            onRetry={() => void query.refetch()}
            onGoTab={onGoTab}
        >
            {(data) => (
                <div className="flex flex-col gap-4">
                    {!data.editable && (
                        <p className="text-2xs text-warning">
                            面板当前关闭了管理功能，这里只能看，改要去面板。
                        </p>
                    )}
                    {data.sections.length === 0 && (
                        <p className="text-xs text-text-tertiary">面板没有返回任何提示词分区。</p>
                    )}
                    {data.sections.map((section) => (
                        <section key={section.name} className="flex flex-col gap-1.5">
                            <h4 className="text-2xs uppercase tracking-widest text-text-tertiary">
                                {section.name}
                            </h4>
                            <ul className="flex flex-col gap-1">
                                {section.keys.map((k) => (
                                    <li
                                        key={k.path}
                                        className="rounded-sm border border-border-subtle bg-inset/40 px-3 py-2"
                                    >
                                        <div className="flex items-baseline justify-between gap-2">
                                            <span className="truncate text-xs text-text">
                                                {k.label}
                                            </span>
                                            <span className="flex shrink-0 items-baseline gap-1.5">
                                                {k.overridden && (
                                                    <span className="text-2xs text-warning">
                                                        已改
                                                    </span>
                                                )}
                                                <span className="font-mono text-2xs text-text-tertiary">
                                                    {k.kind}
                                                </span>
                                            </span>
                                        </div>
                                        <p
                                            className="mt-1 line-clamp-3 whitespace-pre-wrap font-mono text-2xs leading-snug text-text-secondary"
                                            title={k.value}
                                        >
                                            {k.value || '(空)'}
                                        </p>
                                        {k.placeholders.length > 0 && (
                                            <p className="mt-1 truncate text-2xs text-text-tertiary">
                                                占位符：{k.placeholders.join('、')}
                                            </p>
                                        )}
                                    </li>
                                ))}
                            </ul>
                        </section>
                    ))}
                </div>
            )}
        </PanelStateView>
    );
};
