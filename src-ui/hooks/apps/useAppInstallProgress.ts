// 某个实例正在进行的这次安装的进度。任务按安装目标从全局任务表认出来，
// 进度优先取组件动作进度表（逐条事件累加）；网页重载后那张表是空的，就拿任务快照里的事件重算。
// 两张表都由根上的桥喂，切页不断。

import { useMemo, useSyncExternalStore } from 'react';
import { deploymentTaskStore } from '../task-queue/deploymentTaskStore';
import { componentActionStore } from '../components/componentActionStore';
import {
    initialActionProgress,
    reduceActionProgress,
    type ActionProgressView,
} from '../../core/domain/components/progress';
import { matchesAppInstallTask } from '../../modules/apps/instanceState';
import type { AppInstance, DeploymentTaskSnapshot } from '../../core/ipc/types';

export function latestAppInstallTask(
    tasks: Record<string, DeploymentTaskSnapshot>,
    instance: AppInstance,
): DeploymentTaskSnapshot | undefined {
    let latest: DeploymentTaskSnapshot | undefined;
    for (const task of Object.values(tasks)) {
        if (!matchesAppInstallTask(task, instance)) continue;
        if (!latest || Number(task.submittedAtMs) > Number(latest.submittedAtMs)) latest = task;
    }
    return latest;
}

export function useAppInstallProgress(instance: AppInstance): ActionProgressView | null {
    // 表里别的任务在走进度时，这里拿到的还是同一个对象，不会跟着重渲
    const task = useSyncExternalStore(deploymentTaskStore.subscribe, () =>
        latestAppInstallTask(deploymentTaskStore.getSnapshot().tasks, instance),
    );
    const live = useSyncExternalStore(componentActionStore.subscribe, () =>
        task ? componentActionStore.getSnapshot().tasks[task.taskId] : undefined,
    );
    return useMemo(() => {
        if (live) return live;
        if (!task) return null;
        return task.progressEvents.reduce(reduceActionProgress, initialActionProgress);
    }, [live, task]);
}
