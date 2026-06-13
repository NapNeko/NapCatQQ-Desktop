// 模块级 docker 部署进度表。
//
// 为什么提到模块级？DeployDialog 打开期间订阅进度，关闭后进度就没人消费了。
// 但后端事件流不管对话框开没开，只要 task 在跑就会推事件。提到模块级后，
// 事件不会因为对话框关闭而丢失，下次打开同一 task 的对话框还能看到历史进度。
//
// 终态保留时长由设置页「任务队列自动清理」配置。
//
// 相比 componentActionStore 简化了 activeByTarget 映射，docker 部署不需要
// 按 (component, host) 查活跃 task，调用方直接持有 taskId。

import { createStore } from '../utils/createStore';
import {
    initialActionProgress,
    reduceActionProgress,
    type ActionProgressView,
} from '../../core/domain/components/progress';
import type { ProgressEvent } from '../../core/ipc/types';
import { scheduleTaskQueueTerminalCleanup } from '../task-queue/taskQueueTerminalLinger';

export interface DockerDeployProgressStoreState {
    tasks: Record<string, ActionProgressView>;
}

const initialState: DockerDeployProgressStoreState = {
    tasks: {},
};

const store = createStore<DockerDeployProgressStoreState>(initialState);

const lingerTimers = new Map<string, ReturnType<typeof setTimeout>>();

export const dockerDeployProgressStore = {
    getSnapshot: store.getSnapshot,

    subscribe: store.subscribe,

    // 部署开始前调用，初始化该 task 的进度为 pending 状态。
    // 若同一 taskId 已存在（理论上不会，但防御一下），直接覆盖。
    started(taskId: string): void {
        const current = store.getSnapshot();
        // 清掉可能残留的 linger 计时器，避免旧计时器到期把新 task 清掉。
        if (lingerTimers.has(taskId)) {
            clearTimeout(lingerTimers.get(taskId)!);
            lingerTimers.delete(taskId);
        }
        store.setState({
            tasks: { ...current.tasks, [taskId]: initialActionProgress },
        });
    },

    // 应用一条来自后端的 ProgressEvent。
    // 收到未注册 task_id 时自动创建空记录（dev 重载 / 其它窗口场景）。
    applyProgress(taskId: string, event: ProgressEvent): void {
        const current = store.getSnapshot();
        const prev = current.tasks[taskId] ?? initialActionProgress;
        const next = reduceActionProgress(prev, event);
        store.setState({
            tasks: { ...current.tasks, [taskId]: next },
        });

        const isTerminal =
            next.status === 'success' ||
            next.status === 'failed' ||
            next.status === 'cancelled';
        if (isTerminal) {
            scheduleTaskQueueTerminalCleanup(taskId, lingerTimers, (id) => {
                const s = store.getSnapshot();
                const cleaned = { ...s.tasks };
                delete cleaned[id];
                store.setState({ tasks: cleaned });
            });
        }
    },

    _reset(): void {
        for (const t of lingerTimers.values()) clearTimeout(t);
        lingerTimers.clear();
        store._reset();
    },
};
