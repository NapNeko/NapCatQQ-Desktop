// 麦麦认识的人：列表、概况、改称呼 / 删。要麦麦在跑、WebUI 应答了；操作成功后列表和概况一起刷。

import { keepPreviousData, useQuery } from '@tanstack/react-query';
import { maibotResourcesService as svc } from '../../core/services/maibot-resources.service';
import { useResourceAction } from './maibotResourceAction';
import type { MaiBotPersonAction, MaiBotPersonOverview, MaiBotPersonPage, MaiBotPersonQuery } from '../../core/ipc/types';

const personKey = (id: string) => ['maibotPersons', id] as const;

export function useMaiBotPersons(instanceId: string, query: MaiBotPersonQuery, enabled: boolean) {
    return useQuery<MaiBotPersonPage, Error>({
        queryKey: [...personKey(instanceId), 'list', query],
        queryFn: () => svc.persons(instanceId, query),
        enabled,
        retry: false,
        placeholderData: keepPreviousData,
        staleTime: 15_000,
    });
}

export function useMaiBotPersonOverview(instanceId: string, enabled: boolean) {
    return useQuery<MaiBotPersonOverview, Error>({
        queryKey: [...personKey(instanceId), 'overview'],
        queryFn: () => svc.personOverview(instanceId),
        enabled,
        retry: false,
        staleTime: 30_000,
    });
}

export function useMaiBotPersonAction(instanceId: string) {
    return useResourceAction<MaiBotPersonAction>(instanceId, personKey(instanceId), svc.personAction, '人物没改成');
}
