// 单台主机的组件视图：按「框架 / 运行时依赖 / 桌面端」分组；组内 md 起双列网格满宽。
//
// Docker 在这里被当成正常的运行时依赖：
//   - 运行时依赖组里有一行 Docker（状态来自探测，安装走 docker hook）。
//   - 框架行（NapCat / SnowLuma）在 docker 就绪时各带一个「拉镜像」按钮，
//     仅预拉框架镜像；Bot 容器在 Bot 页启动时创建。

import React from 'react';
import { PackageX } from 'lucide-react';
import { FormSection } from '../../shared/ui';
import { PagePlaceholder } from '../../shared/ui/PagePlaceholder';
import { MachineComponentRowView } from './MachineComponentRow';
import { componentCardGridClass } from './ComponentEntityCard';
import { DockerRow } from './DockerRow';
import { FrameworkDockerDeployButton } from './FrameworkDockerDeploy';
import { dockerStatusSummary } from '../../core/domain/docker/status';
import type { MachineView, MachineComponentRow } from '../../core/domain/components/types';
import type { ActionProgressView } from '../../core/domain/components/progress';
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
    getProgress: (
        componentId: ComponentId,
        hostId: string,
    ) => { taskId: string; progress: ActionProgressView } | null;
    isInstalling: (componentId: ComponentId, hostId: string) => boolean;
    onAction: (
        componentId: ComponentId,
        hostId: string,
        action: { stepKind: StepKind } | { cancelTaskId: string },
    ) => void;
    onRetryDetect: (hostId: string) => void;
    // Docker 数据 + 动作（来自 useDockerHosts）
    dockerStatus: DockerStatus | undefined;
    isDockerProbing: boolean;
    isInstallingDocker: boolean;
    dockerInstallHint?: string;
    dockerInstallProgress?: ActionProgressView | null;
    onInstallDocker: (hostId: string) => void;
    onOpenDockerDownload: () => void;
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
    getProgress,
    isInstalling,
    onAction,
    onRetryDetect,
    dockerStatus,
    isDockerProbing,
    isInstallingDocker,
    dockerInstallHint,
    dockerInstallProgress,
    onInstallDocker,
    onOpenDockerDownload,
    isPullingImage,
    onPullImage,
    onPullImageError,
    imageReadyByFlavor,
    containers: _containers,
}) => {
    const { host } = machine;
    const empty =
        machine.framework.length + machine.runtimeDep.length + machine.selfApp.length === 0;

    // 检查当前主机是否有任何组件正在安装
    const allComponents = [...machine.framework, ...machine.runtimeDep, ...machine.selfApp];
    const isAnyInstalling = allComponents.some((row) => isInstalling(row.info.id, host.host_id));

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

    // 框架行的尾随「Docker 部署」按钮：仅 docker 就绪 + 该行是 NapCat/SnowLuma 时给。
    const deployButtonFor = (componentId: ComponentId): React.ReactNode => {
        if (!dockerReady) return null;
        const flavor = frameworkFlavor(componentId);
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

    return (
        <div className="flex min-w-0 w-full max-w-full flex-col gap-6">
            <div className="flex w-full flex-col gap-3">
                <Group
                    title="框架"
                    description="Bot 框架本体；Docker 就绪时可预拉框架镜像，Bot 启动时再创建容器"
                    rows={machine.framework}
                    hostId={host.host_id}
                    isAnyInstalling={isAnyInstalling}
                    latestVersionFor={latestVersionFor}
                    getProgress={getProgress}
                    onAction={onAction}
                    onRetryDetect={onRetryDetect}
                    trailingFor={deployButtonFor}
                />
            </div>

            <RuntimeDepGroup
                rows={machine.runtimeDep}
                hostId={host.host_id}
                os={host.os}
                showDocker={dockerApplicable}
                isAnyInstalling={isAnyInstalling}
                latestVersionFor={latestVersionFor}
                getProgress={getProgress}
                onAction={onAction}
                onRetryDetect={onRetryDetect}
                dockerStatus={dockerStatus}
                isDockerProbing={isDockerProbing}
                isInstallingDocker={isInstallingDocker}
                dockerInstallHint={dockerInstallHint}
                dockerInstallProgress={dockerInstallProgress}
                onInstallDocker={() => onInstallDocker(host.host_id)}
                onOpenDockerDownload={onOpenDockerDownload}
            />

            <Group
                title="桌面端"
                description="NapCatQQ Desktop 本体更新与维护"
                rows={machine.selfApp}
                hostId={host.host_id}
                isAnyInstalling={isAnyInstalling}
                latestVersionFor={latestVersionFor}
                getProgress={getProgress}
                onAction={onAction}
                onRetryDetect={onRetryDetect}
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
    isAnyInstalling: boolean;
    latestVersionFor: (id: ComponentId) => string | null;
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
    trailingFor?: (componentId: ComponentId) => React.ReactNode;
}> = ({
    title,
    description,
    rows,
    hostId,
    isAnyInstalling,
    latestVersionFor,
    getProgress,
    onAction,
    onRetryDetect,
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
                        activeProgress={getProgress(row.info.id, hostId)}
                        isAnyInstalling={isAnyInstalling}
                        onAction={(action) => onAction(row.info.id, hostId, action)}
                        onRetryDetect={() => onRetryDetect(hostId)}
                        trailingActions={trailingFor?.(row.info.id)}
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
    isAnyInstalling: boolean;
    latestVersionFor: (id: ComponentId) => string | null;
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
    dockerStatus: DockerStatus | undefined;
    isDockerProbing: boolean;
    isInstallingDocker: boolean;
    dockerInstallHint?: string;
    dockerInstallProgress?: ActionProgressView | null;
    onInstallDocker: () => void;
    onOpenDockerDownload: () => void;
}> = ({
    rows,
    hostId,
    os,
    showDocker,
    isAnyInstalling,
    latestVersionFor,
    getProgress,
    onAction,
    onRetryDetect,
    dockerStatus,
    isDockerProbing,
    isInstallingDocker,
    dockerInstallHint,
    dockerInstallProgress,
    onInstallDocker,
    onOpenDockerDownload,
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
                        activeProgress={getProgress(row.info.id, hostId)}
                        isAnyInstalling={isAnyInstalling}
                        onAction={(action) => onAction(row.info.id, hostId, action)}
                        onRetryDetect={() => onRetryDetect(hostId)}
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
