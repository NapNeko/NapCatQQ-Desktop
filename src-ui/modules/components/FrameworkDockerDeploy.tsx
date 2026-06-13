// 框架行「拉镜像」：口味已在按钮上选定，无需部署对话框。

import React from 'react';
import { Container, Loader2 } from 'lucide-react';
import { Button } from '../../shared/ui';
import { MotionIcon } from '../../shared/ui/motion';
import { formatDockerDeploySuccessContent } from '../../core/domain/docker/deployInfoBar';
import { dockerDeployProgressStore } from '../../hooks/docker/dockerDeployProgressStore';
import { dockerActionStore } from '../../hooks/docker/dockerActionStore';
import { taskQueueMetaStore } from '../../hooks/task-queue/taskQueueMetaStore';
import { pushInfoBar } from '../../hooks/ui/globalInfoBarStore';
import type { DeployedContainer, DockerFlavor } from '../../core/ipc/types';

interface FrameworkDockerDeployButtonProps {
    flavor: DockerFlavor;
    hostId: string;
    hostLabel?: string;
    isDeploying: boolean;
    alreadyDeployed: boolean;
    onPullImage: (hostId: string, flavor: DockerFlavor, taskId: string) => Promise<DeployedContainer>;
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
    const frameworkLabel = flavor === 'napcat' ? 'NapCat' : 'SnowLuma';
    const hostCtx = hostLabel?.trim() ? ` · ${hostLabel.trim()}` : '';

    const handlePull = () => {
        if (isDeploying || alreadyDeployed) return;
        if (dockerActionStore.isPulling(hostId, flavor)) return;
        const id = crypto.randomUUID();
        dockerDeployProgressStore.started(id);
        taskQueueMetaStore.registerDockerDeploy(id, {
            hostId,
            hostLabel,
            flavor,
        });
        dockerActionStore.markPulling(hostId, flavor, id);
        pushInfoBar({
            key: `docker-deploy-start:${id}`,
            tone: 'info',
            title: `${frameworkLabel} 镜像拉取已提交${hostCtx}`,
            content: '正在后台拉取镜像，可在「任务队列」查看进度与日志。Bot 启动时会自动创建容器。',
            autoDismissMs: 6000,
        });
        void onPullImage(hostId, flavor, id)
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

    return (
        <Button
            size="sm"
            variant="ghost"
            disabled={isDeploying || alreadyDeployed}
            onClick={handlePull}
            title={alreadyDeployed ? '该主机已拉取此框架镜像' : isDeploying ? '正在拉取镜像' : undefined}
        >
            {isDeploying ? (
                <MotionIcon icon={Loader2} motion="spin" playEnter={false} size={13} />
            ) : (
                <Container size={13} />
            )}
            {alreadyDeployed ? '已拉取' : isDeploying ? '拉取中' : '拉镜像'}
        </Button>
    );
};

export default FrameworkDockerDeployButton;