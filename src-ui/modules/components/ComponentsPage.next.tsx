// Components 页（next）：单机视图 + 主机切换。
//
// 交互：页面一次只展示一台机器的组件。顶部一排主机切换标签（本机 / 各远端），
// 点哪台就在下方铺哪台的组件，按框架 / 运行时依赖 / 桌面端分组成网格。装不了
// 的组件（平台不支持）不出现。docker 就绪的机器在末尾带 Docker 部署区。
//
// 只有一台机器时不显示切换条。这样"组件 × 各主机"被翻成"先选机器、再看这台
// 机器能装啥"，扫描成本远低于把每台机器堆成一张大卡上下排。
//
// 严守 frontend-layering：仅 import hooks / shared/ui / 自身组件 + domain
// 纯函数，不直接调 service / @tauri-apps。

import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { Box, Loader2, RefreshCw } from 'lucide-react';
import { Button } from '../../shared/ui';
import { MotionIcon, refreshMotion } from '../../shared/ui/motion';
import { useComponents } from '../../hooks/components/useComponents';
import { useComponentAction } from '../../hooks/components/useComponentAction';
import { useComponentActionErrors } from '../../hooks/components/useComponentActionErrors';
import { useReleases } from '../../hooks/diagnostics/useReleases';
import { useDockerHosts } from '../../hooks/docker/useDockerHosts';
import { useDockerInstallProgress } from '../../hooks/docker/useDockerInstallProgress';
import { HostSwitcher } from './HostSwitcher';
import { HostComponentsView } from './HostComponentsView';
import { SudoPasswordDialog } from '../docker/SudoPasswordDialog';
import { groupByHost, type ComponentRow, type MachineView } from '../../core/domain/components/types';
import type { ComponentId, DockerInstallReport } from '../../core/ipc/types';
import type { DockerInstallOptions } from '../../core/services/docker.service';
import { globalInfoBarStore } from '../../hooks/ui/globalInfoBarStore';
import { errorText } from '../../core/domain/errors';
import { cn } from '../../shared/utils/cn';
import scrollStyles from './componentsPageScroll.module.css';

