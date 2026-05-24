// 单个 (component_id, host_id) 安装 / 更新 / 卸载操作的 hook。
//
// 状态读自模块级 componentActionStore（生命周期 = 应用窗口）；事件订阅由
// AppNext 顶层 useComponentActionEventBridge 一次性挂上。所以本页路由切走
// 再切回来，已有 task 的进度视图不会丢。
//
// frontend-layering：调 componentService（service 唯一允许位）+ 读 store。
// 不直接 invoke / listen。

import { useCallback, useSyncExternalStore } from 'react';
import { componentService } from '../../core/services/component.service';
import {
    componentActionStore,
    targetKey,
} from './componentActionStore';
import type { ActionProgressView } from '../../core/domain/components/progress';
import type { ComponentId, StepKind } from '../../core/ipc/types';

export interface UseComponentActionResult {
    /** 启动一次操作，返回 task_id。 */
    startAction: (
        componentId: ComponentId,
        hostId: string,
        kind: StepKind,
    ) => Promise<string>;
    /** 取消进行中的 task。 */
    cancelAction: (taskId: string) => Promise<void>;
    /** 拿某 (component, host) 当前活跃 task 的进度（无活跃返回 null）。 */
    getProgressFor: (
        componentId: ComponentId,
        hostId: string,
    ) => { taskId: string; progress: ActionProgressView } | null;
}

export function useComponentAction(): UseComponentActionResult {
    // 订阅模块级 store；store 一变就重新 render。
    const state = useSyncExternalStore(
        componentActionStore.subscribe,
        componentActionStore.getSnapshot,
        componentActionStore.getSnapshot,
    );

    const startAction = useCallback(
        async (componentId: ComponentId, hostId: string, kind: StepKind) => {
            const taskId = await componentService.runComponentAction(
                componentId,
                hostId,
                kind,
            );
            componentActionStore.started(taskId, componentId, hostId);
            return taskId;
        },
        [],
    );

    const cancelAction = useCallback(async (taskId: string) => {
        await componentService.cancelComponentAction(taskId);
    }, []);

    const getProgressFor = useCallback(
        (componentId: ComponentId, hostId: string) => {
            const taskId = state.activeByTarget[targetKey(componentId, hostId)];
            if (!taskId) return null;
            const progress = state.tasks[taskId];
            if (!progress) return null;
            return { taskId, progress };
        },
        [state],
    );

    return { startAction, cancelAction, getProgressFor };
}
