// 只读主机档案；表单和选择器不需要创建主机管理的 mutation observers。
import { useQuery } from '@tanstack/react-query';
import { serverService } from '../../core/services/server.service';

export function useServers(options?: { refetchOnMount?: boolean }) {
    return useQuery({
        queryKey: ['servers'],
        queryFn: () => serverService.list(),
        refetchOnMount: options?.refetchOnMount,
    });
}
