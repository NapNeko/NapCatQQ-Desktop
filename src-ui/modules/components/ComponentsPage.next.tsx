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

import React, { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { Box, Loader2, RefreshCw } from 'lucide-react';
import { Button } from '../../shared/ui';
import { MotionIcon, refreshMotion } from '../../shared/ui/motion';
import { useComponents } from '../../hooks/components/useComponents';
import { useComponentAction } from '../../hooks/components/useComponentAction';
import { useComponentActionErrors } from '../../hooks/components/useComponentActionErrors';
import { useComponentPageAlerts } from '../../hooks/components/useComponentPageAlerts';
import { componentService } from '../../core/services/component.service';
import { componentActionStore } from '../../hooks/components/componentActionStore';
import { useReleases } from '../../hooks/diagnostics/useReleases';
import { useDockerHosts } from '../../hooks/docker/useDockerHosts';
import { useDockerInstallProgress } from '../../hooks/docker/useDockerInstallProgress';
import { HostSwitcher } from './HostSwitcher';
import { HostComponentsView } from './HostComponentsView';
import { SudoPasswordDialog } from '../docker/SudoPasswordDialog';
import { groupByHost, type ComponentRow, type MachineView } from '../../core/domain/components/types';
import type { ComponentId, DockerInstallReport } from '../../core/ipc/types';
import type { QqDependencyReport } from '../../core/ipc/generated/qq/QqDependencyReport';
import type { DockerInstallOptions } from '../../core/services/docker.service';
import { globalInfoBarStore } from '../../hooks/ui/globalInfoBarStore';

// componentActionStore 跨路由存活，提权提示的去重状态也必须保持同样生命周期。
const qqSudoPromptedTaskIds = new Set<string>();
import { errorText } from '../../core/domain/errors';
import { cn } from '../../shared/utils/cn';
import { PagePlaceholder } from '../../shared/ui/PagePlaceholder';
import scrollStyles from './componentsPageScroll.module.css';

type QqDependencyProbeState =
    | { status: 'loading'; report: null; error: null }
    | { status: 'ready'; report: QqDependencyReport; error: null }
    | { status: 'error'; report: null; error: string };

function canProbeQqDependencies(machine: MachineView | null | undefined): machine is MachineView {
    if (!machine || machine.host.os !== 'linux') return false;
    const qq = machine.runtimeDep.find((row) => row.info.id === 'qq');
    return qq?.status.state === 'installed';
}

export const ComponentsPageNext: React.FC = () => {
    const { view, hosts, isLoading, error, refetch } = useComponents();
    const { startAction, cancelAction, getProgressFor, onTaskTerminal } = useComponentAction();
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

    const [qqDependencyByHost, setQqDependencyByHost] = useState<
        Record<string, QqDependencyProbeState | undefined>
    >({});
    const qqDependencyInFlightRef = useRef<Set<string>>(new Set());

    const probeQqDependencies = useCallback(
        async (hostId: string, force = false) => {
            const machine = machines.find((m) => m.host.host_id === hostId);
            if (!canProbeQqDependencies(machine)) return;

            const current = qqDependencyByHost[hostId];
            if (!force && current) {
                return;
            }
            if (qqDependencyInFlightRef.current.has(hostId)) return;

            qqDependencyInFlightRef.current.add(hostId);
            setQqDependencyByHost((prev) => ({
                ...prev,
                [hostId]: { status: 'loading', report: null, error: null },
            }));
            try {
                const report = await componentService.detectQqDependencies(hostId);
                setQqDependencyByHost((prev) => ({
                    ...prev,
                    [hostId]: { status: 'ready', report, error: null },
                }));
            } catch (err) {
                const message = errorText(err, 'QQ 依赖探测失败');
                setQqDependencyByHost((prev) => ({
                    ...prev,
                    [hostId]: { status: 'error', report: null, error: message },
                }));
                console.warn('[ComponentsPage] QQ dependency probe failed:', err);
            } finally {
                qqDependencyInFlightRef.current.delete(hostId);
            }
        },
        [machines, qqDependencyByHost],
    );

    useEffect(() => {
        if (!canProbeQqDependencies(activeMachine)) return;
        void probeQqDependencies(activeMachine.host.host_id);
    }, [activeMachine, probeQqDependencies]);

    const activeQqDependencyReport =
        activeMachine
            ? qqDependencyByHost[activeMachine.host.host_id]?.report ?? null
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

    const latestVersionFor = useCallback(
        (id: ComponentId): string | null => {
            switch (id) {
                case 'napcat':
                    return releases.napcat?.version ?? null;
                case 'snowluma':
                    return releases.snowluma?.version ?? null;
                case 'desktop_self':
                    return releases.desktop?.version ?? null;
                case 'ncd_watch':
                    return releases.ncdWatch?.version ?? null;
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
                onTaskTerminal(taskId, (status) => {
                    // 只在成功时刷新状态；失败/取消时不刷新，避免部分删除导致探测返回 None 误显示"未安装"。
                    if (status === 'success') {
                        refetch();
                        if (componentId === 'qq') {
                            void probeQqDependencies(hostId, true);
                        }
                    }
                });
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
        [startAction, cancelAction, onTaskTerminal, refetch, hostNameOf, probeQqDependencies],
    );

    const startQqDepsRepair = useCallback(
        async (hostId: string) => {
            try {
                const taskId = await startAction('qq', hostId, 'ensure_dependencies');
                onTaskTerminal(taskId, (status) => {
                    if (status === 'success') {
                        refetch();
                        void probeQqDependencies(hostId, true);
                    }
                });
            } catch (err) {
                globalInfoBarStore.push({
                    key: `qq-deps-repair:${hostId}`,
                    tone: 'danger',
                    title: `QQ 依赖修复失败 · ${hostNameOf(hostId)}`,
                    content: errorText(err, '无法启动修复任务'),
                    autoDismissMs: 0,
                });
            }
        },
        [startAction, onTaskTerminal, refetch, hostNameOf, probeQqDependencies],
    );

    const handleRefresh = useCallback(() => {
        refetch();
        if (activeMachine) {
            void probeQqDependencies(activeMachine.host.host_id, true);
        }
    }, [refetch, activeMachine, probeQqDependencies]);

    const handleRetryDetect = useCallback(
        (hostId: string) => {
            refetch();
            void probeQqDependencies(hostId, true);
        },
        [refetch, probeQqDependencies],
    );

    const allEmpty = machines.length === 0;

    // Docker / QQ 依赖补全：需要 sudo 时弹密码框，记住后重试。
    const [sudoPrompt, setSudoPrompt] = useState<{
        hostId: string;
        hostName: string;
        reason?: string;
        purpose: 'docker' | 'qq_deps';
    } | null>(null);

    const componentActionSnap = useSyncExternalStore(
        componentActionStore.subscribe,
        componentActionStore.getSnapshot,
        componentActionStore.getSnapshot,
    );
    useEffect(() => {
        for (const taskId of Array.from(qqSudoPromptedTaskIds)) {
            if (!(taskId in componentActionSnap.tasks)) {
                qqSudoPromptedTaskIds.delete(taskId);
            }
        }
        for (const [taskId, progress] of Object.entries(componentActionSnap.tasks)) {
            if (progress.status !== 'failed') continue;
            if (qqSudoPromptedTaskIds.has(taskId)) continue;
            const target = componentActionSnap.taskTargets[taskId];
            if (!target || target.componentId !== 'qq') continue;
            const msg = [...progress.logs].reverse().find((l) => l.level === 'error')?.message
                ?? progress.message;
            if (!msg.includes('elevation_required')) continue;
            qqSudoPromptedTaskIds.add(taskId);
            globalInfoBarStore.push({
                key: `qq-deps-sudo:${target.hostId}`,
                tone: 'warning',
                title: `需要 sudo 密码 · ${hostNameOf(target.hostId)}`,
                content: '安装 QQ 系统依赖需要提权，请输入密码后重试。',
                autoDismissMs: 0,
            });
            setSudoPrompt(p => p ?? {
                hostId: target.hostId,
                hostName: hostNameOf(target.hostId),
                reason: msg,
                purpose: 'qq_deps',
            });
            break;
        }
    }, [componentActionSnap, hostNameOf]);

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
                    setSudoPrompt({
                        hostId,
                        hostName: hostNameOf(hostId),
                        reason: report.message,
                        purpose: 'docker',
                    });
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
            if (sudoPrompt.purpose === 'qq_deps') {
                const serverId = sudoPrompt.hostId.replace(/^remote:/, '');
                if (remember) {
                    await componentService.rememberSudoPassword(serverId, password);
                }
                setSudoPrompt(null);
                await startQqDepsRepair(sudoPrompt.hostId);
                return;
            }
            const report = await runInstall(sudoPrompt.hostId, {
                sudoPassword: password,
                rememberSudo: remember,
            });
            if (report.status === 'needSudoPassword') {
                throw new Error(report.message);
            }
            setSudoPrompt(null);
        },
        [sudoPrompt, runInstall, startQqDepsRepair],
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
                <Button size="sm" variant="secondary" onClick={handleRefresh} disabled={isLoading}>
                    <MotionIcon
                        icon={RefreshCw}
                        motion={refreshMotion(isLoading)}
                        playEnter={false}
                        size={14}
                    />
                    刷新
                </Button>
            </header>

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
                ) : allEmpty ? (
                    <PagePlaceholder className="gap-2">
                        <MotionIcon icon={Box} motion="none" playEnter={false} size={28} className="text-text-tertiary" />
                        <p className="text-sm text-text-secondary">没有可管理的组件</p>
                        <p className="text-xs text-text-tertiary">请检查远端连接或刷新组件清单</p>
                    </PagePlaceholder>
                ) : activeMachine ? (
                    <HostComponentsView
                        machine={activeMachine}
                        latestVersionFor={latestVersionFor}
                        getProgress={getProgressFor}
                        onAction={handleAction}
                        onRetryDetect={handleRetryDetect}
                        qqDependencyReport={activeQqDependencyReport}
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

            {sudoPrompt && (
                <SudoPasswordDialog
                    hostName={sudoPrompt.hostName}
                    reason={
                        sudoPrompt.reason
                        ?? (sudoPrompt.purpose === 'qq_deps'
                            ? '安装 QQ 系统依赖需要 sudo 权限'
                            : undefined)
                    }
                    isSubmitting={
                        sudoPrompt.purpose === 'docker'
                            ? (dockerHosts.installingByHost[sudoPrompt.hostId] ?? false)
                            : false
                    }
                    onConfirm={handleSudoConfirm}
                    onClose={() => setSudoPrompt(null)}
                />
            )}
        </div>
    );
};

const SectionLoading: React.FC = () => (
    <PagePlaceholder className="gap-2 py-12">
        <MotionIcon icon={Loader2} motion="spin" playEnter={false} size={16} className="text-text-tertiary" />
        <span className="text-sm text-text-tertiary">加载中…</span>
    </PagePlaceholder>
);

export default ComponentsPageNext;
