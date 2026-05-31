// 供组件读取某个 docker 部署 task 进度的 hook。
//
// taskId 为 null 时返回 null（对话框还没打开 / 还没生成 taskId 的情况）。
// 用 useSyncExternalStore 订阅模块级 store，store 更新时组件自动重渲染。

import { useSyncExternalStore } from 'react';
import { dockerDeployProgressStore } from './dockerDeployProgressStore';
import type { ActionProgressView } from '../../core/domain/components/progress';

export function useDockerDeployProgress(taskId: string | null): ActionProgressView | null {
    const snapshot = useSyncExternalStore(
        dockerDeployProgressStore.subscribe,
        dockerDeployProgressStore.getSnapshot,
    );

    if (taskId == null) return null;
    return snapshot.tasks[taskId] ?? null;
}
