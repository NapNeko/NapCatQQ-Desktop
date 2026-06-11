import { useDomainEvents } from '../events/useDomainEvents';
import { dockerInstallProgressStore } from './dockerInstallProgressStore';

export function useDockerInstallProgressBridge(): void {
    useDomainEvents((event) => {
        if (event.kind !== 'docker_install_progress') return;
        dockerInstallProgressStore.applyProgress(event.task_id, event.event);
    });
}