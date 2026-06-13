// 模块级 component-action 任务表 + 订阅器。
//
// 为什么要提到模块级而不是放在 useComponentAction 的 useReducer 里？
//   - Components 页路由一切走、`ComponentsPageNext` 卸载，组件级 state 直接清空，
//     useDomainEvents 也跟着 unsubscribe；后端真实的安装 task 还在跑但前端已经
//     "忘了它"。回到该页时 getProgressFor 返回 null，进度条凭空消失。
//   - 提到模块级以后，状态生命周期对齐"应用窗口生命周期"而非"页面挂载周期"，
//     切换路由不再丢任何活跃 task。
//
// 订阅入口由 AppNext 顶层挂一次（见 useComponentActionEventBridge），路由切换
// 不会断流。Components 页里的 useComponentAction 只是个 selector + dispatcher。
//
// 终态保留：success / failed / cancelled 收到时不立刻清 activeByTarget，
// 保留时长由设置页「任务队列自动清理」配置；关闭自动清理则终态条目一直保留。

import { createStore } from '../utils/createStore';
import {
    initialActionProgress,
    reduceActionProgress,
    type ActionProgressView,
} from '../../core/domain/components/progress';
import type { ComponentId, ProgressEvent } from '../../core/ipc/types';
import { scheduleTaskQueueTerminalCleanup, trimTerminalTasksWhenAutoCleanupOff } from '../task-queue/taskQueueTerminalLinger';

export interface ComponentActionStoreState {
    /** task_id → 进度视图 */
    tasks: Record<string, ActionProgressView>;
    /** "<componentId>::<hostId>" → 该 (component, host) 当前活跃的 task_id */
    activeByTarget: Record<string, string>;
    /**
     * task_id → 它属于的 (componentId, hostId)。
     * 终态后 activeByTarget 会被 linger 计时器清掉，但 InfoBar 这类场景需要在
     * 终态发生那一刻立刻知道这个 task 是哪个组件 / 哪台主机的，否则 banner
     * 标题就只能写"安装失败"四个字、丢失上下文。所以单独再开一张表，活到
     * task 整个被遗忘为止。
     */
    taskTargets: Record<string, { componentId: ComponentId; hostId: string }>;
}

const initialState: ComponentActionStoreState = {
    tasks: {},
    activeByTarget: {},
    taskTargets: {},
};

const store = createStore<ComponentActionStoreState>(initialState);

const lingerTimers = new Map<string, ReturnType<typeof setTimeout>>();

export function targetKey(componentId: ComponentId, hostId: string): string {
    return `${componentId}::${hostId}`;
}

function clearActiveForTask(taskId: string): void {
    const current = store.getSnapshot();
    const cleanedActive: Record<string, string> = {};
    for (const [k, v] of Object.entries(current.activeByTarget)) {
        if (v !== taskId) cleanedActive[k] = v;
    }
    // taskTargets 保留，让任务队列能显示完整标题。
    // 会在后续清理 tasks 时一并删除。
    const cleanedTasks: Record<string, ActionProgressView> = {};
    const cleanedTargets: Record<string, { componentId: ComponentId; hostId: string }> = {};
    for (const [k, v] of Object.entries(current.tasks)) {
        if (k !== taskId) {
            cleanedTasks[k] = v;
            if (current.taskTargets[k]) cleanedTargets[k] = current.taskTargets[k];
        }
    }
    store.setState({
        ...current,
        activeByTarget: cleanedActive,
        tasks: cleanedTasks,
        taskTargets: cleanedTargets
    });
}

function isTerminal(progress: ActionProgressView): boolean {
    return (
        progress.status === 'success' ||
        progress.status === 'failed' ||
        progress.status === 'cancelled'
    );
}

function scheduleTerminalCleanup(taskId: string): void {
    scheduleTaskQueueTerminalCleanup(taskId, lingerTimers, clearActiveForTask);
}

function maybeTrimTerminalTasksWhenAutoOff(): void {
    const current = store.getSnapshot();
    const { tasks: trimmedTasks, removedIds } = trimTerminalTasksWhenAutoCleanupOff(
        current.tasks,
        isTerminal,
    );
    if (removedIds.length === 0) return;
    for (const id of removedIds) {
        if (lingerTimers.has(id)) {
            clearTimeout(lingerTimers.get(id)!);
            lingerTimers.delete(id);
        }
    }
    const cleanedTargets = { ...current.taskTargets };
    const cleanedActive = { ...current.activeByTarget };
    for (const id of removedIds) {
        delete cleanedTargets[id];
        for (const [k, v] of Object.entries(cleanedActive)) {
            if (v === id) delete cleanedActive[k];
        }
    }
    store.setState({
        ...current,
        tasks: trimmedTasks,
        taskTargets: cleanedTargets,
        activeByTarget: cleanedActive,
    });
}

