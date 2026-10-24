// Docker 部署入口集中展示运行环境、框架镜像和 Bot 配置入口。
import type { Ref } from 'react';
import { Check, Container, Plus, RefreshCw } from 'lucide-react';
import { Button } from '../../shared/ui';
import { openDockerManagement } from '../../hooks/docker/dockerNavigation';
import { openCreateDockerBot } from '../../hooks/bot/botCreateNavigation';
import { useFeatureEnabled } from '../../hooks/preferences/featureTogglesStore';
import type { ActionProgressView } from '../../core/domain/components/progress';
import type { DeployedContainer, DockerFlavor, DockerStatus, Os } from '../../core/ipc/types';
import { DockerRow } from './DockerRow';
import { FrameworkDockerDeployButton } from './FrameworkDockerDeploy';
import { componentLogoIcon } from './componentLogos';

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
    const managementEnabled = useFeatureEnabled('dockerPage');
    const supported = os === 'linux';
    const canPull =
        supported &&
        probeError == null &&
        !isProbing &&
        !isInstalling &&
        !!status?.installed &&
        status.daemonRunning;
    const canDeploy = canPull && !!status?.composeAvailable;
    const blockedReason = !supported
        ? 'Docker 部署仅支持远端 Linux 主机'
        : isInstalling
          ? '正在准备 Docker 环境'
          : isProbing
            ? '正在更新 Docker 状态'
            : probeError != null
              ? '重新探测 Docker 状态后继续'
              : !status
                ? '等待 Docker 状态'
                : !status.installed
                  ? '先安装 Docker'
                  : !status.daemonRunning
                    ? '先启动 Docker'
                    : !status.composeAvailable
                      ? '补全 Compose 后即可新建 Docker Bot'
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
                        Docker 部署
                    </h3>
                </div>
                <div className="flex items-center gap-1">
                    <Button
                        size="sm"
                        variant="ghost"
                        onClick={onRetry}
                        disabled={isProbing || isInstalling}
                        aria-label="刷新 Docker 部署状态"
                    >
                        <RefreshCw size={13} />
                        刷新状态
                    </Button>
                    {managementEnabled && (
                        <Button
                            size="sm"
                            variant="secondary"
                            onClick={() => openDockerManagement(hostId)}
                        >
                            <Container size={13} />
                            Docker 管理
                        </Button>
                    )}
                </div>
            </div>
            <div className="min-w-0 overflow-hidden rounded-md border border-border-subtle bg-surface shadow-card">
                <div data-tour-id="comp-row-docker" className="min-w-0">
                    <DockerRow
                        os={os}
                        status={status}
                        isProbing={isProbing}
                        probeError={probeError}
                        isInstalling={isInstalling}
                        installHint={installHint}
                        installProgress={installProgress}
                        onInstall={onInstall}
                        onOpenDownload={onOpenDownload}
                        onRetry={onRetry}
                    />
                </div>
                <div
                    className={
                        flavors.length > 1 ? 'grid min-w-0 @min-[48rem]:grid-cols-2' : 'min-w-0'
                    }
                >
                    {flavors.map((flavor) => {
                        const label = flavor === 'napcat' ? 'NapCat' : 'SnowLuma';
                        const imageReady = imageReadyByFlavor[flavor];
                        const pulling = isPullingImage(hostId, flavor);
                        const imageError = imageErrorByFlavor?.[flavor];
                        const imageLabel = pulling
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
                            imageReady != null || pulling || imageError != null || canPull;
                        return (
                            <article
                                key={flavor}
                                aria-label={`${label} Docker 镜像`}
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
