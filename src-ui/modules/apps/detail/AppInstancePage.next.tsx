// 应用实例管理与配置文件编辑。

import { useEffect, useMemo, useState } from 'react';
import { ArrowLeft, Download, ListChecks } from 'lucide-react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    PagePlaceholder,
    Tabs,
    TabsContent,
    TooltipProvider,
} from '../../../shared/ui';
import { ActionMotionIcon, EMPHASIS_MOTION } from '../../../shared/ui/motion';
import { useServerManager } from '../../../hooks/remote/useServerManager';
import { useAppFrameworks, useAppInstances } from '../../../hooks/apps/useAppInstances';
import { useAppFrameworkVersions } from '../../../hooks/apps/useAppFrameworkVersions';
import { hasAppUpdate } from '../../../core/domain/apps/appVersions';
import { ReinstallDialog } from './ReinstallDialog';
import { AppLinkDialog } from '../AppLinkDialog';
import { DetailInstallProgress } from '../InstallProgress';
import { DeleteInstanceDialog } from '../DeleteInstanceDialog';
import { hostIdDisplayLabel, isAppHostRemoved } from '../hostLabel';
import { isInstalled } from '../../../core/domain/apps/instanceState';
import { cn } from '../../../shared/utils/cn';
import { DetailHeader } from './DetailHeader';
import { DetailSideNav } from './DetailSideNav';
import { InstanceLogTab } from './InstanceLogTab';
import { InstanceVersionTab } from './InstanceVersionTab';
import { PaneLoading } from './PaneStatus';
import { InstanceConfigEditor } from './InstanceConfigEditor';
import { WebUiAccountCard } from './WebUiAccountCard';
import type { DetailTabHint } from '../list/AppInstanceListPage';
import type { AppInstance } from '../../../core/ipc/types';

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
    const { servers, isSuccess: serversLoaded } = useServerManager();

    const instance = apps.instances.find((i) => i.id === instanceId) ?? null;
    const manifest = useMemo(
        () => (frameworks.data ?? []).find((m) => m.id === instance?.framework_id),
        [frameworks.data, instance?.framework_id],
    );

    // 上游最新正式版：只给「支持按版本安装」的框架查；null 时头部不显示更新提示
    const versions = useAppFrameworkVersions(instance?.framework_id);
    const latestVersion = versions.data?.latest ?? null;

    const installed = !!instance && isInstalled(instance);
    const running = instance?.state === 'running';
    const [activeTab, setActiveTab] = useState<string>(initialTab ?? 'config');

    useEffect(() => {
        setActiveTab(initialTab ?? 'config');
    }, [instanceId, initialTab]);

    const [linkOpen, setLinkOpen] = useState(false);
    const [deleteOpen, setDeleteOpen] = useState(false);
    const [accountOpen, setAccountOpen] = useState(false);
    /** 重装并指定版本；未安装时是「立即安装」的带版本入口 */
    const [reinstallOpen, setReinstallOpen] = useState(false);
    /** 用户为这次安装挑的版本；null = 最新正式版 */
    const [installVersion, setInstallVersion] = useState<string | null>(null);
    // 已装的版本比上游最新正式版旧：头部与「更多」菜单都要提示
    const hasUpdate = hasAppUpdate(instance?.installed_version, latestVersion);

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
    const fillPane = activeTab === 'log' || activeTab === 'config';

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
                    onInstall={() => apps.install(instance.id, installVersion)}
                    onStart={() => apps.start(instance.id)}
                    onStop={() => apps.stop(instance.id)}
                    onLink={() => setLinkOpen(true)}
                    onUnlink={() => apps.unlink(instance.id)}
                    onWebUi={() => void apps.openWebUi(instance.id)}
                    onWebUiAccount={
                        installed && manifest?.webui_auth === 'user_password'
                            ? () => setAccountOpen(true)
                            : undefined
                    }
                    onOpenDirectory={
                        installed &&
                        instance.host_id === 'local' &&
                        instance.placement === 'local_native'
                            ? () => void apps.openDirectory(instance.id)
                            : undefined
                    }
                    onRefresh={() => apps.refresh(instance.id)}
                    onDelete={() => setDeleteOpen(true)}
                    onAutoStartChange={(autoStart) =>
                        apps.setAutoStart({ id: instance.id, autoStart })
                    }
                    latestVersion={latestVersion}
                    hasUpdate={hasUpdate}
                    onReinstall={installed ? () => setReinstallOpen(true) : undefined}
                />

                {!installed ? (
                    <NotInstalledBody
                        instance={instance}
                        busy={busy}
                        onInstall={() => apps.install(instance.id, installVersion)}
                        onPickVersion={
                            versions.data === null ? undefined : () => setReinstallOpen(true)
                        }
                        installVersion={installVersion}
                        latestVersion={latestVersion}
                        onViewTasks={onViewTasks}
                    />
                ) : (
                    <Tabs
                        value={activeTab}
                        onValueChange={setActiveTab}
                        orientation="vertical"
                        className="flex min-h-0 flex-1"
                    >
                        <DetailSideNav withVersionTab={!!versions.data} />
                        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
                            <div
                                className={cn(
                                    'flex min-h-0 flex-1 flex-col pl-5 pr-3 pt-2',
                                    fillPane ? 'overflow-hidden' : 'overflow-y-auto',
                                )}
                            >
                                <TabsContent
                                    value="config"
                                    className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2"
                                >
                                    <InstanceConfigEditor key={instance.id} instance={instance} />
                                </TabsContent>
                                <TabsContent
                                    value="version"
                                    className="flex min-h-0 flex-1 flex-col pt-2"
                                >
                                    <InstanceVersionTab
                                        instance={instance}
                                        latestVersion={latestVersion}
                                        versions={versions.data?.versions ?? null}
                                        loading={versions.isLoading}
                                        busy={busy}
                                        onSwitch={(v) => {
                                            setInstallVersion(v);
                                            setReinstallOpen(true);
                                        }}
                                    />
                                </TabsContent>
                                <TabsContent
                                    value="log"
                                    className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2"
                                >
                                    <InstanceLogTab instance={instance} />
                                </TabsContent>
                            </div>
                        </div>
                    </Tabs>
                )}

                <AppLinkDialog
                    open={linkOpen}
                    onOpenChange={setLinkOpen}
                    instanceId={linkOpen ? instance.id : null}
                />

                <Dialog open={accountOpen} onOpenChange={setAccountOpen}>
                    <DialogContent size="sm" aria-describedby={undefined}>
                        <DialogHeader>
                            <DialogTitle>WebUI 账号</DialogTitle>
                        </DialogHeader>
                        {accountOpen && <WebUiAccountCard instance={instance} />}
                    </DialogContent>
                </Dialog>

                <ReinstallDialog
                    open={reinstallOpen}
                    instance={instance}
                    version={installVersion}
                    onVersionChange={setInstallVersion}
                    latestVersion={latestVersion}
                    busy={busy}
                    onClose={() => setReinstallOpen(false)}
                    onConfirm={async () => {
                        setReinstallOpen(false);
                        apps.install(instance.id, installVersion);
                    }}
                />

                <DeleteInstanceDialog
                    instance={deleteOpen ? instance : null}
                    hostRemoved={isAppHostRemoved(instance.host_id, servers, serversLoaded)}
                    isRemoving={apps.isRemoving}
                    onClose={() => setDeleteOpen(false)}
                    onConfirm={async (removeFiles) => {
                        await apps.remove({ id: instance.id, removeFiles });
                        setDeleteOpen(false);
                        onBack();
                    }}
                />
            </div>
        </TooltipProvider>
    );
};

