import { useDomainEvents } from '../events/useDomainEvents';
import { dockerInstallProgressStore } from './dockerInstallProgressStore';
import { dockerActionStore } from './dockerActionStore';
import { useQueryClient } from '@tanstack/react-query';

export function useDockerInstallProgressBridge(): void {
    const queryClient = useQueryClient();

    useDomainEvents((event) => {
        if (event.kind !== 'docker_install_progress') return;
        dockerInstallProgressStore.applyProgress(event.task_id, event.event);

        // 终态处理
        if (event.event.kind === 'finished') {
            const hostId = dockerInstallProgressStore.getSnapshot().hostByTaskId[event.task_id];
            if (hostId) {
                dockerActionStore.clearInstalling(hostId);
            }

            // 成功时刷新 Docker 状态缓存
            if (event.event.ok) {
                queryClient.invalidateQueries({ queryKey: ['docker', 'hosts'] });
            }
        }
    });
}