// NeoBot 详情「插件」页。
//
// 与别的框架不同：NeoBot 没有插件市场索引，装第三方插件是**贴一个 GitHub 仓库地址**
// （面板 /api/plugins/install 收 repo 参数）。所以这里不做「市场浏览」，只做
// 「看已装的 + 贴地址装新的 + 对它启停 / 重载 / 卸载」。
//
// 所有动作都是 POST 到面板，成功后重取列表——面板自己会落盘，桌面端不另存状态。

import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { Button, TextField } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import { appFrameworkService } from '../../../../core/services/app-framework.service';
import { parseNeoBotPlugins, type NeoBotPlugin } from './neobotPanels';
import { PanelStateView } from './PanelStateView';
import { neobotPanelKey, usePanelJson } from './useNeoBotPanel';

/** 一次插件动作：POST 完重取列表。面板是唯一事实来源，桌面端不乐观更新。 */
function usePluginAction(instanceId: string) {
    const queryClient = useQueryClient();
    return useMutation({
        mutationFn: async (req: { path: string; body?: unknown }) => {
            const res = await appFrameworkService.panelCall(instanceId, 'POST', req.path, req.body);
            if (res === null) throw new Error('该框架不支持面板操作');
            if (res.kind !== 'ok') throw new Error(res.message || '面板拒绝了这次操作');
            return res;
        },
        onSettled: () => {
            void queryClient.invalidateQueries({ queryKey: neobotPanelKey(instanceId, 'plugins') });
        },
    });
}

const PluginRow: React.FC<{
    plugin: NeoBotPlugin;
    consolePlugin: string;
    disabled: boolean;
    busy: boolean;
    onAction: (path: string, confirmText?: string) => void;
}> = ({ plugin, consolePlugin, disabled, busy, onAction }) => {
    const isConsole = plugin.id === consolePlugin;
    // 面板自身不能被卸载；官方插件与不可管插件由面板自己拒绝，这里先挡一道
    const canUninstall = plugin.manageable && !isConsole && !plugin.official;
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
                    {isConsole && (
                        <span className="shrink-0 text-2xs text-text-tertiary">面板自身</span>
                    )}
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
                <p
                    className="mt-1 truncate font-mono text-2xs text-text-tertiary"
                    title={plugin.configPath}
                >
                    配置：{plugin.configPath}
                </p>
            )}
            {!isConsole && (
                <div className="mt-2 flex items-center gap-1.5">
                    <Button
                        size="sm"
                        variant="secondary"
                        disabled={disabled || busy || !plugin.manageable}
                        onClick={() =>
                            onAction(
                                '/api/plugins/' + plugin.id + '/toggle',
                            )
                        }
                    >
                        {plugin.enabled ? '停用' : '启用'}
                    </Button>
                    <Button
                        size="sm"
                        variant="ghost"
                        disabled={disabled || busy || !plugin.manageable}
                        onClick={() => onAction('/api/plugins/' + plugin.id + '/reload')}
                    >
                        重载
                    </Button>
                    {canUninstall && (
                        <Button
                            size="sm"
                            variant="ghost"
                            disabled={disabled || busy}
                            onClick={() =>
                                onAction(
                                    '/api/plugins/' + plugin.id + '/uninstall',
                                    '卸载 ' + plugin.name + '？插件目录会从数据目录移除。',
                                )
                            }
                        >
                            卸载
                        </Button>
                    )}
                </div>
            )}
        </li>
    );
};

export const NeoBotPluginsTab: React.FC<{
    instanceId: string;
    onGoTab: (tab: string) => void;
}> = ({ instanceId, onGoTab }) => {
    const query = usePanelJson(instanceId, 'plugins', '/api/plugins', parseNeoBotPlugins);
    const action = usePluginAction(instanceId);
    const [repo, setRepo] = useState('');
    const [notice, setNotice] = useState<string | null>(null);

    const run = (path: string, confirmText?: string) => {
        if (confirmText && !window.confirm(confirmText)) return;
        setNotice(null);
        action.mutate(
            { path },
            {
                onSuccess: () => setNotice('已提交，面板正在处理'),
                onError: (e) => setNotice(e instanceof Error ? e.message : String(e)),
            },
        );
    };

    const install = () => {
        const trimmed = repo.trim();
        if (!trimmed) return;
        setNotice(null);
        action.mutate(
            { path: '/api/plugins/install', body: { repo: trimmed } },
            {
                onSuccess: () => {
                    setRepo('');
                    setNotice('安装请求已提交，面板会在后台拉取仓库');
                },
                onError: (e) => setNotice(e instanceof Error ? e.message : String(e)),
            },
        );
    };

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
                            NeoBot 没有插件市场，装法是填一个 GitHub 仓库地址。
                            {!data.manageEnabled &&
                                '面板当前关闭了管理功能，这里只能看，装与启停要去面板开。'}
                        </p>
                        <div className="mt-3 flex items-end gap-2">
                            <TextField
                                className="flex-1"
                                label="插件仓库地址"
                                placeholder="https://github.com/作者/插件仓库"
                                value={repo}
                                disabled={!data.manageEnabled || action.isPending}
                                onValueChange={setRepo}
                            />
                            <Button
                                variant="primary"
                                size="sm"
                                disabled={
                                    !data.manageEnabled || action.isPending || !repo.trim()
                                }
                                onClick={install}
                            >
                                {action.isPending ? '提交中…' : '安装'}
                            </Button>
                        </div>
                        {notice && <p className="mt-2 text-2xs text-text-secondary">{notice}</p>}
                    </section>

                    <section className="flex flex-col gap-1.5">
                        <h4 className="text-2xs uppercase tracking-widest text-text-tertiary">
                            已安装 {data.items.length} 个
                        </h4>
                        <ul className="flex flex-col gap-1.5">
                            {data.items.map((p) => (
                                <PluginRow
                                    key={p.id}
                                    plugin={p}
                                    consolePlugin={data.consolePlugin}
                                    disabled={!data.manageEnabled}
                                    busy={action.isPending}
                                    onAction={run}
                                />
                            ))}
                        </ul>
                    </section>
                </div>
            )}
        </PanelStateView>
    );
};
