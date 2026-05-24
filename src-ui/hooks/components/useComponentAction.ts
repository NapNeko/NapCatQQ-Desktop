// 单个 (component_id, host_id) 安装/更新/卸载操作的 hook。
//
// 职责：
//   1. 维护一张 task_id → ActionProgressView 表
//   2. 订阅 component_action_progress 事件流，按 task_id 分发到对应 reducer
//   3. 暴露 startAction / cancelAction
//
// frontend-layering：调 componentService（service 唯一允许位）+ 订阅事件
// （走 useDomainEvents）。不直接 invoke。

import { useCallback, useReducer, useRef } from 'react';
import { componentService } from '../../core/services/component.service';
import { useDomainEvents } from '../events/useDomainEvents';
import {
    initialActionProgress,
    reduceActionProgress,
    type ActionProgressView,
} from '../../core/domain/components/progress';
import type { ComponentId, StepKind } from '../../core/ipc/types';

interface TaskMapState {
    /// task_id → 进度视图
    tasks: Record<string, ActionProgressView>;
    /// (component_id, host_id) → 当前活跃 task_id（用来在 UI 上判断"该按钮是否处于安装中"）
    activeByTarget: Record<string, string>;
}

const initialMapState: TaskMapState = { tasks: {}, activeByTarget: {} };

function targetKey(componentId: ComponentId, hostId: string): string {
    return `${componentId}::${hostId}`;
}

type Action =
    | {
        type: 'started';
        taskId: string;
        target: { componentId: ComponentId; hostId: string };
    }
    | {
        type: 'progress';
        taskId: string;
        next: ActionProgressView;
    }
    | { type: 'finished'; taskId: string };

function reduceMap(state: TaskMapState, action: Action): TaskMapState {
    switch (action.type) {
        case 'started': {
            const key = targetKey(action.target.componentId, action.target.hostId);
            return {
                tasks: { ...state.tasks, [action.taskId]: initialActionProgress },
                activeByTarget: { ...state.activeByTarget, [key]: action.taskId },
            };
        }
        case 'progress': {
            return {
                ...state,
                tasks: { ...state.tasks, [action.taskId]: action.next },
            };
        }
        case 'finished': {
            // 从 activeByTarget 里清掉对应条目
            const nextActive = { ...state.activeByTarget };
            for (const [k, v] of Object.entries(nextActive)) {
                if (v === action.taskId) delete nextActive[k];
            }
            return { ...state, activeByTarget: nextActive };
        }
    }
}

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
    const [map, dispatch] = useReducer(reduceMap, initialMapState);

    // 用 ref 持有最新 state，避免 useDomainEvents handler 的 ref 闭包陈旧。
    const mapRef = useRef(map);
    mapRef.current = map;

    useDomainEvents((event) => {
        if (event.kind !== 'component_action_progress') return;
        const taskId = event.task_id;
        const prev = mapRef.current.tasks[taskId];
        if (!prev) {
            // 收到一个我们不认识的 task：可能是其它窗口 / dev 重启遗留，忽略
            return;
        }
        const next = reduceActionProgress(prev, event.event);
        dispatch({ type: 'progress', taskId, next });
        if (next.status === 'success' || next.status === 'failed' || next.status === 'cancelled') {
            // 终态：清理 active 标记。但保留 tasks[taskId] 让 UI 可以读最终进度
            // （UI 自行决定何时丢弃，比如关闭对话框时）。
            dispatch({ type: 'finished', taskId });
        }
    });

    const startAction = useCallback(
        async (componentId: ComponentId, hostId: string, kind: StepKind) => {
            const taskId = await componentService.runComponentAction(
                componentId,
                hostId,
                kind,
            );
            dispatch({
                type: 'started',
                taskId,
                target: { componentId, hostId },
            });
            return taskId;
        },
        [],
    );

    const cancelAction = useCallback(async (taskId: string) => {
        await componentService.cancelComponentAction(taskId);
    }, []);

    const getProgressFor = useCallback(
        (componentId: ComponentId, hostId: string) => {
            const taskId = mapRef.current.activeByTarget[targetKey(componentId, hostId)];
            if (!taskId) return null;
            const progress = mapRef.current.tasks[taskId];
            if (!progress) return null;
            return { taskId, progress };
        },
        [],
    );

    return { startAction, cancelAction, getProgressFor };
}
