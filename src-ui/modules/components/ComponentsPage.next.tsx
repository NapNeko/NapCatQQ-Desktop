// 组件页：先选主机，再看这台机器能装啥。
// 编排分散在同目录的模块局部 hook：useComponentHosts（主机选择）、
// useQqDependencyProbes（QQ 依赖探测）、useComponentActions（组件操作）、
// useDockerSudoOps（Docker 安装 + sudo 弹框）、useAppInstanceDialogs（应用端实例对话框）。

import React, { useCallback, useMemo, useState, useSyncExternalStore } from 'react';
import { Box, Loader2, RefreshCw } from 'lucide-react';
import { useQueryClient } from '@tanstack/react-query';
import { Button } from '../../shared/ui';
import { MotionIcon, refreshMotion } from '../../shared/ui/motion';
import { useComponents } from '../../hooks/components/useComponents';
import { useComponentAction } from '../../hooks/components/useComponentAction';
import { useComponentActionErrors } from '../../hooks/components/useComponentActionErrors';
import { useComponentPageAlerts } from '../../hooks/components/useComponentPageAlerts';
import { useBotSnapshots } from '../../hooks/bot/useBotSnapshots';
import { useBotConfigsMap } from '../../hooks/bot/useBotConfigsMap';
import { useQqDependencyOps } from '../../hooks/components/useQqDependencyOps';
import { useReleases } from '../../hooks/diagnostics/useReleases';
import { useDockerHosts } from '../../hooks/docker/useDockerHosts';
import { useDockerInstallProgress } from '../../hooks/docker/useDockerInstallProgress';
import { useAppFrameworks, useAppInstances } from '../../hooks/apps/useAppInstances';
import { useFeatureEnabled } from '../../hooks/preferences/featureTogglesStore';
import { useServerManager } from '../../hooks/remote/useServerManager';
import { HostSwitcher } from './HostSwitcher';
import { HostComponentsView } from './HostComponentsView';
import { ReleaseNotesDialog } from './ReleaseNotesDialog';
import { SnowLumaPackageDialog } from './SnowLumaPackageDialog';
import { SudoPasswordDialog } from '../docker/SudoPasswordDialog';
import { CreateInstanceDialog, ImportInstanceDialog } from '../apps';
import type { ComponentRow } from '../../core/domain/components/types';
import {
    getComponentsHostBridge,
    subscribeComponentsHostBridge,
} from '../../hooks/desktop/componentsHostBridge';
import type { ComponentId } from '../../core/ipc/types';
import {
    releaseNotesLabel,
    resolveLatestRelease,
    resolveLatestVersion,
} from '../../core/domain/components/releaseLookup';
import { cn } from '../../shared/utils/cn';
import { PagePlaceholder } from '../../shared/ui/PagePlaceholder';
import scrollStyles from './componentsPageScroll.module.css';
import { useComponentHosts } from './useComponentHosts';
import { useQqDependencyProbes } from './useQqDependencyProbes';
import { useComponentActions } from './useComponentActions';
import { useDockerSudoOps } from './useDockerSudoOps';
import { useAppInstanceDialogs } from './useAppInstanceDialogs';

