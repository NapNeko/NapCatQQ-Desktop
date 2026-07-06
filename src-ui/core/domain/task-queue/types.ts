// 全局任务队列的领域模型。纯类型，无 React / Tauri 依赖。

import type { ActionProgressView } from '../components/progress';

export type TaskQueueKind = 'component_action' | 'docker_install' | 'docker_deploy';

/** 与 ActionProgressView.status 对齐；docker_install 进行中用 installing。 */
export type TaskQueueStatus =
    | 'pending'
    | 'running'
    | 'paused'
    | 'installing'
    | 'success'
    | 'failed'
    | 'cancelled';

export interface TaskQueueItem {
    /** 队列内唯一键：组件/Docker 部署用 taskId；Docker 安装用 docker_install::<hostId> */
    id: string;
    kind: TaskQueueKind;
    title: string;
    hostId: string;
    hostLabel: string;
    status: TaskQueueStatus;
    /** 毫秒时间戳；未知时为 0 */
    startedAt: number;
    /** 终态毫秒时间戳；未知时为 undefined */
    endedAt?: number;
    /** 组件 / Docker 部署的细粒度进度；安装类为 null */
    progress: ActionProgressView | null;
    /** 无逐步进度时的说明（如 Docker 安装 hint） */
    logHint: string | null;
    /** 后端是否允许取消 */
    cancellable?: boolean;
}

export interface TaskQueueSnapshot {
    items: TaskQueueItem[];
    /** 角标：进行中任务数（pending / running / paused / installing） */
    activeCount: number;
}
