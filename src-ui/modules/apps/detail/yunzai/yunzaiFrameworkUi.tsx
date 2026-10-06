// 云崽详情页装配：配置页吃同一份类型化配置（config/config 下几份 yaml），改动走底部保存条。
// 插件页是通用商店，按插件索引的分表加了分类筛选。

import type { ReactNode } from 'react';
import { TabsContent } from '../../../../shared/ui';
import {
    YUNZAI_CONFIG_FORM,
    YUNZAI_STORE_CATEGORIES,
    yunzaiNeedsMaster,
    yunzaiPluginToggleable,
} from '../../../../core/domain/apps/yunzaiConfig';
import { useAppConfigForm } from '../../../../hooks/apps/useAppConfigForm';
import { useAppInstances } from '../../../../hooks/apps/useAppInstances';
import { AppStoreTab } from '../AppStoreTab';
import { PaneLoadError, PaneLoading } from '../PaneStatus';
import { useSyncFrameworkSaveHandle } from '../useSyncFrameworkSaveHandle';
import { useSyncNavBadges } from '../useSyncNavBadges';
import type {
    FrameworkDetailProps,
    FrameworkNavGroup,
    FrameworkUiModule,
    NavBadgeTone,
} from '../frameworkUi';
import { YunzaiBasicTab } from './YunzaiBasicTab';
import { YunzaiConnectionTab } from './YunzaiConnectionTab';
import { YunzaiGroupsTab } from './YunzaiGroupsTab';
import { YunzaiOverviewTab } from './YunzaiOverviewTab';
import { YunzaiPermissionsTab } from './YunzaiPermissionsTab';
import { YunzaiRenderTab } from './YunzaiRenderTab';

const NAV: readonly FrameworkNavGroup[] = [
    { id: 'home', items: [{ value: 'overview', label: '概览' }] },
    {
        id: 'config',
        label: '配置',
        items: [
            { value: 'basic', label: '基础' },
            { value: 'permissions', label: '主人与权限' },
            { value: 'groups', label: '群聊' },
            { value: 'render', label: '渲染' },
        ],
    },
    { id: 'extend', label: '扩展', items: [{ value: 'plugins', label: '插件' }] },
    { id: 'instance', label: '实例', items: [{ value: 'connection', label: '连接' }] },
];

const TYPED_TABS = new Set(['overview', 'basic', 'permissions', 'groups', 'render', 'connection']);
// 插件商店自己落盘，不是表单：铺满内容区、不挂保存条
const FILL_PANE = new Set(['plugins']);

// bot.yaml 里浏览器那几项放在「渲染」页
const RENDER_BOT_FIELDS = ['bot/chromium_path', 'bot/puppeteer_ws', 'bot/puppeteer_timeout'];

function tabForIssue(path: string): string {
    if (RENDER_BOT_FIELDS.some((p) => path === p || path.startsWith(`${p}/`))) return 'render';
    switch (path.split('/')[0]) {
        case 'other':
            return 'permissions';
        case 'group':
            return 'groups';
        case 'server':
        case 'redis':
            return 'connection';
        case 'renderer':
            return 'render';
        default:
            return 'basic';
    }
}

function YunzaiFrameworkDetail({
    instance,
    onSaveHandle,
    onGoTab,
    onOpenLink,
    onNavBadges,
}: FrameworkDetailProps) {
    const form = useAppConfigForm(YUNZAI_CONFIG_FORM, instance.id, instance.display_name);
    useSyncFrameworkSaveHandle(onSaveHandle, form);
    const apps = useAppInstances();

    // 和概览状态卡同一套判定：没设主人亮在「主人与权限」
    const badges: Record<string, NavBadgeTone> = {};
    if (form.form) {
        if (yunzaiNeedsMaster(form.form)) badges.permissions = 'next';
        // auth 里有 NapCat 带不上的头，连不上；重新对接会去掉
        if (form.form.server.extra_auth_headers.length > 0) badges.connection = 'warn';
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
            <TabsContent
                value="plugins"
                className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2"
            >
                <AppStoreTab
                    instance={instance}
                    resource="plugin"
                    officialLabel="推荐"
                    categories={YUNZAI_STORE_CATEGORIES}
                    toggleable={yunzaiPluginToggleable}
                    toggleBlockedReason="目录插件整个加载，不能单独停，不要就卸载"
                />
            </TabsContent>
            {pane('overview', (cfg) => (
                <YunzaiOverviewTab
                    instance={instance}
                    config={cfg}
                    onGoTab={onGoTab}
                    onOpenLink={onOpenLink}
                    onStart={() => apps.start(instance.id)}
                    starting={apps.pendingId === instance.id}
                />
            ))}
            {pane('basic', (cfg) => (
                <YunzaiBasicTab config={cfg} {...common} />
            ))}
            {pane('permissions', (cfg) => (
                <YunzaiPermissionsTab config={cfg} {...common} />
            ))}
            {pane('groups', (cfg) => (
                <YunzaiGroupsTab config={cfg} {...common} />
            ))}
            {pane('render', (cfg) => (
                <YunzaiRenderTab config={cfg} {...common} />
            ))}
            {pane('connection', (cfg) => (
                <YunzaiConnectionTab instance={instance} config={cfg} {...common} />
            ))}
        </>
    );
}

export const yunzaiFrameworkUi: FrameworkUiModule = {
    nav: NAV,
    defaultTab: 'overview',
    typedTabs: TYPED_TABS,
    fillPaneTabs: FILL_PANE,
    tabForIssue,
    Detail: YunzaiFrameworkDetail,
};
