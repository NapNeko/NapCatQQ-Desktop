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
    onDeploy: (hostId: string, spec: DockerDeploySpec, taskId: string) => Promise<DeployedContainer>;
    onDeployed: (result: DeployedContainer) => void;
}

export const FrameworkDockerDeployButton: React.FC<FrameworkDockerDeployButtonProps> = ({
    flavor,
    hostId,
    isDeploying,
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
            <Button size="sm" variant="ghost" disabled={isDeploying} onClick={handleOpen}>
                {isDeploying ? (
                    <Loader2 size={13} className="animate-spin" />
                ) : (
                    <Container size={13} />
                )}
                Docker 部署
            </Button>
            {open && taskId && (
                <DeployDialog
                    flavor={flavor}
                    initialSpec={defaultDeploySpec(flavor)}
                    isDeploying={isDeploying}
                    taskId={taskId}
                    onClose={handleClose}
                    onConfirm={async (spec) => {
                        const result = await onDeploy(hostId, spec, taskId);
                        onDeployed(result);
                        handleClose();
                    }}
                />
            )}
        </>
    );
};

export default FrameworkDockerDeployButton;
