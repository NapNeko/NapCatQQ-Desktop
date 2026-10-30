// 单台主机的组件分组与 Docker 部署区。

import React from 'react';
import { PackageX, WifiOff, Wrench } from 'lucide-react';
import { Button } from '../../shared/ui';
import { PagePlaceholder } from '../../shared/ui/PagePlaceholder';
import { MachineComponentRowView } from './MachineComponentRow';
import { componentCardGridClass } from './ComponentEntityCard';
import { DockerDeploymentSection } from './DockerDeploymentSection';
import { AppFrameworkRow, runtimeDepsFor } from './AppFrameworkRow';
import { isHostConnectivityFailureReason } from '../../core/domain/components/types';
import { componentDisplayName } from '../../core/domain/task-queue/labels';
import type { MachineView, MachineComponentRow } from '../../core/domain/components/types';
import type { ActionProgressView } from '../../core/domain/components/progress';
import type { ComponentActionProgress } from '../../hooks/components/useComponentAction';
import type { ReleaseInfoView } from '../../core/domain/release/normalize';
import type { QqDependencyReport } from '../../core/ipc/generated/qq/QqDependencyReport';
import type {
    AppFrameworkManifest,
    AppInstance,
    ComponentId,
    ContainerInfo,
    DeployedContainer,
    DockerFlavor,
    DockerStatus,
    StepKind,
} from '../../core/ipc/types';

interface HostComponentsViewProps {
    machine: MachineView;
    // 应用端（按实例安装的合成行）
    appFrameworks: AppFrameworkManifest[];
    appInstances: AppInstance[];
    onCreateAppInstance: (manifest: AppFrameworkManifest, hostId: string) => void;
    onImportAppInstance: (manifest: AppFrameworkManifest, hostId: string) => void;
    latestVersionFor: (id: ComponentId) => string | null;
    /** 远端 release 全文（含更新日志）；无则不显示「日志」按钮 */
    latestReleaseFor: (id: ComponentId) => ReleaseInfoView | null;
    getProgress: (componentId: ComponentId, hostId: string) => ComponentActionProgress | null;
    onAction: (
        componentId: ComponentId,
        hostId: string,
        action: { stepKind: StepKind } | { cancelTaskId: string },
    ) => void;
    onRetryDetect: (hostId: string) => void;
    onShowReleaseNotes: (componentId: ComponentId) => void;
    /** 该主机有活跃 Bot 时，update/uninstall 按钮禁用文案 */
    lifecycleBlockedReason?: string | null;
    // Docker 数据 + 动作（来自 useDockerHosts）
    dockerStatus: DockerStatus | undefined;
    dockerProbeError?: unknown;
    dockerSectionRef?: React.Ref<HTMLElement>;
    isDockerProbing: boolean;
    isInstallingDocker: boolean;
    dockerInstallHint?: string;
    dockerInstallProgress?: ActionProgressView | null;
    onInstallDocker: (hostId: string) => void;
    onOpenDockerDownload: () => void;
    onEnsureQqDependencies: (hostId: string) => void;
    qqDependencyReport?: QqDependencyReport | null;
    isPullingImage: (hostId: string, flavor: DockerFlavor) => boolean;
    onPullImage: (
        hostId: string,
        flavor: DockerFlavor,
        taskId: string,
        mirror?: string | null,
    ) => Promise<DeployedContainer>;
    onPullImageError?: (hostId: string, flavor: DockerFlavor, error: unknown) => void;
    // 这台主机各 flavor 官方镜像是否已拉取（不创建 napcat/snowluma 演示容器）。
    imageReadyByFlavor: Partial<Record<DockerFlavor, boolean | undefined>>;
    imageErrorByFlavor?: Partial<Record<DockerFlavor, unknown>>;
    // 容器列表（Docker 管理页等；框架按钮不再据此判定）。
    containers: ContainerInfo[];
}

