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
};
