// 框架行（NapCat / SnowLuma）上的「Docker 部署」尾随按钮。
//
// 把"用 Docker 部署这个框架"挂到对应框架自己的行上（而不是页面底部单开一块），
// 符合"在对应卡片上操作"的直觉。仅当这台机器 docker 就绪时才渲染按钮；点了开
// 部署对话框，部署成功的 WebUI/noVNC 地址 + 凭据用结果横幅展示在框架组下方。
//
// taskId 在这里生成：点「Docker 部署」按钮时就生成，传给 DeployDialog 用于
// 订阅进度，同时在 onConfirm 时一并传给 onDeploy。这样对话框打开即可订阅，
// 不需要等 deploy 调用返回。

import React, { useState } from 'react';
import { Container, Loader2 } from 'lucide-react';
import { Button } from '../../shared/ui';
import { MotionIcon } from '../../shared/ui/motion';
import { defaultDeploySpec } from '../../core/domain/docker/spec';
import { dockerDeployProgressStore } from '../../hooks/docker/dockerDeployProgressStore';
import type {
    DeployedContainer,
    DockerDeploySpec,
    DockerFlavor,
} from '../../core/ipc/types';
import { DeployDialog } from '../docker/DeployDialog';

interface FrameworkDockerDeployButtonProps {
    flavor: DockerFlavor;
    hostId: string;
    isDeploying: boolean;
    /// 该 flavor 在这台机器上已有容器。已部署时按钮置「已部署」并禁用，
    /// 避免重复部署撞容器名 / 端口。
    alreadyDeployed: boolean;
    onDeploy: (hostId: string, spec: DockerDeploySpec, taskId: string) => Promise<DeployedContainer>;
    onDeployed: (result: DeployedContainer) => void;
}

export const FrameworkDockerDeployButton: React.FC<FrameworkDockerDeployButtonProps> = ({
    flavor,
    hostId,
    isDeploying,
    alreadyDeployed,
    onDeploy,
    onDeployed,
}) => {
    const [open, setOpen] = useState(false);
    // taskId 在打开对话框时生成，整个部署生命周期内不变。
    // 关闭对话框后清空，下次打开重新生成。
    const [taskId, setTaskId] = useState<string | null>(null);

    const handleOpen = () => {
        const id = crypto.randomUUID();
        // 提前注册到 store，让 DeployDialog 订阅时能拿到 pending 初始状态。
        dockerDeployProgressStore.started(id);
        setTaskId(id);
        setOpen(true);
    };

    const handleClose = () => {
        setOpen(false);
        setTaskId(null);
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
                    isDeploying={isDeploying}
                    taskId={taskId}
                    onClose={handleClose}
                    onConfirm={async (spec) => {
                        // 部署成功:同时把结果推到外层 banner(关弹窗后仍可查凭据)并
                        // 回给 DeployDialog——dialog 拿到后切完成态在弹窗内展示,用户
                        // 点「完成」才走 onClose 关闭。不在这里 handleClose。
                        const result = await onDeploy(hostId, spec, taskId);
                        onDeployed(result);
                        return result;
                    }}
                />
            )}
        </>
    );
};

export default FrameworkDockerDeployButton;