export const HostComponentsView: React.FC<HostComponentsViewProps> = ({
    machine,
    appFrameworks,
    appInstances,
    onCreateAppInstance,
    onImportAppInstance,
    latestVersionFor,
    latestReleaseFor,
    getProgress,
    onAction,
    onRetryDetect,
    onShowReleaseNotes,
    lifecycleBlockedReason = null,
    dockerStatus,
    dockerProbeError,
    dockerSectionRef,
    isDockerProbing,
    isInstallingDocker,
    dockerInstallHint,
    dockerInstallProgress,
    onInstallDocker,
    onOpenDockerDownload,
    onEnsureQqDependencies,
    qqDependencyReport,
    isPullingImage,
    onPullImage,
    onPullImageError,
    imageReadyByFlavor,
    imageErrorByFlavor,
    containers: _containers,
}) => {
    const { host } = machine;

    const empty =
        machine.framework.length + machine.runtimeDep.length + machine.selfApp.length === 0;

    const allComponents = [...machine.framework, ...machine.runtimeDep, ...machine.selfApp];

    const dockerApplicable = host.os === 'linux';

    if (empty) {
        return (
            <PagePlaceholder className="gap-2">
                <PackageX size={28} className="text-text-tertiary" strokeWidth={1.5} />
                <p className="text-sm text-text-secondary">这台机器上没有可管理的组件</p>
                <p className="text-xs text-text-tertiary">
                    {host.os} · {host.locality === 'remote' ? '远端' : '本机'} 不支持任何已知组件
                </p>
            </PagePlaceholder>
        );
    }

    // 主机级连接失败占位：远端且所有行都是 unknown + 原因指向连接失败时，
    // 直接在内容区给清晰提示 + 快捷操作，避免用户只看到一堆“探测失败，请看顶部”。
    const allUnknown = allComponents.every((r) => r.status.state === 'unknown');
    const anyConnectivityFail = allComponents.some(
        (r) =>
            r.status.state === 'unknown' &&
            isHostConnectivityFailureReason(
                (r.status as { state: 'unknown'; reason: string }).reason,
            ),
    );
    const hostConnectFailed = host.locality === 'remote' && allUnknown && anyConnectivityFail;

    if (hostConnectFailed) {
        const found = allComponents.find(
            (r) =>
                r.status.state === 'unknown' &&
                isHostConnectivityFailureReason(
                    (r.status as { state: 'unknown'; reason: string }).reason,
                ),
        );
        const sample = found?.status.state === 'unknown' ? found.status.reason : '连接失败';
        if (sample && sample !== '连接失败') {
            console.error('[components] host unreachable', host.host_id, sample);
        }
        return (
            <div className="flex min-w-0 w-full max-w-full flex-col gap-6">
                <div className="flex min-h-[220px] w-full flex-col items-center justify-center gap-3 rounded-md border border-border-subtle bg-surface/40 px-6 py-10 text-center">
                    <WifiOff size={28} className="text-text-tertiary" strokeWidth={1.5} />
                    <p className="text-sm text-text-secondary">{host.display_name} 主机不可达</p>
                    <p className="text-2xs text-text-tertiary">
                        到「远端」页测试连接，或检查网络 / 防火墙 / SSH 配置。
                    </p>
                    <div className="mt-1">
                        <Button
                            size="sm"
                            variant="primary"
                            onClick={() => onRetryDetect(host.host_id)}
                        >
                            重试探测
                        </Button>
                    </div>
                </div>
            </div>
        );
    }

    const runtimeTrailingFor = (row: MachineComponentRow): React.ReactNode => {
        if (row.info.id !== 'qq' || host.os !== 'linux') return null;
        if (row.status.state !== 'installed') return null;
        if (!qqDependencyReport || qqDependencyReport.missing.length === 0) return null;
        return (
            <Button
                size="sm"
                variant="secondary"
                title={`缺失 ${qqDependencyReport.missing.length} 个 QQ 系统依赖`}
                onClick={() => onEnsureQqDependencies(host.host_id)}
            >
                <Wrench size={13} strokeWidth={2} />
                补全依赖
            </Button>
        );
    };

    const isDemo = host.host_id === 'tour:demo-remote'; // DEMO_REMOTE_HOST_ID
    const showDocker = dockerApplicable && !isDemo;

    return (
        <div className="flex min-w-0 w-full max-w-full flex-col gap-6">
            {isDemo ? (
                <p className="rounded-md border border-brand/25 bg-brand/[0.06] px-3 py-2 text-[12.5px] leading-relaxed text-text-secondary">
                    当前是<strong className="font-medium text-text">演示远端</strong>
                    ，不会连真机、也不写入远程档案。仅用于预览远端 NC / SL 的组件清单。
                </p>
            ) : null}

            {machine.framework.length > 0 ? (
                <div className="flex w-full flex-col" data-tour-id="comp-group-framework">
                    <Group
                        title="协议端"
                        rows={machine.framework}
                        hostId={host.host_id}
                        disableActions={isDemo}
                        lifecycleBlockedReason={lifecycleBlockedReason}
                        latestVersionFor={latestVersionFor}
                        latestReleaseFor={latestReleaseFor}
                        getProgress={getProgress}
                        onAction={onAction}
                        onRetryDetect={onRetryDetect}
                        onShowReleaseNotes={onShowReleaseNotes}
                    />
                </div>
            ) : null}

            {showDocker && (
                <DockerDeploymentSection
                    key={host.host_id}
                    hostId={host.host_id}
                    hostLabel={host.display_name}
                    os={host.os}
                    status={dockerStatus}
                    probeError={dockerProbeError}
                    isProbing={isDockerProbing}
                    isInstalling={isInstallingDocker}
                    installHint={dockerInstallHint}
                    installProgress={dockerInstallProgress}
                    flavors={machine.framework.flatMap((row) => {
                        const flavor = frameworkFlavor(row.info.id);
                        return flavor ? [flavor] : [];
                    })}
                    imageReadyByFlavor={imageReadyByFlavor}
                    imageErrorByFlavor={imageErrorByFlavor}
                    isPullingImage={isPullingImage}
                    onPullImage={onPullImage}
                    onPullImageError={onPullImageError}
                    onInstall={() => onInstallDocker(host.host_id)}
                    onRetry={() => onRetryDetect(host.host_id)}
                    onOpenDownload={onOpenDockerDownload}
                    sectionRef={dockerSectionRef}
                />
            )}

            <AppFrameworkGroup
                manifests={appFrameworks}
                instances={appInstances}
                machine={machine}
                disableActions={isDemo}
                onCreate={(manifest) => onCreateAppInstance(manifest, host.host_id)}
                onImport={(manifest) => onImportAppInstance(manifest, host.host_id)}
            />

            <div data-tour-id="comp-group-runtime">
                <RuntimeDepGroup
                    rows={machine.runtimeDep}
                    hostId={host.host_id}
                    disableActions={isDemo}
                    lifecycleBlockedReason={lifecycleBlockedReason}
                    latestVersionFor={latestVersionFor}
                    latestReleaseFor={latestReleaseFor}
                    getProgress={getProgress}
                    onAction={onAction}
                    onRetryDetect={onRetryDetect}
                    onShowReleaseNotes={onShowReleaseNotes}
                    trailingFor={isDemo ? undefined : runtimeTrailingFor}
                />
            </div>

            {machine.selfApp.length > 0 ? (
                <Group
                    title="桌面端"
                    rows={machine.selfApp}
                    hostId={host.host_id}
                    disableActions={false}
                    lifecycleBlockedReason={lifecycleBlockedReason}
                    latestVersionFor={latestVersionFor}
                    latestReleaseFor={latestReleaseFor}
                    getProgress={getProgress}
                    onAction={onAction}
                    onRetryDetect={onRetryDetect}
                    onShowReleaseNotes={onShowReleaseNotes}
                />
            ) : null}
        </div>
    );
};

