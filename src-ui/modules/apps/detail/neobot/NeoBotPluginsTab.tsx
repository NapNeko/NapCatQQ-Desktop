// NeoBot 详情「插件」页。
//
// 与别的框架不同：NeoBot 没有插件市场索引，装第三方插件是**贴一个 GitHub 仓库地址**
// （面板 /api/plugins/install 收 repo 参数）。所以这里不做「市场浏览」，只做
// 「看已装的 + 贴地址装新的」。

import { useState } from 'react';
import { Button, TextField } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import { parseNeoBotPlugins, type NeoBotPlugin } from './neobotPanels';
import { PanelStateView } from './PanelStateView';
import { usePanelJson } from './useNeoBotPanel';

const PluginRow: React.FC<{ plugin: NeoBotPlugin; consolePlugin: string }> = ({
    plugin,
    consolePlugin,
}) => {
    const isConsole = plugin.id === consolePlugin;
    return (
        <li className="rounded-sm border border-border-subtle bg-inset/40 px-3 py-2">
            <div className="flex items-baseline justify-between gap-2">
                <span className="flex min-w-0 items-baseline gap-1.5">
                    <span className="truncate text-xs font-medium text-text">{plugin.name}</span>
                    {plugin.version && (
                        <span className="shrink-0 text-2xs text-text-tertiary">
                            v{plugin.version}
                        </span>
                    )}
                    {plugin.official && <span className="shrink-0 text-2xs text-brand">官方</span>}
                    {isConsole && <span className="shrink-0 text-2xs text-text-tertiary">面板自身</span>}
                </span>
                <span
                    className={cn(
                        'shrink-0 text-2xs',
                        plugin.error
                            ? 'text-danger'
                            : plugin.enabled
                              ? 'text-brand'
                              : 'text-text-tertiary',
                    )}
                >
                    {plugin.status || (plugin.enabled ? '已启用' : '已停用')}
                </span>
            </div>
            {plugin.description && (
                <p className="mt-0.5 text-2xs leading-snug text-text-secondary">
                    {plugin.description}
                </p>
            )}
            {plugin.missingPythonDependencies.length > 0 && (
                <p className="mt-1 text-2xs text-warning">
                    缺 Python 依赖：{plugin.missingPythonDependencies.join('、')}
                </p>
            )}
            {plugin.error && <p className="mt-1 text-2xs text-danger">{plugin.error}</p>}
            {plugin.configPath && (
                <p className="mt-1 truncate font-mono text-2xs text-text-tertiary" title={plugin.configPath}>
                    配置：{plugin.configPath}
                </p>
            )}
        </li>
    );
};

export const NeoBotPluginsTab: React.FC<{
    instanceId: string;
    onGoTab: (tab: string) => void;
}> = ({ instanceId, onGoTab }) => {
    const query = usePanelJson(instanceId, 'plugins', '/api/plugins', parseNeoBotPlugins);
    const [repo, setRepo] = useState('');

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
                    <section className="rounded-md border border-border-subtle bg-surface px-4 py-3">
                        <h3 className="text-sm font-semibold text-text">安装第三方插件</h3>
                        <p className="mt-1 text-xs leading-relaxed text-text-secondary">
                            NeoBot 没有插件市场，装法是填一个 GitHub 仓库地址。装完在这里启停、重载或卸载。
                            {!data.manageEnabled &&
                                '（面板当前关闭了管理功能，只能查看，不能安装或启停。）'}
                        </p>
                        <div className="mt-3 flex items-end gap-2">
                            <TextField
                                className="flex-1"
                                label="插件仓库地址"
                                placeholder="https://github.com/作者/插件仓库"
                                value={repo}
                                disabled={!data.manageEnabled}
                                onValueChange={setRepo}
                            />
                            <Button variant="primary" size="sm" disabled>
                                安装
                            </Button>
                        </div>
                        <p className="mt-1 text-2xs text-text-tertiary">
                            安装动作桌面端还没接上（面板的 /api/plugins/install 还没接）——先只做只读展示。
                        </p>
                    </section>

                    <section className="flex flex-col gap-1.5">
                        <h4 className="text-2xs uppercase tracking-widest text-text-tertiary">
                            已安装 {data.items.length} 个
                        </h4>
                        <ul className="flex flex-col gap-1.5">
                            {data.items.map((p) => (
                                <PluginRow key={p.id} plugin={p} consolePlugin={data.consolePlugin} />
                            ))}
                        </ul>
                    </section>
                </div>
            )}
        </PanelStateView>
    );
};
