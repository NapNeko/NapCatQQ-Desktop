// Docker 拉镜像结果 → InfoBar 文案。

import type { DeployedContainer } from '../../ipc/types';

export function formatDockerDeploySuccessContent(result: DeployedContainer): string {
    const lines = [`镜像已就绪：${result.image}`];
    lines.push('实际 Bot 容器在 Bot 页启动时自动创建（ncbot-<QQ号>）。');
    lines.push('可在「任务队列」查看拉取进度与日志。');
    return lines.join('\n');
}