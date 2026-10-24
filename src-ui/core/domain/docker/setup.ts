// Docker 环境补全动作根据实际缺失项展示。
import type { DockerStatus } from '../../ipc/types';

export function dockerSetupAction(
    status?: DockerStatus,
): { label: string; pendingLabel: string } | null {
    if (!status?.installed) {
        return { label: '安装 Docker', pendingLabel: '正在安装' };
    }
    if (!status.daemonRunning) {
        return { label: '启动 Docker', pendingLabel: '正在启动' };
    }
    if (!status.composeAvailable) {
        return { label: '补全 Compose', pendingLabel: '正在补全' };
    }
    return null;
}