/// 轻量分组容器：对齐页面左右边缘，不带左侧竖线缩进。
const ComponentSection: React.FC<{
    title: string;
    count: number;
    description?: string;
    children: React.ReactNode;
}> = ({ title, count, description, children }) => (
    <section className="flex flex-col gap-2.5">
        <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="flex items-center gap-2">
                <span aria-hidden className="h-3.5 w-1 rounded-full bg-brand/70" />
                <h3 className="text-[13px] font-semibold tracking-tight text-text">{title}</h3>
                <span className="rounded-xs bg-inset/80 px-1.5 py-0.5 font-mono text-[10px] leading-none text-text-tertiary tabular-nums">
                    {count}
                </span>
            </div>
            {description ? (
                <span className="text-2xs text-text-tertiary">{description}</span>
            ) : null}
        </div>
        {children}
    </section>
);

/// 应用端组：每个框架一张卡，只列这台主机上的实例；不支持该主机 placement 的框架也保留卡片但标「不支持」。
const AppFrameworkGroup: React.FC<{
    manifests: AppFrameworkManifest[];
    instances: AppInstance[];
    machine: MachineView;
    disableActions: boolean;
    onCreate: (manifest: AppFrameworkManifest) => void;
    onImport: (manifest: AppFrameworkManifest) => void;
}> = ({ manifests, instances, machine, disableActions, onCreate, onImport }) => {
    if (manifests.length === 0) return null;
    const { host } = machine;
    return (
        <div data-tour-id="comp-group-app">
            <ComponentSection
                title="应用端"
                count={manifests.length}
                description="按实例安装；启停、对接与日志在「应用端」页"
            >
                <div className={componentCardGridClass}>
                    {manifests.map((manifest) => (
                        <div
                            key={manifest.id}
                            data-tour-id={`comp-row-${manifest.component_id}`}
                            className="min-w-0"
                        >
                            <AppFrameworkRow
                                manifest={manifest}
                                host={host}
                                instances={instances.filter(
                                    (i) =>
                                        i.host_id === host.host_id &&
                                        i.framework_id === manifest.id,
                                )}
                                runtimeDeps={runtimeDepsFor(manifest, machine.runtimeDep, (id) =>
                                    componentDisplayName(id as ComponentId),
                                )}
                                disabled={disableActions}
                                onCreate={() => onCreate(manifest)}
                                onImport={() => onImport(manifest)}
                            />
                        </div>
                    ))}
                </div>
            </ComponentSection>
        </div>
    );
};