function onTaskReachedTerminal(taskId: string): void {
    scheduleTerminalCleanup(taskId);
    maybeTrimTerminalTasksWhenAutoOff();
}

export const componentActionStore = {
    /** 当前快照（同步）。useSyncExternalStore 用。 */
    getSnapshot: store.getSnapshot,

    subscribe: store.subscribe,

    trimTerminalTasksWhenAutoCleanupOff: maybeTrimTerminalTasksWhenAutoOff,

    /**
     * 把 task 绑定到组件主机目标。ProgressEvent 可能比 runComponentAction 返回更早到，
     * 所以这里只补 target 映射，不重置已有进度，尤其不能把终态覆盖回 pending。
     */
    registerTarget(taskId: string, componentId: ComponentId, hostId: string): void {
        const current = store.getSnapshot();
        const key = targetKey(componentId, hostId);
        // 同 (component, host) 上一次的 linger 计时器要先清掉，否则新任务起来
        // 几秒后旧计时器到期把新任务的 active 标记给清了。
        const prevTaskId = current.activeByTarget[key];
        if (prevTaskId && prevTaskId !== taskId && lingerTimers.has(prevTaskId)) {
            clearTimeout(lingerTimers.get(prevTaskId)!);
            lingerTimers.delete(prevTaskId);
        }
        const progress = current.tasks[taskId] ?? initialActionProgress;
        store.setState({
            tasks: { ...current.tasks, [taskId]: progress },
            activeByTarget: { ...current.activeByTarget, [key]: taskId },
            taskTargets: { ...current.taskTargets, [taskId]: { componentId, hostId } },
        });
        if (isTerminal(progress)) onTaskReachedTerminal(taskId);
    },

    /** 启动一个 task：注册到 active 表，初始化进度视图为 pending。 */
    started(
        taskId: string,
        componentId: ComponentId,
        hostId: string,
        queueMessage?: string,
    ): void {
        this.registerTarget(taskId, componentId, hostId);
        if (!queueMessage?.trim()) return;
        const current = store.getSnapshot();
        const prev = current.tasks[taskId] ?? initialActionProgress;
        store.setState({
            ...current,
            tasks: {
                ...current.tasks,
                [taskId]: { ...prev, message: queueMessage.trim() },
            },
        });
    },

    /** invoke 失败时把任务标为 failed，避免队列里一直 pending。 */
    failTask(taskId: string, err: unknown): void {
        const msg =
            err instanceof Error
                ? err.message
                : typeof err === 'string'
                  ? err
                  : '组件操作启动失败';
        const current = store.getSnapshot();
        const prev = current.tasks[taskId] ?? initialActionProgress;
        const next: ActionProgressView = {
            ...prev,
            status: 'failed',
            message: msg,
            logs: [
                ...prev.logs,
                {
                    level: 'error',
                    message: msg,
                    timestamp_ms: Date.now(),
                },
            ],
        };
        store.setState({
            ...current,
            tasks: { ...current.tasks, [taskId]: next },
        });
        onTaskReachedTerminal(taskId);
    },

    /**
     * 应用一条来自后端的 ProgressEvent。
     * 收到未注册 task_id 时（典型场景：dev 重载、其它窗口、或 started 还没回来）
     * 自动给 task_id 创建一条空记录，让进度照样累计；这种"孤立 task"只更
     * tasks，不更 activeByTarget（因为不知道属于哪个 (component, host)）。
     *
     * 终态：按设置保留一段时间后从 activeByTarget / tasks 移除；关闭自动清理则不移除。
     */
    applyProgress(taskId: string, event: ProgressEvent): void {
        const current = store.getSnapshot();
        const prev = current.tasks[taskId] ?? initialActionProgress;
        const next = reduceActionProgress(prev, event);
        store.setState({
            ...current,
            tasks: { ...current.tasks, [taskId]: next },
        });

        const isNextTerminal = isTerminal(next);
        if (isNextTerminal) onTaskReachedTerminal(taskId);
    },

    /** 测试 / dev 重置用。生产代码不要碰。 */
    _reset(): void {
        for (const t of lingerTimers.values()) clearTimeout(t);
        lingerTimers.clear();
        store._reset();
    },
};
