// MaiBot 详情页装配：概览 / 聊天名单 / 连接三页都吃同一份类型化配置，改动走底部保存条。

import type { ReactNode } from 'react';
import { TabsContent } from '../../../../shared/ui';
import { maibotChatDropsEverything } from '../../../../core/domain/apps/maibotConfig';
import { useAppInstances } from '../../../../hooks/apps/useAppInstances';
import { PaneLoadError, PaneLoading } from '../PaneStatus';
import { useMaiBotConfigForm } from '../useMaiBotConfigForm';
import { useSyncFrameworkSaveHandle } from '../useSyncFrameworkSaveHandle';
import { useSyncNavBadges } from '../useSyncNavBadges';
import type {
    FrameworkDetailProps,
    FrameworkNavGroup,
    FrameworkUiModule,
    NavBadgeTone,
} from '../frameworkUi';
import { MaiBotOverviewTab } from './MaiBotOverviewTab';
import { MaiBotChatTab } from './MaiBotChatTab';
import { MaiBotConnectionTab } from './MaiBotConnectionTab';

const NAV: readonly FrameworkNavGroup[] = [
    { id: 'home', items: [{ value: 'overview', label: '概览' }] },
    { id: 'message', label: '消息', items: [{ value: 'chat', label: '聊天名单' }] },
    { id: 'instance', label: '实例', items: [{ value: 'connection', label: '连接' }] },
];

const TYPED_TABS = new Set(['overview', 'chat', 'connection']);
const FILL_PANE = new Set<string>();

function tabForIssue(path: string): string {
    return path.startsWith('adapter/chat/') ? 'chat' : 'connection';
}

function MaiBotFrameworkDetail({ instance, onSaveHandle, onGoTab, onOpenLink, onNavBadges }: FrameworkDetailProps) {
    const form = useMaiBotConfigForm(instance.id, true, instance.display_name);
    useSyncFrameworkSaveHandle(onSaveHandle, form);
    const apps = useAppInstances();

    // 和概览状态卡同一套判定：名单把消息全丢了就在「聊天名单」上亮下一步
    const badges: Record<string, NavBadgeTone> = {};
    const chat = form.form?.adapter?.chat;
    if (chat && maibotChatDropsEverything(chat)) badges.chat = 'next';
    useSyncNavBadges(onNavBadges, badges);

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
            {pane('chat', (cfg) => (
                <MaiBotChatTab
                    config={cfg}
                    onChange={form.setForm}
                    errors={form.errors}
                    disabled={form.saving}
                />
            ))}
            {pane('connection', (cfg) => (
                <MaiBotConnectionTab
                    instance={instance}
                    config={cfg}
                    onChange={form.setForm}
                    errors={form.errors}
                    disabled={form.saving}
                />
            ))}
        </>
    );
}

export const maibotFrameworkUi: FrameworkUiModule = {
    nav: NAV,
    defaultTab: 'overview',
    typedTabs: TYPED_TABS,
    fillPaneTabs: FILL_PANE,
    tabForIssue,
    Detail: MaiBotFrameworkDetail,
};