const NotInstalledBody: React.FC<{
    instance: AppInstance;
    busy: boolean;
    onInstall: () => void;
    /** 打开「选择版本」对话框；框架不支持按版本安装时不传，按钮不出现 */
    onPickVersion?: () => void;
    installVersion: string | null;
    latestVersion: string | null;
    onViewTasks?: () => void;
}> = ({ instance, busy, onInstall, onPickVersion, installVersion, latestVersion, onViewTasks }) =>
    instance.state === 'installing' ? (
        <PagePlaceholder className="gap-3 py-16">
            <p className="text-sm text-text-secondary">正在安装，完成后即可启动和对接</p>
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
            <p className="text-sm text-text-secondary">实例尚未安装</p>
            <p className="text-xs text-text-tertiary">
                {installVersion
                    ? `将安装 ${installVersion}`
                    : latestVersion
                      ? `将安装最新正式版 ${latestVersion}`
                      : '将安装最新正式版'}
            </p>
            <div className="flex items-center gap-2">
                <Button size="sm" variant="primary" disabled={busy} onClick={onInstall}>
                    <ActionMotionIcon icon={Download} size={13} motion={EMPHASIS_MOTION} />
                    立即安装
                </Button>
                {onPickVersion && (
                    <Button size="sm" variant="secondary" disabled={busy} onClick={onPickVersion}>
                        选择版本
                    </Button>
                )}
            </div>
        </PagePlaceholder>
    );

export default AppInstancePageNext;
