// 框架行「拉镜像」：口味已在按钮上选定；可选手动镜像源。

import React, { useState } from 'react';
import { ChevronDown, Container, Loader2 } from 'lucide-react';
import { Button, Popover, PopoverContent, PopoverTrigger } from '../../shared/ui';
import { MotionIcon } from '../../shared/ui/motion';
import { formatDockerDeploySuccessContent } from '../../core/domain/docker/deployInfoBar';
import { dockerDeployProgressStore } from '../../hooks/docker/dockerDeployProgressStore';
import { dockerActionStore } from '../../hooks/docker/dockerActionStore';
import { taskQueueMetaStore } from '../../hooks/task-queue/taskQueueMetaStore';
import { pushInfoBar } from '../../hooks/ui/globalInfoBarStore';
import type { DeployedContainer, DockerFlavor } from '../../core/ipc/types';

/** 与后端 DockerPullSpec.mirror 对齐 */
export type DockerPullMirrorChoice = 'auto' | 'hub' | 'docker.1ms.run' | 'docker.m.daocloud.io';

const MIRROR_OPTIONS: { id: DockerPullMirrorChoice; label: string; hint: string }[] = [
    { id: 'auto', label: '自动换源', hint: '优先国内站；有进度可拉很久，久无输出才换源' },
    { id: 'docker.1ms.run', label: '毫秒镜像', hint: 'docker.1ms.run' },
    { id: 'docker.m.daocloud.io', label: 'DaoCloud', hint: 'docker.m.daocloud.io' },
    { id: 'hub', label: '仅 Docker Hub', hint: '官方源（国内常很慢）' },
];

interface FrameworkDockerDeployButtonProps {
    flavor: DockerFlavor;
    hostId: string;
    hostLabel?: string;
    isDeploying: boolean;
    alreadyDeployed: boolean;
    onPullImage: (
        hostId: string,
        flavor: DockerFlavor,
        taskId: string,
        mirror?: string | null,
    ) => Promise<DeployedContainer>;
    onPullError?: (error: unknown) => void;
    onPulled?: (result: DeployedContainer) => void;
}

export const FrameworkDockerDeployButton: React.FC<FrameworkDockerDeployButtonProps> = ({
    flavor,
    hostId,
    hostLabel,
    isDeploying,
    alreadyDeployed,
    onPullImage,
    onPullError,
    onPulled,
}) => {
    const [menuOpen, setMenuOpen] = useState(false);
    const frameworkLabel = flavor === 'napcat' ? 'NapCat' : 'SnowLuma';
    const hostCtx = hostLabel?.trim() ? ` · ${hostLabel.trim()}` : '';

    const startPull = (mirror: DockerPullMirrorChoice) => {
        if (isDeploying || alreadyDeployed) return;
        if (dockerActionStore.isPulling(hostId, flavor)) return;
        setMenuOpen(false);
        const id = crypto.randomUUID();
        dockerDeployProgressStore.started(id);
        taskQueueMetaStore.registerDockerDeploy(id, {
            hostId,
            hostLabel,
            flavor,
        });
        dockerActionStore.markPulling(hostId, flavor, id);
        const mirrorArg = mirror === 'auto' ? null : mirror;
        const mirrorHint =
            mirror === 'auto' ? '自动换源' : MIRROR_OPTIONS.find((o) => o.id === mirror)?.label ?? mirror;
        pushInfoBar({
            key: `docker-deploy-start:${id}`,
            tone: 'info',
            title: `${frameworkLabel} 镜像拉取已提交${hostCtx}`,
            content: `源：${mirrorHint}。可在「任务队列」查看进度；拉取中可点「停止」。`,
            autoDismissMs: 6000,
        });
        void onPullImage(hostId, flavor, id, mirrorArg)
            .then((result) => {
                onPulled?.(result);
                pushInfoBar({
                    key: `docker-deploy-ok:${id}`,
                    tone: 'success',
                    title: `${frameworkLabel} 镜像已就绪${hostCtx}`,
                    content: formatDockerDeploySuccessContent(result),
                    autoDismissMs: 0,
                });
            })
            .catch((error) => {
                onPullError?.(error);
            });
    };

    if (alreadyDeployed) {
        return (
            <Button size="sm" variant="ghost" disabled title="该主机已拉取此框架镜像">
                <Container size={13} />
                已拉取
            </Button>
        );
    }

    if (isDeploying) {
        return (
            <Button size="sm" variant="ghost" disabled title="正在拉取镜像">
                <MotionIcon icon={Loader2} motion="spin" playEnter={false} size={13} />
                拉取中
            </Button>
        );
    }

    return (
        <div className="inline-flex items-center">
            <Button
                size="sm"
                variant="ghost"
                className="rounded-r-none border-r border-border-subtle/60 pr-2"
                onClick={() => startPull('auto')}
                title="自动换源拉取（推荐）"
            >
                <Container size={13} />
                拉镜像
            </Button>
            <Popover open={menuOpen} onOpenChange={setMenuOpen} modal={false}>
                <PopoverTrigger asChild>
                    <Button
                        size="sm"
                        variant="ghost"
                        className="rounded-l-none px-1.5"
                        title="选择镜像源"
                        aria-label="选择镜像源"
                    >
                        <ChevronDown size={13} />
                    </Button>
                </PopoverTrigger>
                <PopoverContent align="end" className="w-64 p-1.5" sideOffset={6}>
                    <p className="px-2 py-1.5 text-[11px] font-medium text-text-tertiary">
                        选择镜像源
                    </p>
                    <ul className="flex flex-col gap-0.5">
                        {MIRROR_OPTIONS.map((opt) => (
                            <li key={opt.id}>
                                <button
                                    type="button"
                                    className="flex w-full flex-col items-start rounded-md px-2 py-1.5 text-left hover:bg-surface-hover"
                                    onClick={() => startPull(opt.id)}
                                >
                                    <span className="text-[12.5px] font-medium text-text">
                                        {opt.label}
                                    </span>
                                    <span className="text-[11px] text-text-tertiary">{opt.hint}</span>
                                </button>
                            </li>
                        ))}
                    </ul>
                </PopoverContent>
            </Popover>
        </div>
    );
};

export default FrameworkDockerDeployButton;
