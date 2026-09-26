// 应用端实例详情：头部 + 左侧分组导航 + 内容区 + 底部保存条。
// 框架专有页走 registry，未知框架只有原始文件和日志。

import { useEffect, useMemo, useState } from 'react';
import { ArrowLeft, Download, ListChecks } from 'lucide-react';
import { Button, PagePlaceholder, Tabs, TabsContent, TooltipProvider } from '../../../shared/ui';
import { ActionMotionIcon, EMPHASIS_MOTION } from '../../../shared/ui/motion';
import { useServerManager } from '../../../hooks/remote/useServerManager';
import { useAppFrameworks, useAppInstances } from '../../../hooks/apps/useAppInstances';
import { AppLinkDialog } from '../AppLinkDialog';
import { DetailInstallProgress } from '../InstallProgress';
import { DeleteInstanceDialog } from '../DeleteInstanceDialog';
import { hostIdDisplayLabel } from '../hostLabel';
import { isInstalled } from '../instanceState';
import { cn } from '../../../shared/utils/cn';
import { ConfigConflictDialog } from './ConfigConflictDialog';
import { DetailHeader } from './DetailHeader';
import { DetailSideNav } from './DetailSideNav';
import { InstanceLogTab } from './InstanceLogTab';
import { PaneLoading } from './PaneStatus';
import { RawFilesTab } from './RawFilesTab';
import { SaveBar } from './SaveBar';
import { buildDetailNav, resolveFrameworkUi, type FrameworkSaveHandle, type NavBadges } from './frameworkUi';
import type { DetailTabHint } from '../list/AppInstanceListPage';
import type { AppConfigIssue, AppInstance } from '../../../core/ipc/types';

export interface AppInstancePageNextProps {
    instanceId: string;
    initialTab?: DetailTabHint;
    onBack: () => void;
    /** 安装中「在任务队列查看」要跳走；没给就不显示这个按钮 */
    onViewTasks?: () => void;
}

