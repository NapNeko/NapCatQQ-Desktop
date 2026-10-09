import { useState } from 'react';
import { Button, Select, TextField } from '../../../../shared/ui';
import { asRecord } from '../../../../core/domain/apps/neobotPanel';
import {
    NeoBotPanelError,
    paramsPath,
    record,
    records,
    strings,
    text,
    type PanelObject,
} from '../../../../core/domain/apps/neobotWorkspace';
import { usePanelJson } from '../../../../hooks/apps/useNeoBotPanel';
import { useNeoBotAction } from '../../../../hooks/apps/useNeoBotAction';
import { useNeoBotDraftState } from '../../../../hooks/apps/useNeoBotDraftState';
import { useAppInstances } from '../../../../hooks/apps/useAppInstances';
import { NeoBotConfigEditor } from './NeoBotConfigTab';
import {
    ConfirmAction,
    ObjectView,
    PanelPage,
    PanelSection,
    type NeoBotPageProps,
} from './workspaceParts';

function PluginConfig({ name, ...props }: NeoBotPageProps & { name: string }) {
    const path = `/api/plugins/${encodeURIComponent(name)}/config`;
    const query = usePanelJson(props.instanceId, path, path, asRecord);
    return (
        <PanelPage query={query} onGoTab={props.onGoTab}>
            {(doc) => (
                <NeoBotConfigEditor
                    key={path}
                    instanceId={props.instanceId}
                    doc={doc}
                    path={path}
                    title={`${name} 配置`}
                />
            )}
        </PanelPage>
    );
}

