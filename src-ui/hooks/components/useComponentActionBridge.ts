// 顶层事件桥：把 DomainEvent 流里的 component_action_progress 全部喂给
// 模块级 store。AppNext 挂载一次即可，路由切换 / Components 页卸载都不会断。

import { useDomainEvents } from '../events/useDomainEvents';
import { componentActionStore } from './componentActionStore';

export function useComponentActionEventBridge(): void {
    useDomainEvents((event) => {
        if (event.kind !== 'component_action_progress') return;
        componentActionStore.applyProgress(event.task_id, event.event);
    });
}
