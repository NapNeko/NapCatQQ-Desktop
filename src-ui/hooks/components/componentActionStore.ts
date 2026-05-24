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

import {
    initialActionProgress,
    reduceActionProgress,
    type ActionProgressView,
} from '../../core/domain/components/progress';
import type { ComponentId, ProgressEvent } from '../../core/ipc/types';

export interface ComponentActionStoreState {
    /** task_id → 进度视图 */
    tasks: Record<string, ActionProgressView>;
    /** "<componentId>::<hostId>" → 该 (component, host) 当前活跃的 task_id */
    activeByTarget: Record<string, string>;
}

const initialState: ComponentActionStoreState = {
    tasks: {},
    activeByTarget: {},
};

export function targetKey(componentId: ComponentId, hostId: string): string {
    return `${componentId}::${hostId}`;
}

let state: ComponentActionStoreState = initialState;
const listeners = new Set<() => void>();

function emit(): void {
    for (const fn of listeners) fn();
}

export const componentActionStore = {
    /** 当前快照（同步）。useSyncExternalStore 用。 */
    getSnapshot(): ComponentActionStoreState {
        return state;
    },

    subscribe(listener: () => void): () => void {
        listeners.add(listener);
        return () => listeners.delete(listener);
    },

    /** 启动一个 task：注册到 active 表，初始化进度视图为 pending。 */
    started(taskId: string, componentId: ComponentId, hostId: string): void {
        const key = targetKey(componentId, hostId);
        state = {
            tasks: { ...state.tasks, [taskId]: initialActionProgress },
            activeByTarget: { ...state.activeByTarget, [key]: taskId },
        };
        emit();
    },

    /**
     * 应用一条来自后端的 ProgressEvent。
     * 收到未注册 task_id 时（典型场景：dev 重载、其它窗口、或 started 还没回来）
     * 自动给 task_id 创建一条空记录，让进度照样累计；这种"孤立 task"只更
     * tasks，不更 activeByTarget（因为不知道属于哪个 (component, host)）。
     */
    applyProgress(taskId: string, event: ProgressEvent): void {
        const prev = state.tasks[taskId] ?? initialActionProgress;
        const next = reduceActionProgress(prev, event);
        let activeByTarget = state.activeByTarget;
        if (
            next.status === 'success' ||
            next.status === 'failed' ||
            next.status === 'cancelled'
        ) {
            // 终态：清掉 active 标记，但保留 tasks[taskId] 让 UI 还能读最终状态。
            const cleaned: Record<string, string> = {};
            for (const [k, v] of Object.entries(activeByTarget)) {
                if (v !== taskId) cleaned[k] = v;
            }
            activeByTarget = cleaned;
        }
        state = {
            tasks: { ...state.tasks, [taskId]: next },
            activeByTarget,
        };
        emit();
    },

    /** 测试 / dev 重置用。生产代码不要碰。 */
    _reset(): void {
        state = initialState;
        emit();
    },
};
