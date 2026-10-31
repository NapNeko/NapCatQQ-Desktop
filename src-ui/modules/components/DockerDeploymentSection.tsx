// Docker 部署入口集中展示运行环境、框架镜像和 Bot 配置入口。
import type { Ref } from 'react';
import { Check, Container, Plus, RefreshCw } from 'lucide-react';
import { Button } from '../../shared/ui';
import { openDockerManagement } from '../../hooks/docker/dockerNavigation';
import { openCreateDockerBot } from '../../hooks/bot/botCreateNavigation';
import { useFeatureEnabled } from '../../hooks/preferences/featureTogglesStore';
import type { ActionProgressView } from '../../core/domain/components/progress';
import type { DeployedContainer, DockerFlavor, DockerStatus, Os } from '../../core/ipc/types';
import { ContainerEngineSelect } from './ContainerEngineSelect';
import { DockerRow } from './DockerRow';
import { FrameworkDockerDeployButton } from './FrameworkDockerDeploy';
import { componentLogoIcon } from './componentLogos';
import { dockerManagementSummary } from '../../core/domain/docker/status';
import { useContainerEngine } from '../../hooks/docker/useContainerEngine';
import { pushErrorBar } from '../../hooks/ui/pushErrorBar';
import { errorText } from '../../core/domain/errors';

export interface DockerDeploymentSectionProps {
    hostId: string;
    hostLabel: string;
    os: Os;
    status: DockerStatus | undefined;
    isProbing: boolean;
    probeError?: unknown;
    isInstalling: boolean;
    installHint?: string;
    installProgress?: ActionProgressView | null;
    flavors: DockerFlavor[];
    imageReadyByFlavor: Partial<Record<DockerFlavor, boolean | undefined>>;
    imageErrorByFlavor?: Partial<Record<DockerFlavor, unknown>>;
    isPullingImage: (hostId: string, flavor: DockerFlavor) => boolean;
    onPullImage: (
        hostId: string,
        flavor: DockerFlavor,
        taskId: string,
        mirror?: string | null,
    ) => Promise<DeployedContainer>;
    onPullImageError?: (hostId: string, flavor: DockerFlavor, error: unknown) => void;
    onInstall: () => void;
    onRetry: () => void;
    onOpenDownload: () => void;
    sectionRef?: Ref<HTMLElement>;
}

