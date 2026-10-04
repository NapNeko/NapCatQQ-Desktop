// NeoBot 详情「模型」页：模型库 + 角色分配。
//
// 模型库是「本机引用名 → 供应商 + 真实模型名」的映射；分配是「角色 → 引用名」。
// 只读展示：改模型要在面板里改（模型编辑要拉供应商模型列表、试连通，桌面端不重复做）。

import { parseNeoBotModels } from './neobotPanels';
import { PanelStateView } from './PanelStateView';
import { usePanelJson } from './useNeoBotPanel';

export const NeoBotModelsTab: React.FC<{
    instanceId: string;
    onGoTab: (tab: string) => void;
}> = ({ instanceId, onGoTab }) => {
    const query = usePanelJson(instanceId, 'models', '/api/config/models', parseNeoBotModels);

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
                    <section className="flex flex-col gap-1.5">
                        <h4 className="text-2xs uppercase tracking-widest text-text-tertiary">
                            角色分配
                        </h4>
                        {Object.keys(data.assignments).length === 0 ? (
                            <p className="text-xs text-text-tertiary">
                                还没有分配：面板里没有把任何角色指向模型，Bot 用不了大模型能力。
                            </p>
                        ) : (
                            <ul className="flex flex-col gap-1">
                                {Object.entries(data.assignments).map(([role, ref]) => (
                                    <li
                                        key={role}
                                        className="flex items-baseline justify-between gap-2 rounded-sm border border-border-subtle bg-inset/40 px-3 py-1.5"
                                    >
                                        <span className="text-xs text-text-secondary">{role}</span>
                                        <span className="truncate font-mono text-2xs text-text">
                                            {ref}
                                        </span>
                                    </li>
                                ))}
                            </ul>
                        )}
                    </section>

                    <section className="flex flex-col gap-1.5">
                        <h4 className="text-2xs uppercase tracking-widest text-text-tertiary">
                            模型库 {data.library.length} 个
                        </h4>
                        {data.library.length === 0 ? (
                            <p className="text-xs text-text-tertiary">模型库是空的。</p>
                        ) : (
                            <ul className="flex flex-col gap-1.5">
                                {data.library.map((m) => (
                                    <li
                                        key={m.modelRef}
                                        className="rounded-sm border border-border-subtle bg-inset/40 px-3 py-2"
                                    >
                                        <div className="flex items-baseline justify-between gap-2">
                                            <span className="truncate font-mono text-xs text-text">
                                                {m.modelRef}
                                            </span>
                                            <span className="shrink-0 text-2xs text-text-tertiary">
                                                {m.typeLabel}
                                            </span>
                                        </div>
                                        <p className="mt-0.5 truncate text-2xs text-text-secondary">
                                            {m.displayName || '(无显示名)'}
                                            {m.provider ? ' · ' + m.provider : ''}
                                            {m.modelName ? ' · ' + m.modelName : ''}
                                        </p>
                                        {!m.providerHasKey && (
                                            <p className="mt-0.5 text-2xs text-warning">
                                                该供应商还没配 Key，这个模型现在用不了
                                            </p>
                                        )}
                                    </li>
                                ))}
                            </ul>
                        )}
                    </section>
                </div>
            )}
        </PanelStateView>
    );
};
