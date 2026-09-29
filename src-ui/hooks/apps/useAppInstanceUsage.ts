// 应用端实例的现状：在跑或在装的有几个、每个框架有几个。设置 · 功能判断能不能关用，
// 只读实例列表缓存，不带应用端页的整套 hook。

import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { appFrameworkService } from '../../core/services/app-framework.service';
import { isInstanceActive } from '../../core/domain/apps/instanceState';
import type { AppInstance } from '../../core/ipc/types';
import { APP_INSTANCES_KEY } from './appInstancesCache';

export function useAppInstanceUsage(): { active: number; byFramework: Record<string, number> } {
    const { data } = useQuery<AppInstance[], Error>({
        queryKey: APP_INSTANCES_KEY,
        queryFn: appFrameworkService.listInstances,
    });
    return useMemo(() => {
        const list = data ?? [];
        const byFramework: Record<string, number> = {};
        for (const i of list) byFramework[i.framework_id] = (byFramework[i.framework_id] ?? 0) + 1;
        return { active: list.filter(isInstanceActive).length, byFramework };
    }, [data]);
}
