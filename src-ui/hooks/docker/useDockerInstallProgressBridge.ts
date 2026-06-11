import { useDomainEvents } from '../events/useDomainEvents';
import { dockerInstallProgressStore } from './dockerInstallProgressStore';
import { useQueryClient } from '@tanstack/react-query';

export function useDockerInstallProgressBridge(): void {
    const queryClient = useQueryClient();

    useDomainEvents((event) => {
        if (event.kind !== 'docker_install_progress') return;
        dockerInstallProgressStore.applyProgress(event.task_id, event.event);

        // 安装成功后刷新 Docker 状态缓存
        if (event.event.kind === 'finished' && event.event.ok) {
            queryClient.invalidateQueries({ queryKey: ['docker', 'hosts'] });
        }
    });
}