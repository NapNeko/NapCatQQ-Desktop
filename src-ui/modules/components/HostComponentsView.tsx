// 单台主机的组件视图：组件页选中某台机器后，这里直接铺这台机器上能装的
// 所有组件，按"框架 / 运行时依赖 / 桌面端"分组，组件行用响应式网格排。
//
// Docker 在这里被当成正常的运行时依赖：
//   - 运行时依赖组里有一行 Docker（状态来自探测，安装走 docker hook）。
//   - 框架行（NapCat / SnowLuma）在 docker 就绪时各带一个「Docker 部署」按钮，
//     点了开部署对话框 —— 部署形态归到对应框架自己的行上，不再是页面底部
//     单开一块。

import React, { useState } from 'react';
import { PackageX } from 'lucide-react';
import { MachineComponentRowView } from './MachineComponentRow';
import { DockerRow } from './DockerRow';
import { FrameworkDockerDeployButton } from './FrameworkDockerDeploy';
import { DeployResultBanner } from '../docker/DeployResultBanner';
import { dockerStatusSummary } from '../../core/domain/docker/status';
import type { MachineView, MachineComponentRow } from '../../core/domain/components/types';
import type { ActionProgressView } from '../../core/domain/components/progress';
import type {
    ComponentId,
    DeployedContainer,
    DockerDeploySpec,
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
    onInstallDocker: (hostId: string) => void;
    onOpenDockerDownload: () => void;
    isDeploying: boolean;
    onDeploy: (hostId: string, spec: DockerDeploySpec, taskId: string) => Promise<DeployedContainer>;
}

