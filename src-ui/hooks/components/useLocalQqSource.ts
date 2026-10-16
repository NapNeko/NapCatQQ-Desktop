// 本机 QQ 来源与组件卡片的安装状态一起刷新。

import { useQuery } from '@tanstack/react-query';
import { componentService } from '../../core/services/component.service';

export const LOCAL_QQ_SOURCE_QUERY_KEY = ['localQqSource'] as const;

export function useLocalQqSource(enabled: boolean) {
    const query = useQuery({
        queryKey: LOCAL_QQ_SOURCE_QUERY_KEY,
        queryFn: componentService.localQqSource,
        enabled,
        staleTime: 30_000,
    });
    return enabled ? query.data : undefined;
}