function PluginsManager({ doc, ...props }: NeoBotPageProps & { doc: PanelObject }) {
    const action = useNeoBotAction(props.instanceId);
    const [install, setInstall] = useNeoBotDraftState('plugins:install', {
        repo: '',
        branch: 'main',
    });
    const [probeName, setProbeName] = useState('');
    const [result, setResult] = useState<PanelObject | null>(null);
    const [configName, setConfigName] = useNeoBotDraftState('plugins:configName', '');
    const [proxy, setProxy] = useNeoBotDraftState<PanelObject | null>('plugins:proxy', null);
    const extensions = usePanelJson(props.instanceId, 'extensions', '/api/extensions', asRecord);
    const { openWebUi } = useAppInstances();
    const busy = action.isPending;
    const manageable = doc.manage_enabled === true;
    const installable = manageable && doc.installer !== false;
    const conflict =
        action.error instanceof NeoBotPanelError ? record(record(action.error.data).conflict) : {};
    const installPlugin = async (dryRun: boolean, replace = false) => {
        const next = await action.run({
            path: '/api/plugins/install',
            body: { ...install, dry_run: dryRun, replace },
            quiet: dryRun,
        });
        if (next) {
            setResult(next);
            if (!dryRun) setInstall({ repo: '', branch: 'main' });
        }
    };
    const proxyDraft = proxy ?? record(doc.proxy);
    return (
        <div className="flex flex-col gap-4">
            <PanelSection title="安装第三方插件">
                <div className="grid gap-3 sm:grid-cols-[2fr_1fr]">
                    <TextField
                        label="GitHub 仓库地址"
                        value={install.repo}
                        disabled={busy || !manageable}
                        onValueChange={(repo) => setInstall({ ...install, repo })}
                    />
                    <TextField
                        label="分支"
                        value={install.branch}
                        disabled={busy || !manageable}
                        onValueChange={(branch) => setInstall({ ...install, branch })}
                    />
                </div>
                <div className="mt-3 flex flex-wrap gap-2">
                    <Button
                        size="sm"
                        variant="secondary"
                        disabled={busy || !installable || !install.repo.trim()}
                        onClick={() => void installPlugin(true)}
                    >
                        检查安装冲突
                    </Button>
                    <Button
                        size="sm"
                        variant="primary"
                        disabled={busy || !installable || !install.repo.trim()}
                        onClick={() => void installPlugin(false)}
                    >
                        安装插件
                    </Button>
                </div>
                {Object.keys(conflict).length > 0 && (
                    <div className="mt-3">
                        <ObjectView data={conflict} />
                        {conflict.reserved !== true &&
                            record(conflict.existing).official !== true && (
                                <ConfirmAction
                                    label="替换现有插件"
                                    description="将备份并替换已有插件目录，请核对两侧来源和版本。"
                                    disabled={busy || !manageable}
                                    onConfirm={() => void installPlugin(false, true)}
                                />
                            )}
                    </div>
                )}
                <details className="mt-3 text-xs text-text-secondary">
                    <summary className="cursor-pointer">探测插件 ID</summary>
                    <div className="mt-3 flex items-end gap-2">
                        <TextField label="插件 ID" value={probeName} onValueChange={setProbeName} />
                        <Button
                            size="sm"
                            variant="secondary"
                            disabled={busy || !probeName.trim()}
                            onClick={() =>
                                void action
                                    .run({
                                        path: paramsPath('/api/plugins/probe', { name: probeName }),
                                        method: 'GET',
                                        quiet: true,
                                    })
                                    .then(setResult)
                            }
                        >
                            探测
                        </Button>
                    </div>
                </details>
            </PanelSection>
            <PanelSection title="下载代理">
                <div className="grid gap-3 sm:grid-cols-3">
                    <Select
                        label="代理模式"
                        value={text(proxyDraft.mode) || 'system'}
                        disabled={busy || !manageable}
                        items={[
                            { value: 'system', label: '系统代理' },
                            { value: 'none', label: '直连' },
                            { value: 'custom', label: '自定义' },
                        ]}
                        onValueChange={(mode) => setProxy({ ...proxyDraft, mode })}
                    />
                    <TextField
                        label="代理主机"
                        value={text(proxyDraft.host)}
                        disabled={busy || !manageable}
                        onValueChange={(host) => setProxy({ ...proxyDraft, host })}
                    />
                    <TextField
                        label="代理端口"
                        type="number"
                        value={text(proxyDraft.port)}
                        disabled={busy || !manageable}
                        onValueChange={(port) => setProxy({ ...proxyDraft, port: Number(port) })}
                    />
                </div>
                <Button
                    className="mt-3"
                    size="sm"
                    variant="primary"
                    disabled={busy || !manageable || !proxy}
                    onClick={() =>
                        void action
                            .run({ path: '/api/plugins/proxy', body: proxyDraft })
                            .then((next) => {
                                if (next) setProxy(null);
                            })
                    }
                >
                    保存代理
                </Button>
            </PanelSection>
            <PanelSection
                title={`已安装 · ${records(doc.items).length}`}
                actions={
                    <Button
                        size="sm"
                        variant="secondary"
                        disabled={busy}
                        onClick={() =>
                            void action
                                .run({
                                    path: '/api/plugins/check-updates',
                                    method: 'GET',
                                    quiet: true,
                                })
                                .then(setResult)
                        }
                    >
                        检查更新
                    </Button>
                }
            >
                {!manageable && (
                    <p className="mb-3 text-xs text-warning">
                        插件管理已关闭，可在 dashboard 插件配置中开启。
                    </p>
                )}
                <div className="flex flex-col gap-3">
                    {records(doc.items).map((p) => {
                        const name = text(p.id) || text(p.name);
                        const root = `/api/plugins/${encodeURIComponent(name)}`;
                        const isConsole = name === text(doc.console_plugin);
                        const writable = manageable && p.manageable !== false && !isConsole;
                        return (
                            <div key={name} className="rounded-sm border border-border-subtle p-3">
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                    <h4 className="text-xs font-medium text-text">
                                        {text(p.name)} · {text(p.version)}{' '}
                                        {p.official ? '（官方）' : ''}
                                    </h4>
                                    <span className="text-2xs text-text-tertiary">
                                        {text(p.status)}
                                        {p.hot_reload === false ? ' · 需重启进程' : ''}
                                    </span>
                                </div>
                                {p.description ? (
                                    <p className="mt-2 text-xs text-text-secondary">
                                        {text(p.description)}
                                    </p>
                                ) : null}
                                {[
                                    text(p.error),
                                    text(p.config_error),
                                    text(p.disabled_reason),
                                    ...strings(p.dependency_issues),
                                ]
                                    .filter(Boolean)
                                    .map((warning) => (
                                        <p key={warning} className="mt-2 text-xs text-warning">
                                            {warning}
                                        </p>
                                    ))}
                                <div className="mt-3 flex flex-wrap gap-2">
                                    <ConfirmAction
                                        label={p.enabled ? '停用' : '启用'}
                                        description={`${p.enabled ? '停用' : '启用'} ${name}？依赖它的插件可能随之变化。`}
                                        disabled={busy || !writable}
                                        onConfirm={() =>
                                            void action.run({ path: `${root}/toggle` })
                                        }
                                    />
                                    <Button
                                        size="sm"
                                        variant="secondary"
                                        disabled={busy || !writable || p.hot_reload === false}
                                        onClick={() => void action.run({ path: `${root}/reload` })}
                                    >
                                        重载
                                    </Button>
                                    {!p.official && (
                                        <ConfirmAction
                                            label="更新"
                                            description={`更新 ${name} 的插件代码？`}
                                            disabled={busy || !writable}
                                            onConfirm={() =>
                                                void action.run({ path: `${root}/update` })
                                            }
                                        />
                                    )}
                                    {!p.official && (
                                        <ConfirmAction
                                            label="卸载"
                                            description={`移除 ${name} 的插件目录，依赖它的插件可能被停用。`}
                                            disabled={busy || !writable}
                                            onConfirm={() =>
                                                void action.run({ path: `${root}/uninstall` })
                                            }
                                        />
                                    )}
                                    <Button
                                        size="sm"
                                        variant="ghost"
                                        disabled={busy}
                                        onClick={() => setConfigName(name)}
                                    >
                                        配置
                                    </Button>
                                </div>
                                {strings(p.dependencies).length > 0 && (
                                    <p className="mt-2 text-2xs text-text-tertiary">
                                        依赖：{strings(p.dependencies).join('、')}
                                    </p>
                                )}
                            </div>
                        );
                    })}
                </div>
            </PanelSection>
            {result && (
                <PanelSection title="检查结果">
                    <ObjectView data={result} />
                </PanelSection>
            )}
            {configName && <PluginConfig {...props} name={configName} />}
            <PanelPage query={extensions} onGoTab={props.onGoTab}>
                {(data) => {
                    const pages = records(data.items).filter((extension) => {
                        const path = text(record(extension.panel).path);
                        return (
                            path.startsWith('/') &&
                            !path.startsWith('//') &&
                            !path.includes('..') &&
                            ![path, ...strings(extension.prefixes)].some((prefix) =>
                                /^\/(game|games|starship)(\/|$)/i.test(prefix),
                            )
                        );
                    });
                    return pages.length > 0 ? (
                        <PanelSection title="插件自定义页面">
                            {pages.map((extension) => {
                                const panel = record(extension.panel);
                                return (
                                    <Button
                                        key={text(extension.name)}
                                        size="sm"
                                        variant="secondary"
                                        onClick={() =>
                                            void openWebUi(props.instanceId, text(panel.path))
                                        }
                                    >
                                        {text(panel.title) || text(extension.name)}
                                    </Button>
                                );
                            })}
                        </PanelSection>
                    ) : null;
                }}
            </PanelPage>
        </div>
    );
}

export function NeoBotPluginsTab(props: NeoBotPageProps) {
    const query = usePanelJson(props.instanceId, 'plugins', '/api/plugins', asRecord);
    return (
        <PanelPage query={query} onGoTab={props.onGoTab}>
            {(doc) => <PluginsManager key={props.instanceId} {...props} doc={doc} />}
        </PanelPage>
    );
}
