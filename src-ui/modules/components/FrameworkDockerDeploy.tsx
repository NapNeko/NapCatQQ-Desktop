// 框架行（NapCat / SnowLuma）上的「Docker 部署」尾随按钮。
//
// 把"用 Docker 部署这个框架"挂到对应框架自己的行上（而不是页面底部单开一块），
// 符合"在对应卡片上操作"的直觉。仅当这台机器 docker 就绪时才渲染按钮；点了开
// 部署对话框，部署成功的 WebUI/noVNC 地址 + 凭据用结果横幅展示在框架组下方。

import React, { useState } from 'react';
import { Container, Loader2 } from 'lucide-react';
import { Button } from '../../shared/ui';
import { defaultDeploySpec } from '../../core/domain/docker/spec';
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
    onDeploy: (hostId: string, spec: DockerDeploySpec) => Promise<DeployedContainer>;
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

    return (
        <>
            <Button size="sm" variant="ghost" disabled={isDeploying} onClick={() => setOpen(true)}>
                {isDeploying ? (
                    <Loader2 size={13} className="animate-spin" />
                ) : (
                    <Container size={13} />
                )}
                Docker 部署
            </Button>
            {open && (
                <DeployDialog
                    flavor={flavor}
                    initialSpec={defaultDeploySpec(flavor)}
                    isDeploying={isDeploying}
                    onClose={() => setOpen(false)}
                    onConfirm={async (spec) => {
                        const result = await onDeploy(hostId, spec);
                        onDeployed(result);
                        setOpen(false);
                    }}
                />
            )}
        </>
    );
};

export default FrameworkDockerDeployButton;
