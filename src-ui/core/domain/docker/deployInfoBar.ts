// Docker 拉镜像结果 → InfoBar 文案。

import type { DeployedContainer } from '../../ipc/types';

export function formatDockerDeploySuccessContent(result: DeployedContainer): string {
    return `镜像已就绪：${result.image}`;
}