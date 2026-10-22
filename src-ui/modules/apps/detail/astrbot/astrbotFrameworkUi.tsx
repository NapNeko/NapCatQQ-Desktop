// AstrBot 详情页的各页装配。表单类改动统一走底部保存条；人格 / 知识库 / 规则的增删走 Dashboard 即时落库。
// 桌面端只编辑默认配置档案：读路径没有 conf_id，写别的档案等于把默认档案的内容盖过去。

import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react';
import { TabsContent } from '../../../../shared/ui';
import { AppStoreTab } from '../AppStoreTab';
import {
    ASTRBOT_CONFIG_FORM,
    astrbotConfigWarnings,
    astrbotSetup,
} from '../../../../core/domain/apps/astrbotConfig';
import { useAppConfigForm } from '../../../../hooks/apps/useAppConfigForm';
import { AstrBotOverviewTab } from './AstrBotOverviewTab';
import { AstrBotConnectionsTab } from './AstrBotConnectionsTab';
import { AstrBotModelsTab } from './AstrBotModelsTab';
import { AstrBotTalkTab } from './AstrBotTalkTab';
import { AstrBotAdvancedTab, type AdvancedDrafts } from './AstrBotAdvancedTab';
import { advancedSections, advancedValueIssues, resolveAdvancedSchema } from '../../../../core/domain/apps/astrbotAdvanced';
import { AstrBotPersonaTab } from './AstrBotPersonaTab';
import { AstrBotKbTab } from './AstrBotKbTab';
import { AstrBotSubagentTab } from './AstrBotSubagentTab';
import { AstrBotRulesTab } from './AstrBotRulesTab';
import { PaneLoadError, PaneLoading } from '../PaneStatus';
import { WebUiAccountCard } from '../WebUiAccountCard';
import type { FrameworkDetailProps, FrameworkUiModule, NavBadgeTone } from '../frameworkUi';
import { useSyncFrameworkSaveHandle } from '../useSyncFrameworkSaveHandle';
import { useSyncNavBadges } from '../useSyncNavBadges';
import { useAppInstances } from '../../../../hooks/apps/useAppInstances';
import {
    useAstrBotConfigSchema,
    useAstrBotDashboardStatus,
    useAstrBotPersonas,
} from '../../../../hooks/apps/useAstrBotDashboard';
import { dashboardReady } from './AstrBotRuntimeGate';
import { ASTRBOT_NAV } from './astrbotNav';

// 概览能就地改配置（选默认模型、打开大模型、加提供商），人格 / 知识库页也能（设默认、挂载），
// 保存条都得在；只有插件页是纯外部资源
const TYPED_TABS = new Set([
    'overview',
    'connections',
    'models',
    'talk',
    'persona',
    'kb',
    'subagent',
    'rules',
    'advanced',
]);
const FILL_PANE = new Set(['plugins']);

/** 校验路径跳到能改它的那一页：默认模型在模型页、默认人格在人格页、知识库参数在知识库页 */
function tabForIssue(path: string): string {
    if (path.startsWith('advanced')) return 'advanced';
    if (path.startsWith('sources/') || path.startsWith('models/')) return 'models';
    if (path === 'ai/default_provider_id' || path.startsWith('ai/fallback_chat_models'))
        return 'models';
    if (path === 'ai/default_personality') return 'persona';
    if (path.startsWith('kb/')) return 'kb';
    if (
        path.startsWith('ai/') ||
        path.startsWith('stt/') ||
        path.startsWith('tts/') ||
        path.startsWith('websearch/') ||
        path.startsWith('gates/')
    ) {
        return 'talk';
    }
    if (path.startsWith('subagent/')) return 'subagent';
    return 'connections';
}

