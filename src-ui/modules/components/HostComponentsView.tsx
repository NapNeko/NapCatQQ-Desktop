// 单台主机的组件视图：按「框架 / 运行时依赖 / 桌面端」分组；组内 md 起双列网格满宽。
//
// Docker 在这里被当成正常的运行时依赖：
//   - 运行时依赖组里有一行 Docker（状态来自探测，安装走 docker hook）。
//   - 框架行（NapCat / SnowLuma）在 docker 就绪时各带一个「拉镜像」按钮，
//     仅预拉框架镜像；Bot 容器在 Bot 页启动时创建。

import React from 'react';
import { PackageX, WifiOff, Wrench } from 'lucide-react';
import { Button, FormSection } from '../../shared/ui';
import { PagePlaceholder } from '../../shared/ui/PagePlaceholder';
import { MachineComponentRowView } from './MachineComponentRow';
import { componentCardGridClass } from './ComponentEntityCard';
import { DockerRow } from './DockerRow';
import { FrameworkDockerDeployButton } from './FrameworkDockerDeploy';
import { dockerStatusSummary } from '../../core/domain/docker/status';
import { isHostConnectivityFailureReason } from '../../core/domain/components/types';
import type { MachineView, MachineComponentRow } from '../../core/domain/components/types';
import type { ActionProgressView } from '../../core/domain/components/progress';
import type { ReleaseInfoView } from '../../core/domain/release/normalize';
import type { QqDependencyReport } from '../../core/ipc/generated/qq/QqDependencyReport';
import type {
    ComponentId,
    ContainerInfo,
    DeployedContainer,
    DockerFlavor,
    DockerStatus,
    Os,
    StepKind,
} from '../../core/ipc/types';
interface HostComponentsViewProps {
    machine: MachineView;
    latestVersionFor: (id: ComponentId) => string | null;
    /** 远端 release 全文（含更新日志）；无则不显示「日志」按钮 */
    latestReleaseFor: (id: ComponentId) => ReleaseInfoView | null;
    getProgress: (
        componentId: ComponentId,
        hostId: string,
    ) => { taskId: string; progress: ActionProgressView } | null;
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
    isDockerProbing: boolean;
    isInstallingDocker: boolean;
    dockerInstallHint?: string;
    dockerInstallProgress?: ActionProgressView | null;
    onInstallDocker: (hostId: string) => void;
    onOpenDockerDownload: () => void;
    onEnsureQqDependencies: (hostId: string) => void;
    qqDependencyReport?: QqDependencyReport | null;
    isPullingImage: (hostId: string, flavor: DockerFlavor) => boolean;
    onPullImage: (hostId: string, flavor: DockerFlavor, taskId: string) => Promise<DeployedContainer>;
    onPullImageError?: (hostId: string, flavor: DockerFlavor, error: unknown) => void;
    // 这台主机各 flavor 官方镜像是否已拉取（不创建 napcat/snowluma 演示容器）。
    imageReadyByFlavor: Partial<Record<DockerFlavor, boolean | undefined>>;
    // 容器列表（Docker 管理页等；框架按钮不再据此判定）。
    containers: ContainerInfo[];
}

