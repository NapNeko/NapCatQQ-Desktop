// 应用端实例详情：头部 + Tabs + 粘性保存。框架专有 Tab 走 registry，未知框架只有原文和日志。

import { useEffect, useMemo, useState } from 'react';
import {
    AlertCircle,
    ArrowLeft,
    Check,
    Download,
    ExternalLink,
    Link2,
    MessageSquare,
    MoreHorizontal,
    Play,
    RefreshCw,
    Save,
    Square,
    Trash2,
    Unlink,
} from 'lucide-react';
import {
    Badge,
    Button,
    PagePlaceholder,
    Popover,
    PopoverClose,
    PopoverContent,
    PopoverTrigger,
    Spinner,
    Tabs,
    TabsContent,
    TabsList,
    TabsTrigger,
    TooltipProvider,
} from '../../../shared/ui';
import { ActionMotionIcon, EMPHASIS_MOTION, infoToneMotion } from '../../../shared/ui/motion';
import { useServerManager } from '../../../hooks/remote/useServerManager';
import { useAppFrameworks, useAppInstances } from '../../../hooks/apps/useAppInstances';
import { AppLinkDialog } from '../AppLinkDialog';
import { DeleteInstanceDialog } from '../DeleteInstanceDialog';
import { hostIdDisplayLabel } from '../hostLabel';
import { isInstalled } from '../instanceState';
import { cn } from '../../../shared/utils/cn';
import { ConfigConflictDialog } from './ConfigConflictDialog';
import { InstanceLogTab } from './InstanceLogTab';
import { PaneLoading } from './PaneStatus';
import { RawFilesTab } from './RawFilesTab';
import { resolveFrameworkUi, type FrameworkSaveHandle } from './frameworkUi';
import type { DetailTabHint } from '../list/AppInstanceListPage';
import type { AppConfigIssue, AppInstance } from '../../../core/ipc/types';

export interface AppInstancePageNextProps {
    instanceId: string;
    initialTab?: DetailTabHint;
    onBack: () => void;
}

