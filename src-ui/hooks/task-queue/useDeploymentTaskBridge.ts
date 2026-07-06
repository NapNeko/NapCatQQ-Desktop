import { useEffect } from 'react';

import { useDomainEvents } from '../events/useDomainEvents';
import { deploymentTaskStore } from './deploymentTaskStore';

export function useDeploymentTaskBridge(): void {
    useEffect(() => {
        void deploymentTaskStore.load().catch((err) => {
            console.error('[deploymentTaskStore] load failed:', err);
        });
    }, []);

    useDomainEvents((event) => {
        if (event.kind !== 'deployment_task_changed') return;
        deploymentTaskStore.applyTask(event.task);
    });
}
