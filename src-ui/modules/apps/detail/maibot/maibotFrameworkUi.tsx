// MaiBot 详情页装配：所有页吃同一份类型化配置（两份主配置 + 适配器名单），改动走底部保存条。
// 模型页和聊天名单、连接是手写的；其余配置页照 maibotPages 的定义由 schema 铺出来。

import { useState, type ReactNode } from 'react';
import { TabsContent } from '../../../../shared/ui';
import { maibotChatDropsEverything, maibotModelSetupIssue } from '../../../../core/domain/apps/maibotConfig';
import { useAppInstances } from '../../../../hooks/apps/useAppInstances';
import {
    useMaiBotChatSessions,
    useMaiBotMcpStatus,
    useMaiBotStatus,
} from '../../../../hooks/apps/useMaiBotRuntime';
import { McpStatusPanel } from './maibotProbes';
import { useAdvancedSections } from './advancedToggle';
import { MaiBotPromptsTab } from './MaiBotPromptsTab';
import { MaiBotExpressionsTab } from './MaiBotExpressionsTab';
import { MaiBotJargonTab } from './MaiBotJargonTab';
import { MaiBotEmojisTab } from './MaiBotEmojisTab';
import { MaiBotPersonsTab } from './MaiBotPersonsTab';
import { MaiBotKnowledgeTab } from './MaiBotKnowledgeTab';
import { MaiBotBehaviorTab } from './MaiBotBehaviorTab';
import { MaiBotTryChatTab } from './MaiBotTryChatTab';
import type { KnowledgeView } from './maibotKnowledgeParts';
import { usePromptDrafts } from './maibotPromptDrafts';
import { AppStoreTab } from '../AppStoreTab';
import { PaneLoadError, PaneLoading } from '../PaneStatus';
import { useMaiBotConfigForm } from '../useMaiBotConfigForm';
import { useSyncFrameworkSaveHandle } from '../useSyncFrameworkSaveHandle';
import { useSyncNavBadges } from '../useSyncNavBadges';
import type { FrameworkDetailProps, FrameworkUiModule, NavBadgeTone } from '../frameworkUi';
import { MaiBotOverviewTab } from './MaiBotOverviewTab';
import { MaiBotChatTab } from './MaiBotChatTab';
import { MaiBotConnectionTab } from './MaiBotConnectionTab';
import { MaiBotModelsTab } from './MaiBotModelsTab';
import { MaiBotSchemaTab } from './MaiBotSchemaTab';
import { MAIBOT_NAV, MAIBOT_SCHEMA_PAGES, maibotTabForIssue } from './maibotPages';

const TYPED_TABS = new Set(['overview', 'models', 'chat', 'connection', ...Object.keys(MAIBOT_SCHEMA_PAGES)]);
// 插件商店、提示词、资源页这些自己落盘，不是表单：铺满内容区、不挂保存条
const FILL_PANE = new Set([
    'trychat',
    'plugins',
    'prompts',
    'emoji',
    'expressions',
    'jargon',
    'persons',
    'knowledge',
    'behavior',
]);

