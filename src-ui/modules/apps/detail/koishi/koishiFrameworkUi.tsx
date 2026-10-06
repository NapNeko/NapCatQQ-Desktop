// Koishi 详情页装配：服务器、全局设置、插件树、连接吃同一份类型化配置（整份 koishi.yml），
// 改动走底部保存条；运行中保存由后端拆成控制台操作当场生效。插件市场自己装卸，不挂保存条。
// 试聊 / 指令 / 数据库 / 文件是控制台功能的原生版（沙盒协议、指令管理器、dataview、explorer），
// 改完即生效，也不挂保存条。

import { useState, type ReactNode } from 'react';
import { TabsContent } from '../../../../shared/ui';
import {
    KOISHI_CONFIG_FORM,
    effective,
    isLinkNode,
    nodePathOfIssue,
} from '../../../../core/domain/apps/koishiConfig';
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
import { KoishiOverviewTab } from './KoishiOverviewTab';
import { KoishiServerTab } from './KoishiServerTab';
import { KoishiGlobalTab } from './KoishiGlobalTab';
import { KoishiPluginsTab } from './KoishiPluginsTab';
import { KoishiConnectionTab } from './KoishiConnectionTab';
import { KoishiSandboxTab } from './KoishiSandboxTab';
import { KoishiCommandsTab } from './KoishiCommandsTab';
import { KoishiDatabaseTab } from './KoishiDatabaseTab';
import { KoishiFilesTab } from './KoishiFilesTab';

const NAV: readonly FrameworkNavGroup[] = [
    { id: 'overview', items: [{ value: 'overview', label: '概览' }] },
    {
        id: 'config',
        label: '配置',
        items: [
            { value: 'server', label: '服务器' },
            { value: 'global', label: '全局设置' },
        ],
    },
    {
        id: 'extend',
        label: '扩展',
        items: [
            { value: 'plugins', label: '插件' },
            { value: 'market', label: '插件市场' },
        ],
    },
    {
        id: 'tools',
        label: '工具',
        items: [
            { value: 'sandbox', label: '试聊' },
            { value: 'commands', label: '指令' },
            { value: 'database', label: '数据库' },
            { value: 'files', label: '文件' },
        ],
    },
    { id: 'instance', label: '实例', items: [{ value: 'connection', label: '连接' }] },
];

const TYPED_TABS = new Set(['overview', 'server', 'global', 'plugins', 'connection']);
const FILL_PANE = new Set(['plugins', 'market', 'sandbox', 'database', 'files']);

function tabForIssue(path: string): string {
    if (path.startsWith('server/')) return 'server';
    if (nodePathOfIssue(path)) return 'plugins';
    return 'global';
}

function KoishiFrameworkDetail({
    instance,
    onSaveHandle,
    onGoTab,
    onOpenLink,
    onNavBadges,
}: FrameworkDetailProps) {
    const running = instance.state === 'running';
    const form = useAppConfigForm(KOISHI_CONFIG_FORM, instance.id, instance.display_name, running);
    useSyncFrameworkSaveHandle(onSaveHandle, form);
    const apps = useAppInstances();
    const [focus, setFocus] = useState<{ name: string; seq: number } | null>(null);

    const badges: Record<string, NavBadgeTone> = {};
    // 对接过了但对接条目被停用：Bot 连得上端口，Koishi 不认
    if (form.form && instance.link && !effective(form.form.plugins).some(isLinkNode))
        badges.plugins = 'warn';
    useSyncNavBadges(onNavBadges, badges);

    const pane = (
        tab: string,
        body: (cfg: NonNullable<typeof form.form>) => ReactNode,
        fill = false,
    ) =>
        form.isLoading && !form.form ? (
            <TabsContent key={tab} value={tab} className="flex min-h-0 flex-1 flex-col pt-2">
                <PaneLoading text="正在读取 koishi.yml…" />
            </TabsContent>
        ) : form.loadError && !form.form ? (
            <TabsContent key={tab} value={tab} className="flex min-h-0 flex-1 flex-col pt-2">
                <PaneLoadError message="读取配置失败" onRetry={() => void form.reloadDiscard()} />
            </TabsContent>
        ) : form.form ? (
            <TabsContent
                key={tab}
                value={tab}
                className={fill ? 'flex min-h-0 flex-1 flex-col overflow-hidden pt-2' : 'pb-8 pt-2'}
            >
                {body(form.form)}
            </TabsContent>
        ) : null;

    const common = { onChange: form.setForm, disabled: form.saving };

    return (
        <>
            <TabsContent
                value="market"
                className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2"
            >
                <AppStoreTab
                    instance={instance}
                    resource="plugin"
                    onConfigure={(_id, name) => {
                        setFocus((f) => ({ name, seq: (f?.seq ?? 0) + 1 }));
                        onGoTab('plugins');
                    }}
                />
            </TabsContent>
            <TabsContent
                value="sandbox"
                className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2"
            >
                <KoishiSandboxTab instance={instance} />
            </TabsContent>
            <TabsContent value="commands" className="pb-8 pt-2">
                <KoishiCommandsTab instance={instance} />
            </TabsContent>
            <TabsContent
                value="database"
                className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2"
            >
                <KoishiDatabaseTab instance={instance} />
            </TabsContent>
            <TabsContent
                value="files"
                className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2"
            >
                <KoishiFilesTab instance={instance} />
            </TabsContent>
            {pane('overview', (cfg) => (
                <KoishiOverviewTab
                    instance={instance}
                    config={cfg}
                    onGoTab={onGoTab}
                    onOpenLink={onOpenLink}
                    onStart={() => apps.start(instance.id)}
                    starting={apps.pendingId === instance.id}
                />
            ))}
            {pane('server', (cfg) => (
                <KoishiServerTab
                    config={cfg}
                    errors={form.errors}
                    running={running}
                    onOpenWebUi={() => void apps.openWebUi(instance.id)}
                    {...common}
                />
            ))}
            {pane('global', (cfg) => (
                <KoishiGlobalTab instance={instance} config={cfg} {...common} />
            ))}
            {pane(
                'plugins',
                (cfg) => (
                    <KoishiPluginsTab
                        instance={instance}
                        config={cfg}
                        errors={form.errors}
                        onGoMarket={() => onGoTab('market')}
                        focus={focus}
                        {...common}
                    />
                ),
                true,
            )}
            {pane('connection', (cfg) => (
                <KoishiConnectionTab
                    instance={instance}
                    config={cfg}
                    onGoTab={onGoTab}
                    onOpenLink={onOpenLink}
                />
            ))}
        </>
    );
}

export const koishiFrameworkUi: FrameworkUiModule = {
    nav: NAV,
    defaultTab: 'overview',
    typedTabs: TYPED_TABS,
    fillPaneTabs: FILL_PANE,
    tabForIssue,
    Detail: KoishiFrameworkDetail,
};
