// 任务队列：关闭自动清理时，对三个 progress store 做一次终态条数硬顶修剪。

import { componentActionStore } from '../components/componentActionStore';
import { dockerDeployProgressStore } from '../docker/dockerDeployProgressStore';
import { dockerInstallProgressStore } from '../docker/dockerInstallProgressStore';

/** 偏好为「不自动清理」时，扫掉各 store 里超过硬顶的终态任务（含启动 hydrate 后历史堆积）。 */
export function trimAllProgressStoresWhenAutoCleanupOff(): void {
    componentActionStore.trimTerminalTasksWhenAutoCleanupOff();
    dockerDeployProgressStore.trimTerminalTasksWhenAutoCleanupOff();
    dockerInstallProgressStore.trimTerminalTasksWhenAutoCleanupOff();
}