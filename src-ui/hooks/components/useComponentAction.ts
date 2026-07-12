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
import { desktopUpdateService } from '../../core/services/desktop-update.service';
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
    /** 检查某 (component, host) 是否有进行中的任务。 */
    isInstalling: (componentId: ComponentId, hostId: string) => boolean;
    /**
     * 当指定 task 进入终态（success / failed / cancelled）时回调一次。
     * 返回 unsubscribe；如果 task 已经在终态会立刻 fire 一次。
     *
     * 用途：操作发起方拿到 taskId 后挂上 listener，等终态再 refetch detect，
     * 比 setTimeout 估个延迟更靠谱（NapCat 装包要几十秒，500ms refetch 拿到的
     * 还是未安装；refetch 必须等真正完成）。
     */
    onTaskTerminal: (
        taskId: string,
        cb: (status: 'success' | 'failed' | 'cancelled') => void,
    ) => () => void;
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
            const taskId = crypto.randomUUID();
            const needsPkgQueue =
                (componentId === 'novnc' &&
                    (kind === 'ensure_installed' || kind === 'force_install')) ||
                (componentId === 'qq' && kind === 'ensure_dependencies');
            const queueHint = needsPkgQueue
                ? '排队等待包管理器（Docker 等 apt 任务完成后自动开始）…'
                : undefined;
            componentActionStore.started(taskId, componentId, hostId, queueHint);
            try {
                // Desktop 自身更新走 ncd-update MSI 路径，不走 DeployPlan/Component::update
                if (componentId === 'desktop_self' && kind === 'update') {
                    if (hostId !== 'local') {
                        throw new Error('Desktop 自更新仅支持本机');
                    }
                    const available = await desktopUpdateService.check();
                    if (!available) {
                        throw new Error('当前已是最新版本，无需更新');
                    }
                    // 进度经 component_action_progress；成功后进程会 exit
                    return await desktopUpdateService.install(available, taskId);
                }

                const backendTaskId = await componentService.runComponentAction(
                    componentId,
                    hostId,
                    kind,
                    taskId,
                );
                return backendTaskId;
            } catch (err) {
                componentActionStore.failTask(taskId, err);
                throw err;
            }
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

    const isInstalling = useCallback(
        (componentId: ComponentId, hostId: string) => {
            const taskId = state.activeByTarget[targetKey(componentId, hostId)];
            if (!taskId) return false;
            const progress = state.tasks[taskId];
            if (!progress) return false;
            return progress.status === 'running' || progress.status === 'pending';
        },
        [state],
    );

    const onTaskTerminal = useCallback(
        (
            taskId: string,
            cb: (status: 'success' | 'failed' | 'cancelled') => void,
        ) => {
            // 已经在终态：下一帧 fire，让调用方有机会先 store ref/state
            const initial = componentActionStore.getSnapshot().tasks[taskId]?.status;
            if (
                initial === 'success' ||
                initial === 'failed' ||
                initial === 'cancelled'
            ) {
                queueMicrotask(() => cb(initial));
                return () => {};
            }
            const unsub = componentActionStore.subscribe(() => {
                const status = componentActionStore.getSnapshot().tasks[taskId]?.status;
                if (
                    status === 'success' ||
                    status === 'failed' ||
                    status === 'cancelled'
                ) {
                    cb(status);
                    unsub();
                }
            });
            return unsub;
        },
        [],
    );

    return { startAction, cancelAction, getProgressFor, isInstalling, onTaskTerminal };
}