function AstrBotFrameworkDetail({
    instance,
    onSaveHandle,
    onGoTab,
    onOpenLink,
    onNavBadges,
}: FrameworkDetailProps) {
    const running = instance.state === 'running';
    const form = useAppConfigForm(ASTRBOT_CONFIG_FORM, instance.id, instance.display_name, running);
    const apps = useAppInstances();
    const dash = useAstrBotDashboardStatus(instance.id, true);
    const live = dashboardReady(dash.data);
    const schemaQuery = useAstrBotConfigSchema(instance.id, live);
    const schema = useMemo(() => resolveAdvancedSchema(schemaQuery.data), [schemaQuery.data]);
    const [advancedDrafts, setAdvancedDrafts] = useState<AdvancedDrafts>({});
    useEffect(() => setAdvancedDrafts({}), [instance.id]);
    const onAdvancedDraft = useCallback((path: string, text: string | null) => {
        setAdvancedDrafts((old) => {
            const next = { ...old };
            if (text === null) delete next[path];
            else next[path] = text;
            return next;
        });
    }, []);
    const advancedIssues = useMemo(() => [
        ...(form.form ? advancedValueIssues(form.form, advancedSections(form.form, schema)) : []),
        ...Object.keys(advancedDrafts).map((path) => ({
            path: 'advanced/' + path, message: 'JSON 格式有误，修正后才能保存',
        })),
    ], [form.form, schema, advancedDrafts]);
    useSyncFrameworkSaveHandle(onSaveHandle, {
        ...form,
        dirty: form.dirty || Object.keys(advancedDrafts).length > 0,
        clientIssues: [...form.clientIssues, ...advancedIssues],
        save: async (overwrite) => advancedIssues.length
            ? { kind: 'invalid', issues: advancedIssues }
            : form.save(overwrite),
        reset: () => { setAdvancedDrafts({}); form.reset(); },
        reloadDiscard: async () => { setAdvancedDrafts({}); await form.reloadDiscard(); },
    });
    const personas = useAstrBotPersonas(instance.id, live);

    // 侧栏圆点：下一步所在页亮品牌色，有配置冲突的页亮黄色；和概览状态卡同一套判定
    const badges: Record<string, NavBadgeTone> = {};
    if (form.form) {
        const setup = astrbotSetup(form.form, !!instance.link);
        if (setup.llmIssue === 'llm_off') badges.talk = 'next';
        else if (setup.llmIssue) badges.models = 'next';
        for (const w of astrbotConfigWarnings(form.form)) badges[w.area] ??= 'warn';
    }
    if (running && dash.data?.gate === 'auth') badges.connections ??= 'warn';
    useSyncNavBadges(onNavBadges, badges);

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
            <TabsContent
                value="plugins"
                className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2"
            >
                <AppStoreTab instance={instance} resource="plugin" />
            </TabsContent>
            {pane('overview', (cfg) => (
                <AstrBotOverviewTab
                    instance={instance}
                    config={cfg}
                    saved={form.saved}
                    onChange={form.setForm}
                    errors={form.errors}
                    disabled={form.saving}
                    dash={dash.data}
                    onGoTab={onGoTab}
                    onOpenLink={onOpenLink}
                    onStart={start}
                    starting={starting}
                    onOpenWebUi={(path) => void apps.openWebUi(instance.id, path)}
                />
            ))}
            {pane('connections', (cfg) => (
                <AstrBotConnectionsTab
                    config={cfg}
                    onChange={form.setForm}
                    errors={form.errors}
                    linked={!!instance.link}
                    disabled={form.saving}
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
            {pane('advanced', (cfg) => (
                <AstrBotAdvancedTab
                    instanceId={instance.id}
                    live={live}
                    config={cfg}
                    onChange={form.setForm}
                    drafts={advancedDrafts}
                    onDraft={onAdvancedDraft}
                    disabled={form.saving}
                />
            ))}
            {pane('talk', (cfg) => (
                <AstrBotTalkTab
                    config={cfg}
                    onChange={form.setForm}
                    disabled={form.saving}
                    onGoTab={onGoTab}
                />
            ))}
            {pane('persona', (cfg) => (
                <AstrBotPersonaTab
                    instanceId={instance.id}
                    status={dash.data}
                    statusLoading={dash.isLoading}
                    defaultPersona={cfg.ai.default_personality}
                    onSetDefault={(id) =>
                        form.setForm({ ...cfg, ai: { ...cfg.ai, default_personality: id } })
                    }
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
    nav: ASTRBOT_NAV,
    defaultTab: 'overview',
    typedTabs: TYPED_TABS,
    fillPaneTabs: FILL_PANE,
    tabForIssue,
    Detail: AstrBotFrameworkDetail,
};
