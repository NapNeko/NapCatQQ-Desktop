// MaiBot 详情页装配：所有页吃同一份类型化配置（两份主配置 + 适配器名单），改动走底部保存条。
// 模型页和聊天名单、连接是手写的；其余配置页照 maibotPages 的定义由 schema 铺出来。

import { useState, type ReactNode } from 'react';
import { TabsContent } from '../../../../shared/ui';
import { maibotChatDropsEverything, maibotModelSetupIssue } from '../../../../core/domain/apps/maibotConfig';
import { useAppInstances } from '../../../../hooks/apps/useAppInstances';
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
const FILL_PANE = new Set<string>();

function MaiBotFrameworkDetail({ instance, onSaveHandle, onGoTab, onOpenLink, onNavBadges }: FrameworkDetailProps) {
    const form = useMaiBotConfigForm(instance.id, true, instance.display_name);
    useSyncFrameworkSaveHandle(onSaveHandle, form);
    const apps = useAppInstances();
    // 高级选项开关整个详情页共用：在模型页打开了，切到别的页还开着
    const [showAdvanced, setShowAdvanced] = useState(false);

    // 和概览状态卡同一套判定：模型没配好亮在「模型」，名单把消息全丢了亮在「聊天名单」
    const badges: Record<string, NavBadgeTone> = {};
    if (form.form) {
        if (maibotModelSetupIssue(form.form.models)) badges.models = 'next';
        const chat = form.form.adapter?.chat;
        if (chat && maibotChatDropsEverything(chat)) badges.chat = 'next';
    }
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

    return (
        <>
            {pane('overview', (cfg) => (
                <MaiBotOverviewTab
                    instance={instance}
                    config={cfg}
                    onGoTab={onGoTab}
                    onOpenLink={onOpenLink}
                    onStart={() => apps.start(instance.id)}
                    starting={apps.pendingId === instance.id}
                    onOpenWebUi={() => void apps.openWebUi(instance.id)}
                />
            ))}
            {pane('models', (cfg) => (
                <MaiBotModelsTab
                    config={cfg}
                    {...common}
                    showAdvanced={showAdvanced}
                    onShowAdvanced={setShowAdvanced}
                />
            ))}
            {pane('chat', (cfg) => <MaiBotChatTab config={cfg} {...common} />)}
            {pane('connection', (cfg) => <MaiBotConnectionTab instance={instance} config={cfg} {...common} />)}
            {Object.entries(MAIBOT_SCHEMA_PAGES).map(([tab, page]) =>
                pane(tab, (cfg) => (
                    <MaiBotSchemaTab
                        page={page}
                        config={cfg}
                        {...common}
                        showAdvanced={showAdvanced}
                        onShowAdvanced={setShowAdvanced}
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