export const ComponentsPageNext: React.FC = () => {
    const queryClient = useQueryClient();
    const { view, hosts, isLoading, error, refetch } = useComponents();
    const action = useComponentAction();
    const { detectQqDependencies, rememberSudoPassword } = useQqDependencyOps();
    const {
        snapshot: releases,
        refetch: refetchReleases,
        isFetching: releasesFetching,
    } = useReleases();
    const { data: botSnapshots = [] } = useBotSnapshots();
    const botConfigs = useBotConfigsMap(botSnapshots);

    const hostIds = useMemo(() => hosts.map((h) => h.host_id), [hosts]);
    const dockerHosts = useDockerHosts(hostIds);

    // 应用端：框架清单 + 实例（按实例安装；应用端页也能导入已有项目）
    const appFrameworks = useAppFrameworks();
    const apps = useAppInstances();
    // 设置里关了应用端就不列应用端组
    const appsEnabled = useFeatureEnabled('apps');
    const { servers } = useServerManager();

    // 组件主导矩阵 → 主机主导，再剔掉这台机器一个组件都装不了的空机器。
    const allRows = useMemo<ComponentRow[]>(
        () => [...view.framework, ...view.runtimeDep, ...view.selfApp],
        [view],
    );
    const hostBridge = useSyncExternalStore(
        subscribeComponentsHostBridge,
        getComponentsHostBridge,
        getComponentsHostBridge,
    );
    const { machines, activeHostId, setActiveHostId, activeMachine } = useComponentHosts(
        allRows,
        hosts,
        hostBridge,
    );

    const { probeQqDependencies, reportByHost } = useQqDependencyProbes(
        machines,
        activeMachine,
        detectQqDependencies,
    );
    const activeQqDependencyReport = activeMachine
        ? (reportByHost[activeMachine.host.host_id]?.report ?? null)
        : null;

    // 清单 / 探测 / 组件操作终态 → 全局 InfoBar（顶层 InfoBarStack 渲染）。
    useComponentPageAlerts(allRows, error, activeHostId);
    useComponentActionErrors(allRows);

    const dockerInstallProgress = useDockerInstallProgress(activeMachine?.host.host_id ?? '');

    const hostNameOf = useCallback(
        (hostId: string) =>
            machines.find((m) => m.host.host_id === hostId)?.host.display_name ?? hostId,
        [machines],
    );

    // 版本号：组件更新按钮用（含 QQ 宿主探测）。
    const latestVersionFor = useCallback(
        (id: ComponentId) => resolveLatestVersion(releases, activeMachine?.host.os, id),
        [releases, activeMachine?.host.os],
    );

    // 更新日志：只给有 GitHub release body 的组件（NC / SL / NCD / ncd-watch）。
    // QQ 走 pcConfig 版本探测，没有可用 changelog，不展示「日志」。
    const latestReleaseFor = useCallback(
        (id: ComponentId) => resolveLatestRelease(releases, id),
        [releases],
    );

    const [releaseNotesTarget, setReleaseNotesTarget] = useState<ComponentId | null>(null);

    const handleShowReleaseNotes = useCallback((componentId: ComponentId) => {
        setReleaseNotesTarget(componentId);
    }, []);

    const dialogs = useAppInstanceDialogs({ hostNameOf, apps });

    const {
        slPkgPrompt,
        closeSlPkgPrompt,
        confirmSnowLumaPackage,
        handleAction,
        lifecycleBlockedReasonForHost,
    } = useComponentActions({
        hostNameOf,
        refetch,
        queryClient,
        action,
        botSnapshots,
        botConfigs,
        probeQqDependencies,
    });

    const {
        sudoPrompt,
        closeSudoPrompt,
        handleInstallDocker,
        handleDockerDeployError,
        startQqDepsRepair,
        handleSudoConfirm,
    } = useDockerSudoOps({
        hostNameOf,
        installDocker: dockerHosts.install,
        rememberSudoPassword,
        startAction: action.startAction,
        onTaskTerminal: action.onTaskTerminal,
        refetch,
        probeQqDependencies,
    });

    const refetchApps = apps.refetch;
    const handleRefresh = useCallback(() => {
        refetch();
        void refetchApps();
        // 远端版本必须 force，否则 1h 磁盘缓存会挡住中转/GitHub 重拉
        refetchReleases();
        if (activeMachine) {
            void probeQqDependencies(activeMachine.host.host_id, true);
        }
    }, [refetch, refetchApps, refetchReleases, activeMachine, probeQqDependencies]);

    const handleRetryDetect = useCallback(
        (hostId: string) => {
            refetch();
            refetchReleases();
            void probeQqDependencies(hostId, true);
        },
        [refetch, refetchReleases, probeQqDependencies],
    );

    const allEmpty = machines.length === 0;

    return (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
            <header className="flex shrink-0 items-end justify-between pb-4 pt-2">
                <div>
                    <p className="text-2xs uppercase tracking-widest text-text-tertiary">
                        components
                    </p>
                    <h1 className="font-display text-xl font-semibold text-text">组件管理</h1>
                </div>
                <Button
                    size="sm"
                    variant="secondary"
                    onClick={handleRefresh}
                    disabled={isLoading || releasesFetching}
                >
                    <MotionIcon
                        icon={RefreshCw}
                        motion={refreshMotion(isLoading || releasesFetching)}
                        playEnter={false}
                        size={14}
                    />
                    刷新
                </Button>
            </header>

            {machines.length > 1 && activeHostId ? (
                <HostSwitcher
                    machines={machines}
                    activeHostId={activeHostId}
                    onSelect={setActiveHostId}
                />
            ) : null}

            <div
                className={cn(
                    'mt-3 flex min-h-0 min-w-0 flex-1 flex-col pb-6',
                    scrollStyles.componentsPageScroll,
                )}
            >
                {isLoading && allEmpty ? (
                    <SectionLoading />
                ) : allEmpty ? (
                    <PagePlaceholder className="gap-2">
                        <MotionIcon
                            icon={Box}
                            motion="none"
                            playEnter={false}
                            size={28}
                            className="text-text-tertiary"
                        />
                        <p className="text-sm text-text-secondary">没有可管理的组件</p>
                        <p className="text-xs text-text-tertiary">请检查远端连接或刷新组件清单</p>
                    </PagePlaceholder>
                ) : activeMachine ? (
                    <HostComponentsView
                        machine={activeMachine}
                        appFrameworks={appsEnabled ? (appFrameworks.data ?? []) : []}
                        appInstances={apps.instances}
                        onCreateAppInstance={dialogs.handleCreateAppInstance}
                        onImportAppInstance={dialogs.handleImportAppInstance}
                        latestVersionFor={latestVersionFor}
                        latestReleaseFor={latestReleaseFor}
                        getProgress={action.getProgressFor}
                        onAction={handleAction}
                        onRetryDetect={handleRetryDetect}
                        onShowReleaseNotes={handleShowReleaseNotes}
                        lifecycleBlockedReason={lifecycleBlockedReasonForHost(
                            activeMachine.host.host_id,
                        )}
                        qqDependencyReport={activeQqDependencyReport}
                        dockerStatus={dockerHosts.statusByHost[activeMachine.host.host_id]}
                        isDockerProbing={
                            dockerHosts.probingByHost[activeMachine.host.host_id] ?? false
                        }
                        isInstallingDocker={
                            dockerHosts.installingByHost[activeMachine.host.host_id] ?? false
                        }
                        dockerInstallHint={
                            dockerHosts.installHintByHost[activeMachine.host.host_id]
                        }
                        dockerInstallProgress={dockerInstallProgress}
                        onInstallDocker={(hostId) => {
                            void handleInstallDocker(hostId);
                        }}
                        onOpenDockerDownload={dockerHosts.openDownloadPage}
                        onEnsureQqDependencies={(hostId) => {
                            void startQqDepsRepair(hostId);
                        }}
                        isPullingImage={dockerHosts.isPullingFrameworkImage}
                        onPullImage={dockerHosts.pullFrameworkImage}
                        onPullImageError={handleDockerDeployError}
                        imageReadyByFlavor={
                            dockerHosts.imageReadyByHost[activeMachine.host.host_id] ?? {}
                        }
                        containers={dockerHosts.containersByHost[activeMachine.host.host_id] ?? []}
                    />
                ) : null}
            </div>

            <ReleaseNotesDialog
                open={releaseNotesTarget != null}
                onOpenChange={(open) => {
                    if (!open) setReleaseNotesTarget(null);
                }}
                componentLabel={releaseNotesLabel(releaseNotesTarget)}
                release={releaseNotesTarget ? latestReleaseFor(releaseNotesTarget) : null}
            />

            <SnowLumaPackageDialog
                open={slPkgPrompt != null}
                onOpenChange={(open) => {
                    if (!open) closeSlPkgPrompt();
                }}
                onConfirm={confirmSnowLumaPackage}
            />

            <ImportInstanceDialog
                target={dialogs.importAppTarget}
                frameworks={appFrameworks.data ?? []}
                servers={servers}
                isImporting={apps.isImporting}
                onClose={dialogs.closeImportDialog}
                onSubmit={dialogs.submitImport}
            />

            <CreateInstanceDialog
                request={dialogs.createAppRequest}
                servers={servers}
                isCreating={apps.isCreating}
                onClose={dialogs.closeCreateDialog}
                onSubmit={dialogs.submitCreate}
            />

            {sudoPrompt && (
                <SudoPasswordDialog
                    hostName={sudoPrompt.hostName}
                    reason={
                        sudoPrompt.reason ??
                        (sudoPrompt.purpose === 'qq_deps'
                            ? '安装 QQ 系统依赖需要 sudo 权限'
                            : undefined)
                    }
                    isSubmitting={
                        sudoPrompt.purpose === 'docker'
                            ? (dockerHosts.installingByHost[sudoPrompt.hostId] ?? false)
                            : false
                    }
                    onConfirm={handleSudoConfirm}
                    onClose={closeSudoPrompt}
                />
            )}
        </div>
    );
};

const SectionLoading: React.FC = () => (
    <PagePlaceholder className="gap-2 py-12">
        <MotionIcon
            icon={Loader2}
            motion="spin"
            playEnter={false}
            size={16}
            className="text-text-tertiary"
        />
        <span className="text-sm text-text-tertiary">加载中…</span>
    </PagePlaceholder>
);

export default ComponentsPageNext;
