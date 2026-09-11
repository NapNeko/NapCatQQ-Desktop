// AstrBot 详情页的 Tab 装配。表单类改动统一走顶部保存条；人格 / 知识库 / 规则的增删走 Dashboard 即时落库。
// 桌面端只编辑默认配置档案：读路径没有 conf_id，写别的档案等于把默认档案的内容盖过去。

import type { ReactNode } from 'react';
import { TabsContent } from '../../../../shared/ui';
import { NoneBot2StoreTab } from '../nonebot2/NoneBot2StoreTab';
import { AstrBotConnectionsTab } from './AstrBotConnectionsTab';
import { AstrBotModelsTab } from './AstrBotModelsTab';
import { AstrBotTalkTab } from './AstrBotTalkTab';
import { AstrBotPersonaTab } from './AstrBotPersonaTab';
import { AstrBotKbTab } from './AstrBotKbTab';
import { AstrBotSubagentTab } from './AstrBotSubagentTab';
import { AstrBotRulesTab } from './AstrBotRulesTab';
import { useAstrBotConfigForm } from '../useAstrBotConfigForm';
import { PaneLoadError, PaneLoading } from '../PaneStatus';
import { WebUiAccountCard } from '../WebUiAccountCard';
import type { FrameworkDetailProps, FrameworkUiModule } from '../frameworkUi';
import { useSyncFrameworkSaveHandle } from '../useSyncFrameworkSaveHandle';
import { useAppInstances } from '../../../../hooks/apps/useAppInstances';
import {
    useAstrBotDashboardStatus,
    useAstrBotKbs,
    useAstrBotPersonas,
} from '../../../../hooks/apps/useAstrBotDashboard';
import { dashboardReady } from './AstrBotRuntimeGate';

const EXTRA_TABS = [
    { value: 'connections', label: '连接' },
    { value: 'models', label: '模型' },
    { value: 'talk', label: '对话' },
    { value: 'persona', label: '人格' },
    { value: 'kb', label: '知识库' },
    { value: 'subagent', label: '子代理' },
    { value: 'rules', label: '规则' },
    { value: 'plugins', label: '插件' },
] as const;

// 人格 / 知识库页也能改配置（设默认、挂载），保存条得在；只有插件页是纯外部资源
const TYPED_TABS = new Set(['connections', 'models', 'talk', 'persona', 'kb', 'subagent', 'rules']);
const FILL_PANE = new Set(['plugins']);

function tabForIssue(path: string): string {
    if (path.startsWith('sources/') || path.startsWith('models/')) return 'models';
    if (
        path.startsWith('ai/') ||
        path.startsWith('stt/') ||
        path.startsWith('tts/') ||
        path.startsWith('websearch/') ||
        path.startsWith('kb/') ||
        path.startsWith('gates/')
    ) {
        return 'talk';
    }
    if (path.startsWith('subagent/')) return 'subagent';
    return 'connections';
}

function AstrBotFrameworkDetail({ instance, onSaveHandle, onGoTab }: FrameworkDetailProps) {
    const running = instance.state === 'running';
    const form = useAstrBotConfigForm(instance.id, true, instance.display_name, running);
    useSyncFrameworkSaveHandle(onSaveHandle, form);
    const apps = useAppInstances();
    const dash = useAstrBotDashboardStatus(instance.id, true);
    const live = dashboardReady(dash.data);
    const personas = useAstrBotPersonas(instance.id, live);
    const kbs = useAstrBotKbs(instance.id, live);

    const openPath = (path: string) => void apps.openWebUi(instance.id, path);
    const start = () => apps.start(instance.id);
    const starting = apps.pendingId === instance.id;

    const pane = (tab: string, body: (cfg: NonNullable<typeof form.form>) => ReactNode) =>
        form.isLoading && !form.form ? (
            <TabsContent value={tab} className="flex min-h-0 flex-1 flex-col pt-2">
                <PaneLoading text="正在读取配置…" />
            </TabsContent>
        ) : form.loadError && !form.form ? (
            <TabsContent value={tab} className="flex min-h-0 flex-1 flex-col pt-2">
                <PaneLoadError message="读取配置失败" onRetry={() => void form.reloadDiscard()} />
            </TabsContent>
        ) : form.form ? (
            <TabsContent value={tab} className="pb-8 pt-2">
                {body(form.form)}
            </TabsContent>
        ) : null;

    return (
        <>
            <TabsContent value="plugins" className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2">
                <NoneBot2StoreTab instance={instance} resource="plugin" />
            </TabsContent>
            {pane('connections', (cfg) => (
                <AstrBotConnectionsTab
                    config={cfg}
                    onChange={form.setForm}
                    errors={form.errors}
                    linked={!!instance.link}
                    disabled={form.saving}
                    onGoTab={onGoTab}
                    footer={<WebUiAccountCard instance={instance} />}
                />
            ))}
            {pane('models', (cfg) => (
                <AstrBotModelsTab
                    config={cfg}
                    saved={form.saved}
                    onChange={form.setForm}
                    errors={form.errors}
                    disabled={form.saving}
                    running={running}
                    instanceId={instance.id}
                />
            ))}
            {pane('talk', (cfg) => (
                <AstrBotTalkTab
                    config={cfg}
                    onChange={form.setForm}
                    disabled={form.saving}
                    personas={personas.data ?? []}
                    kbs={kbs.data ?? []}
                    live={live}
                    onGoTab={onGoTab}
                />
            ))}
            {pane('persona', (cfg) => (
                <AstrBotPersonaTab
                    instanceId={instance.id}
                    status={dash.data}
                    statusLoading={dash.isLoading}
                    defaultPersona={cfg.ai.default_personality}
                    onSetDefault={(id) => form.setForm({ ...cfg, ai: { ...cfg.ai, default_personality: id } })}
                    formDisabled={form.saving}
                    onGoTab={onGoTab}
                    onStart={start}
                    starting={starting}
                />
            ))}
            {pane('kb', (cfg) => (
                <AstrBotKbTab
                    instanceId={instance.id}
                    status={dash.data}
                    statusLoading={dash.isLoading}
                    config={cfg}
                    onChange={form.setForm}
                    formDisabled={form.saving}
                    onOpenWebUi={openPath}
                    onGoTab={onGoTab}
                    onStart={start}
                    starting={starting}
                />
            ))}
            {pane('subagent', (cfg) => (
                <AstrBotSubagentTab
                    instanceId={instance.id}
                    config={cfg}
                    onChange={form.setForm}
                    disabled={form.saving}
                    status={dash.data}
                    personas={personas.data ?? []}
                    onGoTab={onGoTab}
                />
            ))}
            {pane('rules', (cfg) => (
                <AstrBotRulesTab
                    instanceId={instance.id}
                    status={dash.data}
                    statusLoading={dash.isLoading}
                    config={cfg}
                    onOpenWebUi={openPath}
                    onGoTab={onGoTab}
                    onStart={start}
                    starting={starting}
                />
            ))}
        </>
    );
}

export const astrbotFrameworkUi: FrameworkUiModule = {
    extraTabs: EXTRA_TABS,
    defaultTab: 'connections',
    typedTabs: TYPED_TABS,
    fillPaneTabs: FILL_PANE,
    tabForIssue,
    Detail: AstrBotFrameworkDetail,
};
