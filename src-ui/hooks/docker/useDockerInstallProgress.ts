import { useMemo, useSyncExternalStore } from 'react';

import type { ActionProgressView } from '../../core/domain/components/progress';
import { dockerActionStore } from './dockerActionStore';
import { dockerInstallProgressStore } from './dockerInstallProgressStore';

export function useDockerInstallProgress(hostId: string): ActionProgressView | null {
    const action = useSyncExternalStore(
        dockerActionStore.subscribe,
        dockerActionStore.getSnapshot,
        dockerActionStore.getSnapshot,
    );
    const progressState = useSyncExternalStore(
        dockerInstallProgressStore.subscribe,
        dockerInstallProgressStore.getSnapshot,
        dockerInstallProgressStore.getSnapshot,
    );

    return useMemo(() => {
        const taskId = action.installTaskIdByHost[hostId];
        if (!taskId) return null;
        return progressState.tasks[taskId] ?? null;
    }, [action.installTaskIdByHost, hostId, progressState.tasks]);
}