/// 框架 component_id → docker 部署口味。其余组件无部署形态返回 null。
function frameworkFlavor(id: ComponentId): DockerFlavor | null {
    if (id === 'napcat') return 'napcat';
    if (id === 'snowluma') return 'snowluma';
    return null;
}

const Group: React.FC<{
    title: string;
    description?: string;
    rows: MachineComponentRow[];
    hostId: string;
    disableActions?: boolean;
    lifecycleBlockedReason?: string | null;
    latestVersionFor: (id: ComponentId) => string | null;
    latestReleaseFor: (id: ComponentId) => ReleaseInfoView | null;
    getProgress: (componentId: ComponentId, hostId: string) => ComponentActionProgress | null;
    onAction: (
        componentId: ComponentId,
        hostId: string,
        action: { stepKind: StepKind } | { cancelTaskId: string },
    ) => void;
    onRetryDetect: (hostId: string) => void;
    onShowReleaseNotes: (componentId: ComponentId) => void;
    trailingFor?: (row: MachineComponentRow) => React.ReactNode;
}> = ({
    title,
    description,
    rows,
    hostId,
    disableActions = false,
    lifecycleBlockedReason = null,
    latestVersionFor,
    latestReleaseFor,
    getProgress,
    onAction,
    onRetryDetect,
    onShowReleaseNotes,
    trailingFor,
}) => {
    if (rows.length === 0) return null;
    return (
        <ComponentSection title={title} count={rows.length} description={description}>
            <div className={componentCardGridClass}>
                {rows.map((row) => (
                    <div
                        key={row.info.id}
                        data-tour-id={`comp-row-${row.info.id}`}
                        className="min-w-0"
                    >
                        <MachineComponentRowView
                            row={row}
                            hostId={hostId}
                            latestRemoteVersion={latestVersionFor(row.info.id)}
                            latestRelease={latestReleaseFor(row.info.id)}
                            activeProgress={getProgress(row.info.id, hostId)}
                            disabled={disableActions}
                            lifecycleBlockedReason={lifecycleBlockedReason}
                            onAction={(action) => onAction(row.info.id, hostId, action)}
                            onRetryDetect={() => onRetryDetect(hostId)}
                            onShowReleaseNotes={() => onShowReleaseNotes(row.info.id)}
                            trailingActions={trailingFor?.(row)}
                        />
                    </div>
                ))}
            </div>
        </ComponentSection>
    );
};

/// 运行时依赖按当前主机的组件清单显示。
const RuntimeDepGroup: React.FC<{
    rows: MachineComponentRow[];
    hostId: string;
    disableActions?: boolean;
    lifecycleBlockedReason?: string | null;
    latestVersionFor: (id: ComponentId) => string | null;
    latestReleaseFor: (id: ComponentId) => ReleaseInfoView | null;
    getProgress: (componentId: ComponentId, hostId: string) => ComponentActionProgress | null;
    onAction: (
        componentId: ComponentId,
        hostId: string,
        action: { stepKind: StepKind } | { cancelTaskId: string },
    ) => void;
    onRetryDetect: (hostId: string) => void;
    onShowReleaseNotes: (componentId: ComponentId) => void;
    trailingFor?: (row: MachineComponentRow) => React.ReactNode;
}> = ({
    rows,
    hostId,
    disableActions = false,
    lifecycleBlockedReason = null,
    latestVersionFor,
    latestReleaseFor,
    getProgress,
    onAction,
    onRetryDetect,
    onShowReleaseNotes,
    trailingFor,
}) => {
    const count = rows.length;
    if (count === 0) return null;
    return (
        <ComponentSection title="运行时依赖" count={count}>
            <div className={componentCardGridClass}>
                {rows.map((row) => (
                    <div
                        key={row.info.id}
                        data-tour-id={`comp-row-${row.info.id}`}
                        className="min-w-0"
                    >
                        <MachineComponentRowView
                            row={row}
                            hostId={hostId}
                            latestRemoteVersion={latestVersionFor(row.info.id)}
                            latestRelease={latestReleaseFor(row.info.id)}
                            activeProgress={getProgress(row.info.id, hostId)}
                            disabled={disableActions}
                            lifecycleBlockedReason={lifecycleBlockedReason}
                            onAction={(action) => onAction(row.info.id, hostId, action)}
                            onRetryDetect={() => onRetryDetect(hostId)}
                            onShowReleaseNotes={() => onShowReleaseNotes(row.info.id)}
                            trailingActions={trailingFor?.(row)}
                        />
                    </div>
                ))}
            </div>
        </ComponentSection>
    );
};

export default HostComponentsView;
