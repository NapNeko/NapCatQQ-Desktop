// 应用端实例详情：头部 + Tabs + 粘性保存。Karin 走类型化 Tab；NoneBot2 走适配器/插件店 + 窄连接。

import { useEffect, useMemo, useState } from 'react';
import {
    AlertCircle,
    ArrowLeft,
    Check,
    Download,
    ExternalLink,
    Link2,
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
import { RawFilesTab } from './RawFilesTab';
import { useKarinConfigForm } from './useKarinConfigForm';
import { KarinBasicTab } from './karin/KarinBasicTab';
import { KarinConnectionsTab } from './karin/KarinConnectionsTab';
import { KarinPermissionsTab } from './karin/KarinPermissionsTab';
import { KarinRenderStorageTab } from './karin/KarinRenderStorageTab';
import { KarinPluginsTab } from './karin/KarinPluginsTab';
import { KarinRulesTab } from './karin/KarinRulesTab';
import { NoneBot2ConnectionsTab } from './nonebot2/NoneBot2ConnectionsTab';
import { NoneBot2StoreTab } from './nonebot2/NoneBot2StoreTab';
import { useNoneBot2ConfigForm } from './useNoneBot2ConfigForm';
import type { DetailTabHint } from '../list/AppInstanceListPage';
import type { AppConfigIssue, AppInstance } from '../../../core/ipc/types';

type KarinTab = 'basic' | 'permissions' | 'connections' | 'rules' | 'render' | 'plugins';
type NoneBotTab = 'adapters' | 'plugins' | 'connections';
type TabValue = KarinTab | NoneBotTab | 'raw' | 'log';

const NONEBOT_TABS: ReadonlyArray<{ value: NoneBotTab; label: string }> = [
    { value: 'adapters', label: '适配器' },
    { value: 'plugins', label: '插件' },
    { value: 'connections', label: '连接' },
];

const KARIN_TABS: ReadonlyArray<{ value: KarinTab; label: string }> = [
    { value: 'basic', label: '基础' },
    { value: 'permissions', label: '权限' },
    { value: 'connections', label: '连接' },
    { value: 'rules', label: '响应规则' },
    { value: 'render', label: '渲染与存储' },
    { value: 'plugins', label: '插件' },
];

const KARIN_TYPED_TABS = new Set<string>(['basic', 'permissions', 'connections', 'rules', 'render']);
const NONEBOT_TYPED_TABS = new Set<string>(['connections']);

/** 校验 issue 的 path 前缀 → 所在 Tab，保存被驳回时跳过去。 */
function tabForIssuePath(path: string, nonebot: boolean): TabValue {
    if (nonebot) return 'connections';
    const root = path.split('/')[0];
    switch (root) {
        case 'config':
            return 'permissions';
        case 'adapter':
            return 'connections';
        case 'groups':
        case 'privates':
            return 'rules';
        case 'render':
        case 'redis':
            return 'render';
        case 'env':
            if (
                path.startsWith('env/http_') ||
                path.startsWith('env/ws_server_auth_key')
            ) {
                return 'connections';
            }
            return 'basic';
        default:
            return 'basic';
    }
}

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

    const isKarin = instance?.framework_id === 'karin';
    const isNoneBot2 = instance?.framework_id === 'nonebot2';
    const installed = !!instance && isInstalled(instance);
    const running = instance?.state === 'running';
    const karinTyped = isKarin && installed;
    const nonebotTyped = isNoneBot2 && installed;

    const form = useKarinConfigForm(instanceId, karinTyped, instance?.display_name ?? '');
    const nbForm = useNoneBot2ConfigForm(instanceId, nonebotTyped, instance?.display_name ?? '');

    const defaultTab: TabValue =
        initialTab === 'log' ? 'log' : karinTyped ? 'basic' : nonebotTyped ? 'adapters' : 'raw';
    const [activeTab, setActiveTab] = useState<TabValue>(defaultTab);
    // 详情页刚打开时实例列表可能还没到；到了之后按框架修正默认 Tab（仅在用户没手动切过时）
    const [userSwitched, setUserSwitched] = useState(false);
    useEffect(() => {
        if (userSwitched || !instance) return;
        setActiveTab(
            initialTab === 'log' ? 'log' : karinTyped ? 'basic' : nonebotTyped ? 'adapters' : 'raw',
        );
    }, [instance, karinTyped, nonebotTyped, initialTab, userSwitched]);

    const [linkOpen, setLinkOpen] = useState(false);
    const [deleteOpen, setDeleteOpen] = useState(false);

    const jumpToFirstIssue = (issues: AppConfigIssue[]) => {
        const first = issues[0];
        if (!first) return;
        setUserSwitched(true);
        setActiveTab(tabForIssuePath(first.path, isNoneBot2));
    };

    const handleSave = async (overwrite = false) => {
        const outcome = isNoneBot2 ? await nbForm.save(overwrite) : await form.save(overwrite);
        if (outcome.kind === 'invalid') jumpToFirstIssue(outcome.issues);
    };

    if (apps.isLoading && !instance) {
        return (
            <PagePlaceholder className="gap-2 py-16">
                <Spinner size="sm" />
                <span className="text-sm text-text-tertiary">加载实例…</span>
            </PagePlaceholder>
        );
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
    const activeForm = isNoneBot2 ? nbForm : form;
    const showSaveBar =
        (karinTyped && KARIN_TYPED_TABS.has(activeTab))
        || (nonebotTyped && NONEBOT_TYPED_TABS.has(activeTab));
    const fillPane =
        activeTab === 'raw'
        || activeTab === 'log'
        || activeTab === 'plugins'
        || activeTab === 'adapters';

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
                            <Button size="sm" variant="primary" disabled={busy} onClick={() => apps.start(instance.id)}>
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
                        <InstanceMoreMenu
                            busy={busy}
                            canLink={installed}
                            linked={!!instance.link}
                            canWebUi={!!manifest?.has_webui && running}
                            onLink={() => setLinkOpen(true)}
                            onUnlink={() => apps.unlink(instance.id)}
                            onWebUi={() => void apps.openWebUi(instance.id)}
                            onRefresh={() => apps.refresh(instance.id)}
                        />
                        <Button
                            variant="ghost"
                            size="sm"
                            className="text-danger hover:text-danger"
                            disabled={busy}
                            onClick={() => setDeleteOpen(true)}
                        >
                            <ActionMotionIcon icon={Trash2} size={13} strokeWidth={2.2} />
                            删除实例
                        </Button>
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
                            onValueChange={(v) => {
                                setUserSwitched(true);
                                setActiveTab(v as TabValue);
                            }}
                            className={cn('flex flex-1 flex-col', fillPane && 'min-h-0')}
                        >
                            <div className="sticky top-0 z-[5] flex items-center justify-between gap-2 border-b border-border-subtle bg-canvas/95 backdrop-blur-sm">
                                <TabsList className="scrollbar-hide min-w-0 shrink overflow-x-auto border-b-0">
                                    {karinTyped &&
                                        KARIN_TABS.map((t) => (
                                            <TabsTrigger key={t.value} value={t.value}>
                                                {t.label}
                                            </TabsTrigger>
                                        ))}
                                    {nonebotTyped &&
                                        NONEBOT_TABS.map((t) => (
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
                                    {showSaveBar && (
                                        <SaveActions
                                            dirty={activeForm.dirty}
                                            saving={activeForm.saving}
                                            issueCount={activeForm.clientIssues.length}
                                            onSave={() => void handleSave()}
                                            onCancel={activeForm.reset}
                                        />
                                    )}
                                </div>
                            </div>

                            {karinTyped && <TypedTabs instance={instance} form={form} />}
                            {karinTyped && (
                                <TabsContent
                                    value="plugins"
                                    className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2"
                                >
                                    <KarinPluginsTab instance={instance} />
                                </TabsContent>
                            )}
                            {nonebotTyped && (
                                <>
                                    <TabsContent
                                        value="adapters"
                                        className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2"
                                    >
                                        <NoneBot2StoreTab instance={instance} resource="adapter" />
                                    </TabsContent>
                                    <TabsContent
                                        value="plugins"
                                        className="flex min-h-0 flex-1 flex-col overflow-hidden pt-2"
                                    >
                                        <NoneBot2StoreTab instance={instance} resource="plugin" />
                                    </TabsContent>
                                    <NoneBot2TypedTab instance={instance} form={nbForm} />
                                </>
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
                    open={activeForm.conflict}
                    busy={activeForm.saving}
                    onCancel={activeForm.dismissConflict}
                    onReload={() => void activeForm.reloadDiscard()}
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

const NoneBot2TypedTab: React.FC<{
    instance: AppInstance;
    form: ReturnType<typeof useNoneBot2ConfigForm>;
}> = ({ instance, form }) => {
    if (form.isLoading && !form.form) {
        return (
            <TabsContent value="connections" className="pb-8 pt-2">
                <div className="flex items-center gap-2 py-10 text-sm text-text-tertiary">
                    <Spinner size="sm" /> 读取配置…
                </div>
            </TabsContent>
        );
    }
    if (form.loadError && !form.form) {
        return (
            <TabsContent value="connections" className="pb-8 pt-2">
                <div className="flex flex-col items-start gap-2 py-10">
                    <p className="text-sm text-text-secondary">读取配置失败</p>
                    <Button size="sm" variant="secondary" onClick={() => void form.reloadDiscard()}>
                        <ActionMotionIcon icon={RefreshCw} size={13} />
                        重试
                    </Button>
                </div>
            </TabsContent>
        );
    }
    if (!form.form) return null;
    return (
        <TabsContent value="connections" className="pb-8 pt-2">
            <NoneBot2ConnectionsTab
                config={form.form}
                onChange={form.setForm}
                errors={form.errors}
                linked={!!instance.link}
                disabled={form.saving}
            />
        </TabsContent>
    );
};

const TypedTabs: React.FC<{
    instance: AppInstance;
    form: ReturnType<typeof useKarinConfigForm>;
}> = ({ instance, form }) => {
    if (form.isLoading && !form.form) {
        return (
            <>
                {(['basic', 'permissions', 'connections', 'rules', 'render'] as const).map((value) => (
                    <TabsContent key={value} value={value} className="pb-8 pt-2">
                        <div className="flex items-center gap-2 py-10 text-sm text-text-tertiary">
                            <Spinner size="sm" /> 读取配置…
                        </div>
                    </TabsContent>
                ))}
            </>
        );
    }
    if (form.loadError && !form.form) {
        return (
            <>
                {(['basic', 'permissions', 'connections', 'rules', 'render'] as const).map((value) => (
                    <TabsContent key={value} value={value} className="pb-8 pt-2">
                        <div className="flex flex-col items-start gap-2 py-10">
                            <p className="text-sm text-text-secondary">读取配置失败</p>
                            <Button size="sm" variant="secondary" onClick={() => void form.reloadDiscard()}>
                                <ActionMotionIcon icon={RefreshCw} size={13} />
                                重试
                            </Button>
                        </div>
                    </TabsContent>
                ))}
            </>
        );
    }
    if (!form.form) return null;

    const tabProps = {
        config: form.form,
        onChange: form.setForm,
        errors: form.errors,
        linked: !!instance.link,
        disabled: form.saving,
    };

    return (
        <>
            <TabsContent value="basic" className="pb-8 pt-2">
                <KarinBasicTab {...tabProps} />
            </TabsContent>
            <TabsContent value="permissions" className="pb-8 pt-2">
                <KarinPermissionsTab {...tabProps} />
            </TabsContent>
            <TabsContent value="connections" className="pb-8 pt-2">
                <KarinConnectionsTab {...tabProps} />
            </TabsContent>
            <TabsContent value="rules" className="pb-8 pt-2">
                <KarinRulesTab {...tabProps} />
            </TabsContent>
            <TabsContent value="render" className="pb-8 pt-2">
                <KarinRenderStorageTab {...tabProps} />
            </TabsContent>
        </>
    );
};

const InstanceMoreMenu: React.FC<{
    busy: boolean;
    canLink: boolean;
    linked: boolean;
    canWebUi: boolean;
    onLink: () => void;
    onUnlink: () => void;
    onWebUi: () => void;
    onRefresh: () => void;
}> = ({ busy, canLink, linked, canWebUi, onLink, onUnlink, onWebUi, onRefresh }) => (
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
        </PopoverContent>
    </Popover>
);

const MoreItem: React.FC<{
    icon: typeof Link2;
    label: string;
    onClick: () => void;
}> = ({ icon: Icon, label, onClick }) => (
    <PopoverClose asChild>
        <button
            type="button"
            className="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-[13px] text-text hover:bg-inset"
            onClick={onClick}
        >
            <Icon size={14} className="shrink-0 text-text-secondary" />
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