export const AppInstancePageNext: React.FC<AppInstancePageNextProps> = ({
    instanceId,
    initialTab,
    onBack,
    onViewTasks,
}) => {
    const apps = useAppInstances();
    const frameworks = useAppFrameworks();
    const { servers } = useServerManager();

    const instance = apps.instances.find((i) => i.id === instanceId) ?? null;
    const manifest = useMemo(
        () => (frameworks.data ?? []).find((m) => m.id === instance?.framework_id),
        [frameworks.data, instance?.framework_id],
    );

    const ui = instance ? resolveFrameworkUi(instance.framework_id) : undefined;
    const nav = useMemo(() => buildDetailNav(ui), [ui]);
    const installed = !!instance && isInstalled(instance);
    const running = instance?.state === 'running';
    const FrameworkDetail = ui?.Detail;

    const defaultTab = initialTab === 'log' ? 'log' : installed && ui ? ui.defaultTab : 'raw';
    const [activeTab, setActiveTab] = useState(defaultTab);
    const [userSwitched, setUserSwitched] = useState(false);
    const [saveHandle, setSaveHandle] = useState<FrameworkSaveHandle | null>(null);
    const [navBadges, setNavBadges] = useState<NavBadges>({});

    useEffect(() => {
        if (userSwitched || !instance) return;
        setActiveTab(initialTab === 'log' ? 'log' : installed && ui ? ui.defaultTab : 'raw');
    }, [instance, installed, ui, initialTab, userSwitched]);

    // 换实例时别把上一个框架的圆点带过来
    useEffect(() => setNavBadges({}), [instanceId]);

    const [linkOpen, setLinkOpen] = useState(false);
    const [deleteOpen, setDeleteOpen] = useState(false);

    const goTab = (tab: string) => {
        setUserSwitched(true);
        setActiveTab(tab);
    };

    const jumpToFirstIssue = (issues: AppConfigIssue[]) => {
        const first = issues[0];
        if (!first || !ui) return;
        goTab(ui.tabForIssue(first.path));
    };

    const handleSave = async (overwrite = false) => {
        if (!saveHandle) return;
        const outcome = await saveHandle.save(overwrite);
        if (outcome.kind === 'invalid' && outcome.issues) jumpToFirstIssue(outcome.issues);
    };

    if (apps.isLoading && !instance) {
        return <PaneLoading text="正在读取实例…" />;
    }

    if (!instance) {
        return (
            <PagePlaceholder className="gap-3 py-16">
                <p className="text-sm text-text-secondary">实例不存在或已被删除</p>
                <Button size="sm" variant="secondary" onClick={onBack}>
                    <ActionMotionIcon icon={ArrowLeft} size={13} />
                    返回列表
                </Button>
            </PagePlaceholder>
        );
    }

    const busy = apps.pendingId === instance.id || instance.state === 'installing';
    const showSaveBar = !!ui && ui.typedTabs.has(activeTab);
    const fillPane = activeTab === 'raw' || activeTab === 'log' || !!ui?.fillPaneTabs.has(activeTab);

    // 填错的页亮红点，盖过框架给的下一步 / 冲突点：挡着保存的事最急
    const issueTabs = ui && saveHandle ? saveHandle.issuePaths.map(ui.tabForIssue) : [];
    const badges: NavBadges = { ...navBadges, ...Object.fromEntries(issueTabs.map((t) => [t, 'error' as const])) };
    const firstIssueTab = issueTabs[0];

    return (
        <TooltipProvider delayDuration={200}>
            <div className="flex h-full w-full flex-col">
                <DetailHeader
                    instance={instance}
                    frameworkName={manifest?.display_name ?? instance.framework_id}
                    hostLabel={hostIdDisplayLabel(instance.host_id, servers)}
                    installed={installed}
                    busy={busy}
                    canWebUi={!!manifest?.has_webui && running}
                    onBack={onBack}
                    onInstall={() => apps.install(instance.id)}
                    onStart={() => apps.start(instance.id)}
                    onStop={() => apps.stop(instance.id)}
                    onTryChat={
                        running && instance.framework_id === 'astrbot'
                            ? () => void apps.openWebUi(instance.id, '/chat')
                            : undefined
                    }
                    onLink={() => setLinkOpen(true)}
                    onUnlink={() => apps.unlink(instance.id)}
                    onWebUi={() => void apps.openWebUi(instance.id)}
                    onRefresh={() => apps.refresh(instance.id)}
                    onDelete={() => setDeleteOpen(true)}
                />

                {!installed ? (
                    <NotInstalledBody
                        instance={instance}
                        busy={busy}
                        onInstall={() => apps.install(instance.id)}
                        onViewTasks={onViewTasks}
                    />
                ) : (
                    <Tabs value={activeTab} onValueChange={goTab} orientation="vertical" className="flex min-h-0 flex-1">
                        <DetailSideNav groups={nav} badges={badges} />
                        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
                            {/* 插件商店把搜索 / 筛选挂进来；别的页没往里放东西时整行不占位 */}
                            <div
                                id="app-store-toolbar-slot"
                                className="flex min-w-0 shrink-0 items-center justify-end gap-2 pl-5 pr-3 pt-3 empty:hidden"
                            />
                            <div
                                className={cn(
                                    'flex min-h-0 flex-1 flex-col pl-5 pr-3 pt-2',
                                    fillPane ? 'overflow-hidden' : 'overflow-y-auto',
                                )}
                            >
                                {FrameworkDetail && (
                                    <FrameworkDetail
                                        instance={instance}
                                        onSaveHandle={setSaveHandle}
                                        onGoTab={goTab}
                                        onOpenLink={() => setLinkOpen(true)}
                                        onNavBadges={setNavBadges}
                                    />
                                )}
                                <TabsContent value="raw" className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2">
                                    <RawFilesTab instance={instance} />
                                </TabsContent>
                                <TabsContent value="log" className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2">
                                    <InstanceLogTab instance={instance} />
                                </TabsContent>
                            </div>
                            <div id="karin-connections-add-dock" />
                            {showSaveBar && saveHandle && (
                                <SaveBar
                                    dirty={saveHandle.dirty}
                                    saving={saveHandle.saving}
                                    issueCount={saveHandle.issueCount}
                                    onSave={() => void handleSave()}
                                    onCancel={saveHandle.reset}
                                    onLocate={
                                        firstIssueTab && firstIssueTab !== activeTab
                                            ? () => goTab(firstIssueTab)
                                            : undefined
                                    }
                                />
                            )}
                        </div>
                    </Tabs>
                )}

                <AppLinkDialog
                    open={linkOpen}
                    onOpenChange={setLinkOpen}
                    instanceId={linkOpen ? instance.id : null}
                />

                <DeleteInstanceDialog
                    instance={deleteOpen ? instance : null}
                    isRemoving={apps.isRemoving}
                    onClose={() => setDeleteOpen(false)}
                    onConfirm={async (removeFiles) => {
                        await apps.remove({ id: instance.id, removeFiles });
                        setDeleteOpen(false);
                        onBack();
                    }}
                />

                <ConfigConflictDialog
                    open={!!saveHandle?.conflict}
                    busy={!!saveHandle?.saving}
                    onCancel={() => saveHandle?.dismissConflict()}
                    onReload={() => void saveHandle?.reloadDiscard()}
                    onOverwrite={() => void handleSave(true)}
                />
            </div>
        </TooltipProvider>
    );
};

const NotInstalledBody: React.FC<{
    instance: AppInstance;
    busy: boolean;
    onInstall: () => void;
    onViewTasks?: () => void;
}> = ({ instance, busy, onInstall, onViewTasks }) =>
    instance.state === 'installing' ? (
        <PagePlaceholder className="gap-3 py-16">
            <p className="text-sm text-text-secondary">正在安装，完成后即可配置</p>
            <DetailInstallProgress instance={instance} />
            {onViewTasks && (
                <Button size="sm" variant="secondary" onClick={onViewTasks}>
                    <ActionMotionIcon icon={ListChecks} size={13} />
                    在任务队列查看
                </Button>
            )}
        </PagePlaceholder>
    ) : (
        <PagePlaceholder className="gap-3 py-16">
            <p className="text-sm text-text-secondary">实例尚未安装，安装后才能配置与查看日志</p>
            <Button size="sm" variant="primary" disabled={busy} onClick={onInstall}>
                <ActionMotionIcon icon={Download} size={13} motion={EMPHASIS_MOTION} />
                立即安装
            </Button>
        </PagePlaceholder>
    );

export default AppInstancePageNext;
