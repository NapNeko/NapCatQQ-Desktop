// 麦麦学到的表达方式和黑话：列表、概况、操作。都要麦麦在跑、WebUI 应答了；操作成功后列表和概况一起刷。

import { keepPreviousData, useQuery } from '@tanstack/react-query';
import { maibotResourcesService as svc } from '../../core/services/maibot-resources.service';
import { useResourceAction } from './maibotResourceAction';
import type {
    MaiBotExpressionAction,
    MaiBotExpressionOverview,
    MaiBotExpressionPage,
    MaiBotExpressionQuery,
    MaiBotJargonAction,
    MaiBotJargonOverview,
    MaiBotJargonPage,
    MaiBotJargonQuery,
} from '../../core/ipc/types';

const exprKey = (id: string) => ['maibotExpressions', id] as const;
const jargonKey = (id: string) => ['maibotJargons', id] as const;

export function useMaiBotExpressions(instanceId: string, query: MaiBotExpressionQuery, enabled: boolean) {
    return useQuery<MaiBotExpressionPage, Error>({
        queryKey: [...exprKey(instanceId), 'list', query],
        queryFn: () => svc.expressions(instanceId, query),
        enabled,
        retry: false,
        // 翻页、换筛选时先留着上一页，列表不闪成空白
        placeholderData: keepPreviousData,
        staleTime: 15_000,
    });
}

export function useMaiBotExpressionOverview(instanceId: string, enabled: boolean) {
    return useQuery<MaiBotExpressionOverview, Error>({
        queryKey: [...exprKey(instanceId), 'overview'],
        queryFn: () => svc.expressionOverview(instanceId),
        enabled,
        retry: false,
        staleTime: 30_000,
    });
}

export function useMaiBotJargons(instanceId: string, query: MaiBotJargonQuery, enabled: boolean) {
    return useQuery<MaiBotJargonPage, Error>({
        queryKey: [...jargonKey(instanceId), 'list', query],
        queryFn: () => svc.jargons(instanceId, query),
        enabled,
        retry: false,
        placeholderData: keepPreviousData,
        staleTime: 15_000,
    });
}

export function useMaiBotJargonOverview(instanceId: string, enabled: boolean) {
    return useQuery<MaiBotJargonOverview, Error>({
        queryKey: [...jargonKey(instanceId), 'overview'],
        queryFn: () => svc.jargonOverview(instanceId),
        enabled,
        retry: false,
        staleTime: 30_000,
    });
}

export function useMaiBotExpressionAction(instanceId: string) {
    return useResourceAction<MaiBotExpressionAction>(instanceId, exprKey(instanceId), svc.expressionAction, '表达方式没改成');
}

export function useMaiBotJargonAction(instanceId: string) {
    return useResourceAction<MaiBotJargonAction>(instanceId, jargonKey(instanceId), svc.jargonAction, '黑话没改成');
}
