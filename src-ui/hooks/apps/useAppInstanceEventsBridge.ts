// 顶层事件桥：应用实例的变更事件写进 react-query 缓存，AppNext 挂一次。
// 放在页面 hook 里会丢事件：页面不做 keep-alive，装完那一刻人在任务队列页，
// 「已安装」没人接，回到应用端页缓存还是安装中，之后也不会再有事件来纠正。

import { useQueryClient } from '@tanstack/react-query';
import { useDomainEvents } from '../events/useDomainEvents';
import { matchesAppInstallTask } from '../../modules/apps/instanceState';
import { APP_INSTANCES_KEY, upsertInstance } from './appInstancesCache';
import type { AppInstance } from '../../core/ipc/types';

export function useAppInstanceEventsBridge(): void {
    const queryClient = useQueryClient();

    useDomainEvents((event) => {
        if (event.kind === 'app_instance_changed') {
            queryClient.setQueryData<AppInstance[]>(APP_INSTANCES_KEY, (old) =>
                // 列表还没拉过就不凭一条事件造出半张表，等页面自己拉
                old === undefined ? old : upsertInstance(old, event.instance),
            );
            const reason = event.reason ?? '';
            if (reason === 'linked' || reason === 'unlinked' || reason === 'port_changed') {
                queryClient.invalidateQueries({ queryKey: ['appInstanceConfig', event.instance.id] });
                queryClient.invalidateQueries({ queryKey: ['appConfigText', event.instance.id] });
            }
            return;
        }
        if (event.kind !== 'deployment_task_changed') return;
        const { task } = event;
        if (task.status !== 'success' && task.status !== 'failed' && task.status !== 'cancelled') {
            return;
        }
        const hit = queryClient
            .getQueryData<AppInstance[]>(APP_INSTANCES_KEY)
            ?.find((instance) => matchesAppInstallTask(task, instance));
        if (!hit) return;
        // 后端收完尾会发 app_instance_changed。不马上重拉：那时后端可能还没写完，
        // 旧响应晚于事件到达会把「已安装」盖回去。等它的兜底轮询也走过一轮，还是安装中再拉
        setTimeout(() => {
            const now = queryClient
                .getQueryData<AppInstance[]>(APP_INSTANCES_KEY)
                ?.find((instance) => instance.id === hit.id);
            if (now?.state === 'installing') {
                void queryClient.invalidateQueries({ queryKey: APP_INSTANCES_KEY });
            }
        }, INSTALL_SETTLE_GRACE_MS);
    });
}

/** 比后端盯安装的兜底轮询（2s）多留一点 */
const INSTALL_SETTLE_GRACE_MS = 4_000;
