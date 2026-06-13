// 仅「组件页拉镜像」(docker_deploy) 使用分层进度 UI，勿用 currentStep>=2 误判安装等任务。

import type { ActionProgressView } from './progress';
import type { TaskQueueItem } from '../task-queue/types';

/** 后端在 docker pull 流式阶段才会填充 docker_layers */
export function isDockerPullLayerProgress(progress: ActionProgressView): boolean {
    return progress.dockerLayers.length > 0;
}

export function isDockerDeployTaskKind(kind: TaskQueueItem['kind']): boolean {
    return kind === 'docker_deploy';
}

/** 任务详情里是否展示镜像层列表面板（仅 docker_deploy） */
export function shouldShowDockerPullLayersInTaskDetail(
    kind: TaskQueueItem['kind'],
    progress: ActionProgressView,
): boolean {
    if (!isDockerDeployTaskKind(kind)) return false;
    if (isDockerPullLayerProgress(progress)) return true;
    // 拉取步骤进行中、尚未解析出层时，仅在任务详情给一句占位
    return (
        progress.status === 'running' &&
        progress.currentStep === 2 &&
        progress.totalSteps === 2
    );
}

/** docker 拉镜像以进度条 + 镜像层为主，不展示步骤日志区 */
export function shouldShowStepLogsInTaskDetail(kind: TaskQueueItem['kind']): boolean {
    return !isDockerDeployTaskKind(kind);
}