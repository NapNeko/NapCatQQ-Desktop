// 框架行（NapCat / SnowLuma）上的「Docker 部署」尾随按钮。
//
// 点开后仅用于填写部署参数；确认后立即关弹窗，任务进任务队列跟踪进度。
// 成功 / 失败走全局 InfoBar，不再锁死对话框或依赖页面内结果横幅。

import React, { useState } from 'react';
import { Container, Loader2 } from 'lucide-react';
import { Button } from '../../shared/ui';
import { MotionIcon } from '../../shared/ui/motion';
import { defaultDeploySpec } from '../../core/domain/docker/spec';
import { formatDockerDeploySuccessContent } from '../../core/domain/docker/deployInfoBar';
import { dockerDeployProgressStore } from '../../hooks/docker/dockerDeployProgressStore';
import { taskQueueMetaStore } from '../../hooks/task-queue/taskQueueMetaStore';
import { pushInfoBar } from '../../hooks/ui/globalInfoBarStore';
import type {
    DeployedContainer,
    DockerDeploySpec,
    DockerFlavor,
} from '../../core/ipc/types';
import { DeployDialog } from '../docker/DeployDialog';

interface FrameworkDockerDeployButtonProps {
    flavor: DockerFlavor;
    hostId: string;
    hostLabel?: string;
    isDeploying: boolean;
    alreadyDeployed: boolean;
    onDeploy: (hostId: string, spec: DockerDeploySpec, taskId: string) => Promise<DeployedContainer>;
    onDeployError?: (error: unknown) => void;
    onDeployed?: (result: DeployedContainer) => void;
}

export const FrameworkDockerDeployButton: React.FC<FrameworkDockerDeployButtonProps> = ({
    flavor,
    hostId,
    hostLabel,
    isDeploying,
    alreadyDeployed,
    onDeploy,
    onDeployError,
    onDeployed,
}) => {
    const [open, setOpen] = useState(false);
    const [taskId, setTaskId] = useState<string | null>(null);

    const frameworkLabel = flavor === 'napcat' ? 'NapCat' : 'SnowLuma';
    const hostCtx = hostLabel?.trim() ? ` · ${hostLabel.trim()}` : '';

    const handleOpen = () => {
        const id = crypto.randomUUID();
        const spec = defaultDeploySpec(flavor);
        dockerDeployProgressStore.started(id);
        taskQueueMetaStore.registerDockerDeploy(id, {
            hostId,
            hostLabel,
            container: spec.containerName,
            flavor,
        });
        setTaskId(id);
        setOpen(true);
    };

    const handleClose = () => {
        setOpen(false);
        setTaskId(null);
    };

    const handleConfirm = (spec: DockerDeploySpec) => {
        const id = taskId;
        if (!id) return;
        handleClose();
        pushInfoBar({
            key: `docker-deploy-start:${id}`,
            tone: 'info',
            title: `${frameworkLabel} 容器部署已提交${hostCtx}`,
            content: '正在后台部署，可在「任务队列」查看进度与日志。',
            autoDismissMs: 6000,
        });
        void onDeploy(hostId, spec, id)
            .then((result) => {
                onDeployed?.(result);
                pushInfoBar({
                    key: `docker-deploy-ok:${id}`,
                    tone: 'success',
                    title: `${result.name} 部署完成${hostCtx}`,
                    content: formatDockerDeploySuccessContent(result),
                    autoDismissMs: 0,
                });
            })
            .catch((error) => {
                onDeployError?.(error);
            });
    };

    return (
        <>
            <Button
                size="sm"
                variant="ghost"
                disabled={isDeploying || alreadyDeployed}
                onClick={handleOpen}
                title={alreadyDeployed ? '这台机器上已部署该容器，去 Docker 页管理' : undefined}
            >
                {isDeploying ? (
                    <MotionIcon icon={Loader2} motion="spin" playEnter={false} size={13} />
                ) : (
                    <Container size={13} />
                )}
                {alreadyDeployed ? '已部署' : 'Docker 部署'}
            </Button>
            {open && taskId && (
                <DeployDialog
                    flavor={flavor}
                    initialSpec={defaultDeploySpec(flavor)}
                    isDeploying={false}
                    taskId={taskId}
                    onClose={handleClose}
                    onConfirm={handleConfirm}
                />
            )}
        </>
    );
};

export default FrameworkDockerDeployButton;