export const HostComponentsView: React.FC<HostComponentsViewProps> = ({
    machine,
    latestVersionFor,
    getProgress,
    onAction,
    onRetryDetect,
    dockerStatus,
    isDockerProbing,
    isInstallingDocker,
    onInstallDocker,
    onOpenDockerDownload,
    isDeploying,
    onDeploy,
}) => {
    const { host } = machine;
    const empty =
        machine.framework.length + machine.runtimeDep.length + machine.selfApp.length === 0;

    // Docker 只用于远端 Linux：本机（Windows）不显示 Docker 行，也不在框架行
    // 给「Docker 部署」按钮。
    const dockerApplicable = host.locality === 'remote';

    // 部署结果横幅（WebUI/noVNC 地址 + 凭据）。部署完挂在框架组下方，用户手动关。
    const [deployResults, setDeployResults] = useState<DeployedContainer[]>([]);
    const pushResult = (r: DeployedContainer) =>
        setDeployResults((prev) => [...prev.filter((x) => x.name !== r.name), r]);
    const dismissResult = (name: string) =>
        setDeployResults((prev) => prev.filter((x) => x.name !== name));

    const dockerReady =
        dockerApplicable && dockerStatus ? dockerStatusSummary(dockerStatus).ready : false;

    if (empty) {
        return (
            <div className="flex flex-col items-center justify-center gap-2 rounded-md bg-inset/30 px-6 py-12 text-center">
                <PackageX size={28} className="text-text-tertiary" />
                <p className="text-sm text-text-secondary">这台机器上没有可管理的组件</p>
                <p className="text-xs text-text-tertiary">
                    {host.os} · {host.locality === 'remote' ? '远端' : '本机'} 不支持任何已知组件
                </p>
            </div>
        );
    }

    // 框架行的尾随「Docker 部署」按钮：仅 docker 就绪 + 该行是 NapCat/SnowLuma 时给。
    const deployButtonFor = (componentId: ComponentId): React.ReactNode => {
        if (!dockerReady) return null;
        const flavor = frameworkFlavor(componentId);
        if (!flavor) return null;
        return (
            <FrameworkDockerDeployButton
                flavor={flavor}
                hostId={host.host_id}
                isDeploying={isDeploying}
                onDeploy={onDeploy}
                onDeployed={pushResult}
            />
        );
    };

    return (
        <div className="flex flex-col gap-5">
            <section className="flex flex-col gap-2">
                <Group
                    title="框架"
                    rows={machine.framework}
                    hostId={host.host_id}
                    latestVersionFor={latestVersionFor}
                    getProgress={getProgress}
                    onAction={onAction}
                    onRetryDetect={onRetryDetect}
                    trailingFor={deployButtonFor}
                />
                {deployResults.map((r) => (
                    <DeployResultBanner
                        key={r.name}
                        result={r}
                        onDismiss={() => dismissResult(r.name)}
                    />
                ))}
            </section>

            <RuntimeDepGroup
                rows={machine.runtimeDep}
                hostId={host.host_id}
                os={host.os}
                showDocker={dockerApplicable}
                latestVersionFor={latestVersionFor}
                getProgress={getProgress}
                onAction={onAction}
                onRetryDetect={onRetryDetect}
                dockerStatus={dockerStatus}
                isDockerProbing={isDockerProbing}
                isInstallingDocker={isInstallingDocker}
                onInstallDocker={() => onInstallDocker(host.host_id)}
                onOpenDockerDownload={onOpenDockerDownload}
            />

            <Group
                title="桌面端"
                rows={machine.selfApp}
                hostId={host.host_id}
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

const gridStyle: React.CSSProperties = {
    // 两列大卡：每张是有分量的实体卡（实底 + 边框 + 阴影），不再是挤成一团的
    // 小条。窄屏自动塌成单列。
    gridTemplateColumns: 'repeat(auto-fill, minmax(min(360px, 100%), 1fr))',
};

const Group: React.FC<{
    title: string;
    rows: MachineComponentRow[];
    hostId: string;
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
}> = ({ title, rows, hostId, latestVersionFor, getProgress, onAction, onRetryDetect, trailingFor }) => {
    if (rows.length === 0) return null;
    return (
        <section className="flex flex-col gap-2">
            <p className="text-2xs uppercase tracking-widest text-text-tertiary">{title}</p>
            <div className="grid gap-2" style={gridStyle}>
                {rows.map((row) => (
                    <MachineComponentRowView
                        key={row.info.id}
                        row={row}
                        hostId={hostId}
                        latestRemoteVersion={latestVersionFor(row.info.id)}
                        activeProgress={getProgress(row.info.id, hostId)}
                        onAction={(action) => onAction(row.info.id, hostId, action)}
                        onRetryDetect={() => onRetryDetect(hostId)}
                        trailingActions={trailingFor?.(row.info.id)}
                    />
                ))}
            </div>
        </section>
    );
};

/// 运行时依赖组：常规组件行 + 一行 Docker（合成行，状态/动作走 docker hook）。
const RuntimeDepGroup: React.FC<{
    rows: MachineComponentRow[];
    hostId: string;
    os: Os;
    showDocker: boolean;
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
    onInstallDocker: () => void;
    onOpenDockerDownload: () => void;
}> = ({
    rows,
    hostId,
    os,
    showDocker,
    latestVersionFor,
    getProgress,
    onAction,
    onRetryDetect,
    dockerStatus,
    isDockerProbing,
    isInstallingDocker,
    onInstallDocker,
    onOpenDockerDownload,
}) => (
        <section className="flex flex-col gap-2">
            <p className="text-2xs uppercase tracking-widest text-text-tertiary">运行时依赖</p>
            <div className="grid gap-2" style={gridStyle}>
                {rows.map((row) => (
                    <MachineComponentRowView
                        key={row.info.id}
                        row={row}
                        hostId={hostId}
                        latestRemoteVersion={latestVersionFor(row.info.id)}
                        activeProgress={getProgress(row.info.id, hostId)}
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
                        onInstall={onInstallDocker}
                        onOpenDownload={onOpenDockerDownload}
                    />
                )}
            </div>
        </section>
    );

export default HostComponentsView;