export const ComponentsPageNext: React.FC = () => {
    const { view, hosts, isLoading, error, refetch } = useComponents();
    const { startAction, cancelAction, getProgressFor, isInstalling, onTaskTerminal } = useComponentAction();
    const { snapshot: releases } = useReleases();

    const hostIds = useMemo(() => hosts.map((h) => h.host_id), [hosts]);
    const dockerHosts = useDockerHosts(hostIds);

    // 组件主导矩阵 → 主机主导，再剔掉这台机器一个组件都装不了的空机器。
    const allRows = useMemo<ComponentRow[]>(
        () => [...view.framework, ...view.runtimeDep, ...view.selfApp],
        [view],
    );
    const machines = useMemo<MachineView[]>(() => {
        const grouped = groupByHost(allRows, hosts);
        return grouped.filter(
            (m) => m.framework.length + m.runtimeDep.length + m.selfApp.length > 0,
        );
    }, [allRows, hosts]);

    // 终态错误 push 进全局 InfoBar（顶层 InfoBarStack 渲染）。
    useComponentActionErrors(allRows);

    // 选中的主机：默认停在第一台（本机）。机器列表变动后若当前选中项消失，
    // 回落到第一台，避免选中一台已被移除的远端导致空白。
    const [activeHostId, setActiveHostId] = useState<string | null>(null);
    useEffect(() => {
        if (machines.length === 0) {
            if (activeHostId !== null) setActiveHostId(null);
            return;
        }
        const stillThere = machines.some((m) => m.host.host_id === activeHostId);
        if (!stillThere) setActiveHostId(machines[0].host.host_id);
    }, [machines, activeHostId]);

    const activeMachine = useMemo(
        () => machines.find((m) => m.host.host_id === activeHostId) ?? machines[0] ?? null,
        [machines, activeHostId],
    );

    const dockerInstallProgress = useDockerInstallProgress(activeMachine?.host.host_id ?? '');

    const hostNameOf = useCallback(
        (hostId: string) =>
            machines.find((m) => m.host.host_id === hostId)?.host.display_name ?? hostId,
        [machines],
    );

    const latestVersionFor = useCallback(
        (id: ComponentId): string | null => {
            switch (id) {
                case 'napcat':
                    return releases.napcat?.version ?? null;
                case 'snowluma':
                    return releases.snowluma?.version ?? null;
                case 'desktop_self':
                    return releases.desktop?.version ?? null;
                default:
                    return null;
            }
        },
        [releases],
    );

    const handleAction = useCallback(
        async (
            componentId: ComponentId,
            hostId: string,
            payload: { stepKind: import('../../core/ipc/types').StepKind } | { cancelTaskId: string },
        ) => {
            try {
                if ('cancelTaskId' in payload) {
                    await cancelAction(payload.cancelTaskId);
                    return;
                }
                const taskId = await startAction(componentId, hostId, payload.stepKind);
                onTaskTerminal(taskId, () => refetch());
            } catch (err) {
                const hostName = hostNameOf(hostId);
                globalInfoBarStore.push({
                    key: `component-action-start:${componentId}:${hostId}`,
                    tone: 'danger',
                    title: `组件操作失败 · ${hostName}`,
                    content: errorText(err, '组件操作失败，请稍后重试'),
                    autoDismissMs: 0,
                });
                console.error('[ComponentsPage] action failed:', err);
            }
        },
        [startAction, cancelAction, onTaskTerminal, refetch, hostNameOf],
    );

    const allEmpty = machines.length === 0;

    // Docker 安装链路自带反馈:成功弹绿条,彻底装不了弹红条(带主机名)。远端密钥
    // 登录且没缓存密码时,后端返回 needSudoPassword,这里改弹密码输入框,用户填了
    // 带密码重试,而不是笼统报失败。
    const [sudoPrompt, setSudoPrompt] = useState<{
        hostId: string;
        hostName: string;
        reason?: string;
    } | null>(null);

    // 执行一次安装并按 status 分流。返回 report 给调用方(弹框重试时要据此判断
    // 是否仍需密码)。底层 IPC 失败(连接断等)会抛,交给调用方处理。
    const runInstall = useCallback(
        async (hostId: string, options?: DockerInstallOptions): Promise<DockerInstallReport> => {
            const hostName = hostNameOf(hostId);
            const report = await dockerHosts.install(hostId, options);
            switch (report.status) {
                case 'installed':
                case 'alreadyInstalled':
                    globalInfoBarStore.push({
                        key: `docker-install:${hostId}`,
                        tone: 'success',
                        title: `Docker · ${hostName}`,
                        content: report.message,
                        autoDismissMs: 8000,
                    });
                    break;
                case 'manualRequired':
                    globalInfoBarStore.push({
                        key: `docker-install:${hostId}`,
                        tone: 'danger',
                        title: `Docker 未就绪 · ${hostName}`,
                        content: report.message,
                        autoDismissMs: 0,
                    });
                    break;
                case 'needSudoPassword':
                    // 弹框(或更新已开弹框的提示文案)向用户要 sudo 密码。
                    break;
            }
            return report;
        },
        [dockerHosts, hostNameOf],
    );

    // 组件卡片上的"安装 Docker"按钮入口:首次尝试不带密码(后端会自己探 root/
    // 免密/keyring 缓存密码)。只有探下来确实要密码且无缓存时才弹框。
    const handleInstallDocker = useCallback(
        async (hostId: string) => {
            try {
                const report = await runInstall(hostId);
                if (report.status === 'needSudoPassword') {
                    setSudoPrompt({ hostId, hostName: hostNameOf(hostId), reason: report.message });
                }
            } catch (err) {
                globalInfoBarStore.push({
                    key: `docker-install:${hostId}`,
                    tone: 'danger',
                    title: `Docker 安装失败 · ${hostNameOf(hostId)}`,
                    content: errorText(err, 'Docker 安装失败，请手动安装后重试'),
                    autoDismissMs: 0,
                });
            }
        },
        [runInstall, hostNameOf],
    );

    const handleDockerDeployError = useCallback(
        (hostId: string, flavor: import('../../core/ipc/types').DockerFlavor, err: unknown) => {
            const framework = flavor === 'napcat' ? 'NapCat' : 'SnowLuma';
            globalInfoBarStore.push({
                key: `docker-deploy:${hostId}:${flavor}`,
                tone: 'danger',
                title: `${framework} Docker 部署失败 · ${hostNameOf(hostId)}`,
                content: errorText(err, 'Docker 部署失败，请检查 Docker 状态、镜像源与端口占用后重试'),
                autoDismissMs: 0,
            });
        },
        [hostNameOf],
    );

    // 弹框确认:带用户输入的密码重试。装成功就关弹框;密码不对(后端再次返回
    // needSudoPassword)就抛出去,让弹框内联显示"密码不正确"并保持打开。
    const handleSudoConfirm = useCallback(
        async (password: string, remember: boolean) => {
            if (!sudoPrompt) return;
            const report = await runInstall(sudoPrompt.hostId, {
                sudoPassword: password,
                rememberSudo: remember,
            });
            if (report.status === 'needSudoPassword') {
                throw new Error(report.message);
            }
            // installed / alreadyInstalled / manualRequired 都已在 runInstall 里弹了
            // 对应提示条,这里收起弹框即可。
            setSudoPrompt(null);
        },
        [sudoPrompt, runInstall],
    );

    return (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
            <header className="flex shrink-0 items-end justify-between pb-4 pt-2">
                <div>
                    <p className="text-2xs uppercase tracking-widest text-text-tertiary">
                        components
                    </p>
                    <h1 className="font-display text-xl font-semibold text-text">组件管理</h1>
                    <p className="mt-1 text-sm text-text-secondary">
                        选一台机器，管理它上面的 Bot 框架与运行时依赖：安装、更新、卸载、容器部署。
                    </p>
                </div>
                <Button size="sm" variant="secondary" onClick={refetch} disabled={isLoading}>
                    <MotionIcon
                        icon={RefreshCw}
                        motion={refreshMotion(isLoading)}
                        playEnter={false}
                        size={14}
                    />
                    刷新
                </Button>
            </header>

            {error && <ErrorBanner message={error.message} onRetry={refetch} />}

            {machines.length > 1 && activeHostId && (
                <HostSwitcher
                    machines={machines}
                    activeHostId={activeHostId}
                    onSelect={setActiveHostId}
                />
            )}

            <div
                className={cn(
                    'mt-3 flex min-h-0 min-w-0 flex-1 flex-col pb-6',
                    scrollStyles.componentsPageScroll,
                )}
            >
                {isLoading && allEmpty ? (
                    <SectionLoading />
                ) : activeMachine ? (
                    <HostComponentsView
                        machine={activeMachine}
                        latestVersionFor={latestVersionFor}
                        getProgress={getProgressFor}
                        isInstalling={isInstalling}
                        onAction={handleAction}
                        onRetryDetect={() => refetch()}
                        dockerStatus={dockerHosts.statusByHost[activeMachine.host.host_id]}
                        isDockerProbing={dockerHosts.probingByHost[activeMachine.host.host_id] ?? false}
                        isInstallingDocker={dockerHosts.installingByHost[activeMachine.host.host_id] ?? false}
                        dockerInstallHint={
                            dockerHosts.installHintByHost[activeMachine.host.host_id]
                        }
                        dockerInstallProgress={dockerInstallProgress}
                        onInstallDocker={(hostId) => {
                            void handleInstallDocker(hostId);
                        }}
                        onOpenDockerDownload={() => {
                            void dockerHosts.openDownloadPage().catch(() => undefined);
                        }}
                        isDeploying={dockerHosts.isDeploying}
                        onDeploy={dockerHosts.deploy}
                        onDeployError={handleDockerDeployError}
                        containers={dockerHosts.containersByHost[activeMachine.host.host_id] ?? []}
                    />
                ) : null}
            </div>

            {sudoPrompt && (
                <SudoPasswordDialog
                    hostName={sudoPrompt.hostName}
                    reason={sudoPrompt.reason}
                    isSubmitting={dockerHosts.installingByHost[sudoPrompt.hostId] ?? false}
                    onConfirm={handleSudoConfirm}
                    onClose={() => setSudoPrompt(null)}
                />
            )}
        </div>
    );
};

const SectionLoading: React.FC = () => (
    <div className="flex items-center gap-2 rounded-md bg-inset/40 p-6 text-text-tertiary">
        <MotionIcon icon={Loader2} motion="spin" playEnter={false} size={16} />
        <span className="text-sm">加载中…</span>
    </div>
);

const ErrorBanner: React.FC<{ message: string; onRetry: () => void }> = ({ message, onRetry }) => (
    <div className="mb-3 flex shrink-0 items-center justify-between gap-3 rounded-md border border-danger/30 bg-danger-soft px-4 py-3">
        <div className="flex items-center gap-2">
            <Box size={16} className="text-danger" />
            <span className="text-sm text-text">加载组件清单失败：{message}</span>
        </div>
        <Button size="sm" variant="ghost" onClick={onRetry}>
            重试
        </Button>
    </div>
);

export default ComponentsPageNext;