function MaiBotFrameworkDetail({ instance, onSaveHandle, onGoTab, onOpenLink, onNavBadges }: FrameworkDetailProps) {
    const form = useMaiBotConfigForm(instance.id, true, instance.display_name);
    useSyncFrameworkSaveHandle(onSaveHandle, form);
    const apps = useAppInstances();
    const advancedSections = useAdvancedSections();
    const promptDrafts = usePromptDrafts();
    // 知识库在哪一块；「记忆」页的「记忆图谱」「导入知识」直接跳到对应那块
    const [knowledgeView, setKnowledgeView] = useState<KnowledgeView>('browse');
    const goTab = (tab: string, view?: string) => {
        if (tab === 'knowledge' && (view === 'import' || view === 'browse' || view === 'graph')) setKnowledgeView(view);
        onGoTab(tab);
    };

    // 运行期：WebUI 应答了才去拉会话、MCP 状态
    const running = instance.state === 'running';
    const status = useMaiBotStatus(instance.id, running).data;
    const live = status?.gate === 'ok';
    const sessions = useMaiBotChatSessions(instance.id, live);
    const mcpServers = form.form?.bot.mcp.servers ?? [];
    const mcp = useMaiBotMcpStatus(instance.id, live && mcpServers.length > 0);

    // 和概览状态卡同一套判定：模型没配好亮在「模型」，名单把消息全丢了亮在「聊天名单」
    const badges: Record<string, NavBadgeTone> = {};
    if (form.form) {
        if (maibotModelSetupIssue(form.form.models)) badges.models = 'next';
        const chat = form.form.adapter?.chat;
        if (chat && maibotChatDropsEverything(chat)) badges.chat = 'next';
    }
    // 提示词有没保存的草稿：切走了也提醒一下
    if (promptDrafts.keys.length > 0) badges.prompts = 'warn';
    useSyncNavBadges(onNavBadges, badges);

    const pane = (tab: string, body: (cfg: NonNullable<typeof form.form>) => ReactNode) =>
        form.isLoading && !form.form ? (
            <TabsContent key={tab} value={tab} className="flex min-h-0 flex-1 flex-col pt-2">
                <PaneLoading text="正在读取配置…" />
            </TabsContent>
        ) : form.loadError && !form.form ? (
            <TabsContent key={tab} value={tab} className="flex min-h-0 flex-1 flex-col pt-2">
                <PaneLoadError message="读取配置失败" onRetry={() => void form.reloadDiscard()} />
            </TabsContent>
        ) : form.form ? (
            <TabsContent key={tab} value={tab} className="pb-8 pt-2">
                {body(form.form)}
            </TabsContent>
        ) : null;

    const common = { onChange: form.setForm, errors: form.errors, disabled: form.saving };
    // 资源页没连上时给的「启动麦麦」
    const startProps = { onStart: () => apps.start(instance.id), starting: apps.pendingId === instance.id };

    return (
        <>
            <TabsContent value="trychat" className="flex min-h-0 flex-1 flex-col overflow-hidden pb-3 pt-2">
                <MaiBotTryChatTab instance={instance} status={status} {...startProps} />
            </TabsContent>
            <TabsContent value="plugins" className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2">
                <AppStoreTab instance={instance} resource="plugin" />
            </TabsContent>
            <TabsContent value="prompts" className="flex min-h-0 flex-1 flex-col overflow-hidden pb-3 pt-2">
                <MaiBotPromptsTab instance={instance} status={status} drafts={promptDrafts} {...startProps} />
            </TabsContent>
            <TabsContent value="expressions" className="flex min-h-0 flex-1 flex-col overflow-hidden pb-3 pt-2">
                <MaiBotExpressionsTab
                    instance={instance}
                    status={status}
                    // 读不到配置时按上游默认（开着）说
                    curatedOnly={form.form?.bot.expression.expression_checked_only ?? true}
                    {...startProps}
                />
            </TabsContent>
            <TabsContent value="jargon" className="flex min-h-0 flex-1 flex-col overflow-hidden pb-3 pt-2">
                <MaiBotJargonTab instance={instance} status={status} {...startProps} />
            </TabsContent>
            <TabsContent value="emoji" className="flex min-h-0 flex-1 flex-col overflow-hidden pb-3 pt-2">
                <MaiBotEmojisTab
                    instance={instance}
                    status={status}
                    config={form.form?.bot.emoji}
                    onGoTab={onGoTab}
                    {...startProps}
                />
            </TabsContent>
            <TabsContent value="persons" className="flex min-h-0 flex-1 flex-col overflow-hidden pb-3 pt-2">
                <MaiBotPersonsTab instance={instance} status={status} {...startProps} />
            </TabsContent>
            <TabsContent value="knowledge" className="flex min-h-0 flex-1 flex-col overflow-hidden pb-3 pt-2">
                <MaiBotKnowledgeTab
                    instance={instance}
                    status={status}
                    view={knowledgeView}
                    onView={setKnowledgeView}
                    onGoTab={onGoTab}
                    {...startProps}
                />
            </TabsContent>
            <TabsContent value="behavior" className="flex min-h-0 flex-1 flex-col overflow-hidden pb-3 pt-2">
                <MaiBotBehaviorTab
                    instance={instance}
                    status={status}
                    // 读不到配置时按上游默认（关着）说
                    learningOn={form.form?.bot.experimental.enable_behavior_learning ?? false}
                    onGoTab={onGoTab}
                    onOpenWebUi={(path) => void apps.openWebUi(instance.id, path)}
                    {...startProps}
                />
            </TabsContent>
            {pane('overview', (cfg) => (
                <MaiBotOverviewTab
                    instance={instance}
                    config={cfg}
                    onGoTab={onGoTab}
                    onOpenLink={onOpenLink}
                    onStart={() => apps.start(instance.id)}
                    starting={apps.pendingId === instance.id}
                    onOpenWebUi={(path) => void apps.openWebUi(instance.id, path)}
                />
            ))}
            {pane('models', (cfg) => (
                <MaiBotModelsTab
                    config={cfg}
                    {...common}
                    advancedSections={advancedSections}
                    instanceId={instance.id}
                    live={live}
                />
            ))}
            {pane('chat', (cfg) => <MaiBotChatTab config={cfg} {...common} />)}
            {pane('connection', (cfg) => <MaiBotConnectionTab instance={instance} config={cfg} {...common} />)}
            {Object.entries(MAIBOT_SCHEMA_PAGES).map(([tab, page]) =>
                pane(tab, (cfg) => (
                    <MaiBotSchemaTab
                        tab={tab}
                        page={page}
                        config={cfg}
                        {...common}
                        advancedSections={advancedSections}
                        chatTargets={live ? sessions.data : undefined}
                        live={live}
                        onOpenWebUi={(path) => void apps.openWebUi(instance.id, path)}
                        onGoTab={goTab}
                        intro={
                            tab === 'mcp' && live ? (
                                <McpStatusPanel instanceId={instance.id} servers={cfg.bot.mcp.servers} status={mcp.data} />
                            ) : undefined
                        }
                    />
                )),
            )}
        </>
    );
}

export const maibotFrameworkUi: FrameworkUiModule = {
    nav: MAIBOT_NAV,
    defaultTab: 'overview',
    typedTabs: TYPED_TABS,
    fillPaneTabs: FILL_PANE,
    tabForIssue: maibotTabForIssue,
    Detail: MaiBotFrameworkDetail,
};
