// 顶层事件桥：把 DomainEvent 流里的 docker_deploy_progress 全部喂给
// 模块级 store。AppNext 挂载一次即可，路由切换 / 对话框关闭都不会断。

import { useDomainEvents } from '../events/useDomainEvents';
import { dockerDeployProgressStore } from './dockerDeployProgressStore';
import { dockerActionStore } from './dockerActionStore';

export function useDockerDeployProgressBridge(): void {
    useDomainEvents((event) => {
        if (event.kind !== 'docker_deploy_progress') return;
        dockerDeployProgressStore.applyProgress(event.task_id, event.event);
        if (event.event.kind === 'finished') {
            dockerActionStore.clearPullingByTaskId(event.task_id);
        }
    });
}
