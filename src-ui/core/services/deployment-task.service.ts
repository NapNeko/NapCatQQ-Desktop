import { invoke, isTauri } from '../ipc/transport';
import type { DeploymentTaskList } from '../ipc/types';

export const deploymentTaskService = {
    list: async (): Promise<DeploymentTaskList> => {
        if (isTauri) return invoke<DeploymentTaskList>('list_deployment_tasks');
        return { tasks: [] };
    },

    cancel: async (taskId: string): Promise<void> => {
        if (isTauri) return invoke<void>('cancel_deployment_task', { taskId });
    },

    delete: async (taskId: string): Promise<void> => {
        if (isTauri) return invoke<void>('delete_deployment_task', { taskId });
    },

    clearFinished: async (): Promise<number> => {
        if (isTauri) return invoke<number>('clear_finished_deployment_tasks');
        return 0;
    },
};