export const HostComponentsView: React.FC<HostComponentsViewProps> = ({
    machine,
    latestVersionFor,
    latestReleaseFor,
    getProgress,
    onAction,
    onRetryDetect,
    onShowReleaseNotes,
    lifecycleBlockedReason = null,
    dockerStatus,
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
    containers: _containers,
}) => {
    const { host } = machine;

    const empty =
        machine.framework.length + machine.runtimeDep.length + machine.selfApp.length === 0;

    const allComponents = [...machine.framework, ...machine.runtimeDep, ...machine.selfApp];

    // Docker 只用于远端 Linux：本机（Windows）不显示 Docker 行，也不在框架行
    // 给「Docker 部署」按钮。
    const dockerApplicable = host.locality === 'remote';

    const dockerReady =
        dockerApplicable && dockerStatus ? dockerStatusSummary(dockerStatus).ready : false;

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
    // 仅展示一个「重试探测」动作；“去远端页测试连接”是静态指引（用户可从侧边栏切换到远端页）。
    const allUnknown = allComponents.every((r) => r.status.state === 'unknown');
    const anyConnectivityFail = allComponents.some(
        (r) => r.status.state === 'unknown' && isHostConnectivityFailureReason((r.status as { state: 'unknown'; reason: string }).reason),
    );
    const hostConnectFailed = host.locality === 'remote' && allUnknown && anyConnectivityFail;

    if (hostConnectFailed) {
        const found = allComponents.find(
            (r) => r.status.state === 'unknown' && isHostConnectivityFailureReason((r.status as { state: 'unknown'; reason: string }).reason),
        );
        const sample = found?.status.state === 'unknown' ? found.status.reason : '连接失败';
        return (
            <div className="flex min-w-0 w-full max-w-full flex-col gap-6">
                <div className="flex min-h-[220px] w-full flex-col items-center justify-center gap-3 rounded-md border border-border-subtle bg-surface/40 px-6 py-10 text-center">
                    <WifiOff size={28} className="text-text-tertiary" strokeWidth={1.5} />
                    <p className="text-sm text-text-secondary">{host.display_name} 主机不可达</p>
                    <p className="max-w-[48ch] text-xs text-text-tertiary break-words">{sample}</p>
                    <p className="text-2xs text-text-tertiary">请在「远端」页手动测试连接，或检查网络/防火墙/SSH 配置。</p>
                    <div className="mt-1">
                        <Button size="sm" variant="primary" onClick={() => onRetryDetect(host.host_id)}>
                            重试探测
                        </Button>
                    </div>
                </div>
            </div>
        );
    }

    // 框架行的尾随「Docker 部署」按钮：仅 docker 就绪 + 该行是 NapCat/SnowLuma 时给。
    const deployButtonFor = (row: MachineComponentRow): React.ReactNode => {
        if (!dockerReady) return null;
        const flavor = frameworkFlavor(row.info.id);
        if (!flavor) return null;
        // 本地已有官方镜像则禁用重复拉取。
        const alreadyDeployed = imageReadyByFlavor[flavor] === true;
        const pulling = isPullingImage(host.host_id, flavor);
        return (
            <FrameworkDockerDeployButton
                flavor={flavor}
                hostId={host.host_id}
                hostLabel={host.display_name}
                isDeploying={pulling}
                alreadyDeployed={alreadyDeployed}
                onPullImage={onPullImage}
                onPullError={(error) => onPullImageError?.(host.host_id, flavor, error)}
            />
        );
    };

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

    return (
        <div className="flex min-w-0 w-full max-w-full flex-col gap-6">
            <div className="flex w-full flex-col gap-3">
                <Group
                    title="框架"
                    description="Bot 框架本体；Docker 就绪时可预拉框架镜像，Bot 启动时再创建容器"
                    rows={machine.framework}
                    hostId={host.host_id}
                    disableActions={false}
                    lifecycleBlockedReason={lifecycleBlockedReason}
                    latestVersionFor={latestVersionFor}
                    latestReleaseFor={latestReleaseFor}
                    getProgress={getProgress}
                    onAction={onAction}
                    onRetryDetect={onRetryDetect}
                    onShowReleaseNotes={onShowReleaseNotes}
                    trailingFor={deployButtonFor}
                />
            </div>

            <RuntimeDepGroup
                rows={machine.runtimeDep}
                hostId={host.host_id}
                os={host.os}
                showDocker={dockerApplicable}
                disableActions={false}
                lifecycleBlockedReason={lifecycleBlockedReason}
                latestVersionFor={latestVersionFor}
                latestReleaseFor={latestReleaseFor}
                getProgress={getProgress}
                onAction={onAction}
                onRetryDetect={onRetryDetect}
                onShowReleaseNotes={onShowReleaseNotes}
                dockerStatus={dockerStatus}
                isDockerProbing={isDockerProbing}
                isInstallingDocker={isInstallingDocker}
                dockerInstallHint={dockerInstallHint}
                dockerInstallProgress={dockerInstallProgress}
                onInstallDocker={() => onInstallDocker(host.host_id)}
                onOpenDockerDownload={onOpenDockerDownload}
                trailingFor={runtimeTrailingFor}
            />

            <Group
                title="桌面端"
                description="NapCatQQ Desktop 本体，以及配套的远端 NCD Watch（Desktop 退出后仍可告警）"
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
    getProgress: (
        componentId: ComponentId,
        hostId: string,
    ) => { taskId: string; progress: ActionProgressView } | null;
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
            <FormSection title={title} description={description} layout="none">
                <div className={componentCardGridClass}>
                    {rows.map((row) => (
                        <MachineComponentRowView
                            key={row.info.id}
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
                    ))}
                </div>
            </FormSection>
        );
    };

/// 运行时依赖组：常规组件行 + 一行 Docker（合成行，状态/动作走 docker hook）。
const RuntimeDepGroup: React.FC<{
    rows: MachineComponentRow[];
    hostId: string;
    os: Os;
    showDocker: boolean;
    disableActions?: boolean;
    lifecycleBlockedReason?: string | null;
    latestVersionFor: (id: ComponentId) => string | null;
    latestReleaseFor: (id: ComponentId) => ReleaseInfoView | null;
    getProgress: (
        componentId: ComponentId,
        hostId: string,
    ) => { taskId: string; progress: ActionProgressView } | null;
    onAction: (
        componentId: ComponentId,
        hostId: string,
        action: { stepKind: StepKind } | { cancelTaskId: string },
    ) => void;
    onRetryDetect: (hostId: string) => void;
    onShowReleaseNotes: (componentId: ComponentId) => void;
    dockerStatus: DockerStatus | undefined;
    isDockerProbing: boolean;
    isInstallingDocker: boolean;
    dockerInstallHint?: string;
    dockerInstallProgress?: ActionProgressView | null;
    onInstallDocker: () => void;
    onOpenDockerDownload: () => void;
    trailingFor?: (row: MachineComponentRow) => React.ReactNode;
}> = ({
    rows,
    hostId,
    os,
    showDocker,
    disableActions = false,
    lifecycleBlockedReason = null,
    latestVersionFor,
    latestReleaseFor,
    getProgress,
    onAction,
    onRetryDetect,
    onShowReleaseNotes,
    dockerStatus,
    isDockerProbing,
    isInstallingDocker,
    dockerInstallHint,
    dockerInstallProgress,
    onInstallDocker,
    onOpenDockerDownload,
    trailingFor,
}) => {
        const hasRows = rows.length > 0 || showDocker;
        if (!hasRows) return null;
        return (
            <FormSection
                title="运行时依赖"
                description="Node.js、QQ 运行时等与框架配套的依赖；远端 Linux 含 Docker"
                layout="none"
            >
                <div className={componentCardGridClass}>
                    {rows.map((row) => (
                        <MachineComponentRowView
                            key={row.info.id}
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
                    ))}
                    {showDocker && (
                        <DockerRow
                            os={os}
                            status={dockerStatus}
                            isProbing={isDockerProbing}
                            isInstalling={isInstallingDocker}
                            installHint={dockerInstallHint}
                            installProgress={dockerInstallProgress}
                            onInstall={onInstallDocker}
                            onOpenDownload={onOpenDockerDownload}
                        />
                    )}
                </div>
            </FormSection>
        );
    };

export default HostComponentsView;
