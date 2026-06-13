// Docker 部署结果 → InfoBar 文案（凭据仅展示一次，写在 content 里）。

import type { DeployedContainer } from '../../ipc/types';

export function formatDockerDeploySuccessContent(result: DeployedContainer): string {
    const lines = [`WebUI：${result.webuiUrl}`];
    if (result.novncUrl) lines.push(`noVNC：${result.novncUrl}`);
    if (result.webuiSecret) {
        lines.push(`凭据：${result.webuiSecret}（请记下，仅展示一次）`);
    }
    lines.push('可在「任务队列」查看部署进度与日志。');
    return lines.join('\n');
}