export const AppInstancePageNext: React.FC<AppInstancePageNextProps> = ({ instanceId, initialTab, onBack }) => {
    const apps = useAppInstances();
    const frameworks = useAppFrameworks();
    const { servers } = useServerManager();

    const instance = apps.instances.find((i) => i.id === instanceId) ?? null;
    const manifest = useMemo(
        () => (frameworks.data ?? []).find((m) => m.id === instance?.framework_id),
        [frameworks.data, instance?.framework_id],
    );

    const ui = instance ? resolveFrameworkUi(instance.framework_id) : undefined;
    const installed = !!instance && isInstalled(instance);
    const running = instance?.state === 'running';
    const FrameworkDetail = ui?.Detail;

    const defaultTab =
        initialTab === 'log' ? 'log' : installed && ui ? ui.defaultTab : 'raw';
    const [activeTab, setActiveTab] = useState(defaultTab);
    const [userSwitched, setUserSwitched] = useState(false);
    const [saveHandle, setSaveHandle] = useState<FrameworkSaveHandle | null>(null);

    useEffect(() => {
        if (userSwitched || !instance) return;
        setActiveTab(initialTab === 'log' ? 'log' : installed && ui ? ui.defaultTab : 'raw');
    }, [instance, installed, ui, initialTab, userSwitched]);

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
    const fillPane =
        activeTab === 'raw'
        || activeTab === 'log'
        || !!ui?.fillPaneTabs.has(activeTab);

    return (
        <TooltipProvider delayDuration={200}>
            <div className="flex h-full w-full flex-col">
                <header className="flex items-start justify-between gap-3 border-b border-border-subtle py-3">
                    <div className="flex min-w-0 items-start gap-3">
                        <Button variant="ghost" size="icon" onClick={onBack} aria-label="返回列表">
                            <ActionMotionIcon icon={ArrowLeft} size={16} />
                        </Button>
                        <div className="flex min-w-0 flex-col gap-0.5">
                            <div className="flex min-w-0 items-center gap-2">
                                <h1 className="truncate font-display text-md font-semibold text-text">
                                    {instance.display_name}
                                </h1>
                                {instance.link && (
                                    <Badge tone="brand" appearance="soft" className="shrink-0">
                                        对接 {instance.link.bot_id}
                                    </Badge>
                                )}
                            </div>
                            <p className="truncate text-xs text-text-tertiary" title={instance.install_dir}>
                                {[
                                    manifest?.display_name ?? instance.framework_id,
                                    `${hostIdDisplayLabel(instance.host_id, servers)} :${instance.port}`,
                                ]
                                    .filter(Boolean)
                                    .join(' · ')}
                            </p>
                        </div>
                    </div>

                    <div className="flex shrink-0 items-center gap-1.5">
                        {busy && <Spinner size="sm" />}
                        {!installed && (
                            <Button size="sm" variant="primary" disabled={busy} onClick={() => apps.install(instance.id)}>
                                <ActionMotionIcon icon={Download} size={13} motion={EMPHASIS_MOTION} />
                                安装
                            </Button>
                        )}
                        {installed && !running && (
                            <Button size="sm" variant="ghost" disabled={busy} onClick={() => apps.start(instance.id)}>
                                <ActionMotionIcon icon={Play} size={13} motion={EMPHASIS_MOTION} />
                                启动
                            </Button>
                        )}
                        {running && (
                            <Button size="sm" variant="secondary" disabled={busy} onClick={() => apps.stop(instance.id)}>
                                <ActionMotionIcon icon={Square} size={13} />
                                停止
                            </Button>
                        )}
                        {running && instance.framework_id === 'astrbot' && (
                            <Button
                                size="sm"
                                variant="ghost"
                                disabled={busy}
                                onClick={() => void apps.openWebUi(instance.id, '/chat')}
                            >
                                <ActionMotionIcon icon={MessageSquare} size={13} />
                                试聊
                            </Button>
                        )}
                        <InstanceMoreMenu
                            busy={busy}
                            canLink={installed}
                            linked={!!instance.link}
                            canWebUi={!!manifest?.has_webui && running}
                            deleteLabel={instance.origin === 'imported' ? '释放接管' : '删除实例'}
                            onLink={() => setLinkOpen(true)}
                            onUnlink={() => apps.unlink(instance.id)}
                            onWebUi={() => void apps.openWebUi(instance.id)}
                            onRefresh={() => apps.refresh(instance.id)}
                            onDelete={() => setDeleteOpen(true)}
                        />
                    </div>
                </header>

                {!installed ? (
                    <NotInstalledBody instance={instance} busy={busy} onInstall={() => apps.install(instance.id)} />
                ) : (
                    <div className="flex min-h-0 flex-1 flex-col">
                    <div
                        className={cn(
                            'flex min-h-0 flex-1 flex-col px-2',
                            fillPane ? 'overflow-hidden' : 'overflow-y-auto',
                        )}
                    >
                        <Tabs
                            value={activeTab}
                            onValueChange={goTab}
                            className={cn('flex flex-1 flex-col', fillPane && 'min-h-0')}
                        >
                            <div className="sticky top-0 z-[5] flex items-center justify-between gap-2 border-b border-border-subtle bg-canvas/95 backdrop-blur-sm">
                                <TabsList className="scrollbar-hide min-w-0 shrink overflow-x-auto border-b-0">
                                    {ui?.extraTabs.map((t) => (
                                        <TabsTrigger key={t.value} value={t.value}>
                                            {t.label}
                                        </TabsTrigger>
                                    ))}
                                    <TabsTrigger value="raw">原始文件</TabsTrigger>
                                    <TabsTrigger value="log">日志</TabsTrigger>
                                </TabsList>
                                <div className="ml-auto flex min-w-0 items-center justify-end gap-2">
                                    <div
                                        id="app-store-toolbar-slot"
                                        className="flex min-w-0 items-center justify-end"
                                    />
                                    {showSaveBar && saveHandle && (
                                        <SaveActions
                                            dirty={saveHandle.dirty}
                                            saving={saveHandle.saving}
                                            issueCount={saveHandle.issueCount}
                                            onSave={() => void handleSave()}
                                            onCancel={saveHandle.reset}
                                        />
                                    )}
                                </div>
                            </div>

                            {FrameworkDetail && (
                                <FrameworkDetail instance={instance} onSaveHandle={setSaveHandle} onGoTab={goTab} />
                            )}

                            <TabsContent value="raw" className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2">
                                <RawFilesTab instance={instance} />
                            </TabsContent>
                            <TabsContent value="log" className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2">
                                <InstanceLogTab instance={instance} />
                            </TabsContent>
                        </Tabs>
                    </div>
                    <div id="karin-connections-add-dock" />
                    </div>
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

const NotInstalledBody: React.FC<{ instance: AppInstance; busy: boolean; onInstall: () => void }> = ({
    instance,
    busy,
    onInstall,
}) => (
    <PagePlaceholder className="gap-3 py-16">
        <p className="text-sm text-text-secondary">
            {instance.state === 'installing' ? '正在安装，完成后即可配置' : '实例尚未安装，安装后才能配置与查看日志'}
        </p>
        {instance.state !== 'installing' && (
            <Button size="sm" variant="primary" disabled={busy} onClick={onInstall}>
                <ActionMotionIcon icon={Download} size={13} motion={EMPHASIS_MOTION} />
                立即安装
            </Button>
        )}
    </PagePlaceholder>
);

const InstanceMoreMenu: React.FC<{
    busy: boolean;
    canLink: boolean;
    linked: boolean;
    canWebUi: boolean;
    deleteLabel: string;
    onLink: () => void;
    onUnlink: () => void;
    onWebUi: () => void;
    onRefresh: () => void;
    onDelete: () => void;
}> = ({
    busy,
    canLink,
    linked,
    canWebUi,
    deleteLabel,
    onLink,
    onUnlink,
    onWebUi,
    onRefresh,
    onDelete,
}) => (
    <Popover>
        <PopoverTrigger asChild>
            <Button variant="ghost" size="icon" className="h-8 w-8" disabled={busy} aria-label="更多">
                <ActionMotionIcon icon={MoreHorizontal} size={16} />
            </Button>
        </PopoverTrigger>
        <PopoverContent align="end" sideOffset={6} className="w-44 p-1">
            {canLink && (
                <MoreItem icon={Link2} label={linked ? '改绑' : '对接'} onClick={onLink} />
            )}
            {linked && <MoreItem icon={Unlink} label="解除对接" onClick={onUnlink} />}
            {canWebUi && <MoreItem icon={ExternalLink} label="打开 WebUI" onClick={onWebUi} />}
            <MoreItem icon={RefreshCw} label="重新探测" onClick={onRefresh} />
            <div className="my-1 h-px bg-border-subtle" />
            <MoreItem icon={Trash2} label={deleteLabel} tone="danger" onClick={onDelete} />
        </PopoverContent>
    </Popover>
);

const MoreItem: React.FC<{
    icon: typeof Link2;
    label: string;
    tone?: 'neutral' | 'danger';
    onClick: () => void;
}> = ({ icon: Icon, label, tone = 'neutral', onClick }) => (
    <PopoverClose asChild>
        <button
            type="button"
            className={cn(
                'flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-[13px]',
                tone === 'danger'
                    ? 'text-danger hover:bg-danger-soft'
                    : 'text-text hover:bg-inset',
            )}
            onClick={onClick}
        >
            <Icon
                size={14}
                className={cn('shrink-0', tone === 'danger' ? 'text-danger' : 'text-text-secondary')}
            />
            {label}
        </button>
    </PopoverClose>
);

const SaveActions: React.FC<{
    dirty: boolean;
    saving: boolean;
    issueCount: number;
    onSave: () => void;
    onCancel: () => void;
}> = ({ dirty, saving, issueCount, onSave, onCancel }) => (
    <div className="flex shrink-0 items-center gap-3 pr-1">
        <span className="hidden text-xs sm:inline-flex sm:items-center sm:gap-1.5">
            {issueCount > 0 ? (
                <>
                    <ActionMotionIcon
                        icon={AlertCircle}
                        size={12}
                        strokeWidth={2.4}
                        motion={infoToneMotion('danger')}
                        className="text-danger"
                    />
                    <span className="text-danger">{issueCount} 处有误</span>
                </>
            ) : dirty ? (
                <>
                    <ActionMotionIcon
                        icon={AlertCircle}
                        size={12}
                        strokeWidth={2.4}
                        motion={infoToneMotion('info')}
                        className="text-info"
                    />
                    <span className="text-info">未保存</span>
                </>
            ) : (
                <>
                    <ActionMotionIcon icon={Check} size={12} strokeWidth={2.4} className="text-text-tertiary" />
                    <span className="text-text-tertiary">已是最新</span>
                </>
            )}
        </span>
        <div className="flex items-center gap-1.5">
            <Button variant="ghost" size="sm" onClick={onCancel} disabled={!dirty || saving}>
                撤销
            </Button>
            <Button variant="primary" size="sm" onClick={onSave} disabled={!dirty || saving || issueCount > 0}>
                {saving ? (
                    <>
                        <Spinner size="xs" />
                        <span>保存中</span>
                    </>
                ) : (
                    <>
                        <ActionMotionIcon icon={Save} size={13} strokeWidth={2.2} />
                        <span>保存</span>
                    </>
                )}
            </Button>
        </div>
    </div>
);

export default AppInstancePageNext;