export function DockerDeploymentSection({
    hostId,
    hostLabel,
    os,
    status,
    isProbing,
    probeError,
    isInstalling,
    installHint,
    installProgress,
    flavors,
    imageReadyByFlavor,
    imageErrorByFlavor,
    isPullingImage,
    onPullImage,
    onPullImageError,
    onInstall,
    onRetry,
    onOpenDownload,
    sectionRef,
}: DockerDeploymentSectionProps) {
    const supported = os === 'linux';
    const engineState = useContainerEngine(hostId, supported);
    const engine = engineState.engine;
    const engineName = engine === 'podman' ? 'Podman' : 'Docker';
    const engineBusy = engineState.loading || engineState.switching;
    const engineError = engineState.error ?? probeError;
    const busyLabel = engineState.switching
        ? '正在切换引擎'
        : engineState.loading
          ? '正在读取引擎设置'
          : undefined;
    const retry = () => {
        void engineState.refetch();
        onRetry();
    };
    const managementEnabled = useFeatureEnabled('dockerPage');
    const pullingForHost = (['napcat', 'snowluma'] as const).some((flavor) =>
        isPullingImage(hostId, flavor),
    );
    const canPull =
        supported &&
        engineError == null &&
        !engineBusy &&
        !isProbing &&
        !isInstalling &&
        !!status?.installed &&
        status.daemonRunning;
    const canDeploy = canPull && !!status?.composeAvailable;
    const blockedReason = !supported
        ? '容器部署仅支持 Linux 主机'
        : engineBusy
          ? busyLabel!
          : isInstalling
            ? `正在准备 ${engineName} 环境`
            : isProbing
              ? `正在更新 ${engineName} 状态`
              : engineError != null
                ? '重新探测引擎状态后继续'
                : !status
                  ? `等待 ${engineName} 状态`
                  : !status.installed
                    ? `先安装 ${engineName}`
                    : !status.daemonRunning
                      ? engine === 'podman'
                          ? '先修复 Podman 运行环境'
                          : '先启动 Docker'
                      : !status.composeAvailable
                        ? '补全 Compose 后即可新建容器 Bot'
                        : null;

    return (
        <section
            ref={sectionRef}
            tabIndex={-1}
            aria-labelledby="docker-deployment-heading"
            data-tour-id="comp-group-docker"
            className="@container min-w-0 scroll-mt-3 rounded-md outline-none focus-visible:ring-2 focus-visible:ring-brand"
        >
            <div className="mb-2.5 flex flex-wrap items-center justify-between gap-2">
                <div className="flex items-center gap-2">
                    <span aria-hidden className="h-3.5 w-1 rounded-full bg-brand/70" />
                    <h3
                        id="docker-deployment-heading"
                        className="text-[13px] font-semibold tracking-tight text-text"
                    >
                        容器部署
                    </h3>
                </div>
                <div className="flex items-center gap-1">
                    <Button
                        size="sm"
                        variant="ghost"
                        onClick={retry}
                        disabled={isProbing || isInstalling || engineBusy}
                        aria-label="刷新容器部署状态"
                    >
                        <RefreshCw size={13} />
                        刷新状态
                    </Button>
                    {managementEnabled && supported && (
                        <Button
                            size="sm"
                            variant="secondary"
                            onClick={() => openDockerManagement(hostId)}
                            disabled={
                                engineBusy ||
                                isInstalling ||
                                !!engineError ||
                                !status ||
                                !dockerManagementSummary(status).ready
                            }
                            title={
                                status &&
                                dockerManagementSummary(status).ready &&
                                !engineBusy &&
                                !isInstalling &&
                                !engineError
                                    ? '管理当前引擎的容器、镜像、网络和数据卷'
                                    : (blockedReason ?? '等待引擎就绪')
                            }
                        >
                            <Container size={13} />
                            容器管理
                        </Button>
                    )}
                </div>
            </div>
            <div className="min-w-0 overflow-hidden rounded-md border border-border-subtle bg-surface shadow-card">
                <div data-tour-id="comp-row-docker" className="min-w-0">
                    <DockerRow
                        engine={engine}
                        engineControl={
                            supported ? (
                                <ContainerEngineSelect
                                    value={engineState.pendingEngine ?? engine}
                                    disabled={
                                        engineBusy || isInstalling || isProbing || pullingForHost
                                    }
                                    onValueChange={(next) =>
                                        void engineState.select(next).catch((error) =>
                                            pushErrorBar({
                                                title: '无法切换容器引擎',
                                                raw: errorText(error),
                                            }),
                                        )
                                    }
                                />
                            ) : undefined
                        }
                        probeLabel={busyLabel}
                        os={os}
                        status={status}
                        isProbing={isProbing || engineBusy}
                        probeError={engineError}
                        isInstalling={isInstalling}
                        installHint={installHint}
                        installProgress={installProgress}
                        onInstall={onInstall}
                        onOpenDownload={onOpenDownload}
                        onRetry={retry}
                    />
                </div>
                <div
                    className={
                        flavors.length > 1 ? 'grid min-w-0 @min-[48rem]:grid-cols-2' : 'min-w-0'
                    }
                >
                    {flavors.map((flavor) => {
                        const label = flavor === 'napcat' ? 'NapCat' : 'SnowLuma';
                        const imageReady = engineBusy ? undefined : imageReadyByFlavor[flavor];
                        const pulling = isPullingImage(hostId, flavor);
                        const imageError = engineBusy ? undefined : imageErrorByFlavor?.[flavor];
                        const imageLabel = engineBusy
                            ? '等待引擎状态'
                            : pulling
                              ? '正在拉取镜像'
                              : imageError != null
                                ? '镜像检查失败'
                                : imageReady === true
                                  ? '镜像已就绪'
                                  : imageReady === false
                                    ? '尚未拉取镜像'
                                    : canPull
                                      ? '正在检测镜像'
                                      : '镜像待检测';
                        const showImageStatus =
                            engineBusy ||
                            imageReady != null ||
                            pulling ||
                            imageError != null ||
                            canPull;
                        return (
                            <article
                                key={flavor}
                                aria-label={`${label} 容器镜像`}
                                className="grid min-h-[4.25rem] min-w-0 grid-cols-1 items-center gap-2 border-t border-border-subtle px-4 py-3 first:border-t-0 @min-[24rem]:grid-cols-[minmax(0,1fr)_auto] @min-[24rem]:gap-3 @min-[48rem]:border-l @min-[48rem]:border-t-0 @min-[48rem]:first:border-l-0"
                            >
                                <div className="flex min-w-0 items-center gap-2.5">
                                    <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-field/60">
                                        {componentLogoIcon(flavor)}
                                    </span>
                                    <div className="flex h-10 min-w-0 flex-col justify-center">
                                        <p className="truncate text-sm font-medium text-text">
                                            {label}
                                        </p>
                                        {showImageStatus ? (
                                            <p
                                                className={`flex min-w-0 items-center gap-1 text-xs ${imageError != null ? 'text-danger' : 'text-text-secondary'}`}
                                            >
                                                {imageReady && !pulling && imageError == null ? (
                                                    <Check
                                                        size={13}
                                                        className="shrink-0 text-success"
                                                    />
                                                ) : null}
                                                <span className="truncate" title={imageLabel}>
                                                    {imageLabel}
                                                </span>
                                            </p>
                                        ) : null}
                                    </div>
                                </div>
                                <div className="flex h-7 shrink-0 items-center justify-end gap-2 whitespace-nowrap">
                                    {canPull && imageError != null && !pulling ? (
                                        <Button size="sm" variant="ghost" onClick={onRetry}>
                                            <RefreshCw size={13} />
                                            重试检测
                                        </Button>
                                    ) : canPull && (imageReady === false || pulling) ? (
                                        <FrameworkDockerDeployButton
                                            flavor={flavor}
                                            hostId={hostId}
                                            hostLabel={hostLabel}
                                            isDeploying={pulling}
                                            alreadyDeployed={imageReady === true}
                                            onPullImage={onPullImage}
                                            onPullError={(error) =>
                                                onPullImageError?.(hostId, flavor, error)
                                            }
                                        />
                                    ) : null}
                                    <span
                                        className="inline-flex shrink-0 rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                                        tabIndex={blockedReason ? 0 : undefined}
                                        title={blockedReason ?? undefined}
                                        aria-describedby={
                                            blockedReason ? 'docker-deployment-blocked' : undefined
                                        }
                                    >
                                        <Button
                                            size="sm"
                                            variant="secondary"
                                            aria-label={`新建 ${label} Bot`}
                                            disabled={!canDeploy || isInstalling}
                                            aria-describedby={
                                                blockedReason
                                                    ? 'docker-deployment-blocked'
                                                    : undefined
                                            }
                                            onClick={() => openCreateDockerBot({ hostId, flavor })}
                                        >
                                            <Plus size={13} />
                                            新建 Bot
                                        </Button>
                                    </span>
                                </div>
                            </article>
                        );
                    })}
                </div>
                {flavors.length === 0 ? (
                    <p className="px-4 py-4 text-xs text-text-tertiary">没有已启用的协议端</p>
                ) : null}
                {blockedReason ? (
                    <p id="docker-deployment-blocked" className="sr-only">
                        {blockedReason}
                    </p>
                ) : null}
            </div>
        </section>
    );
}
