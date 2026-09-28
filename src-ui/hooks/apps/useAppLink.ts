// 对接对话框的两步：选好实例和 Bot 后生成对接计划给用户看，确认后应用（后端照当时的状态重算一遍再写）。
// 计划跟着两边当下的配置走，每次打开都重新要，不留缓存。

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { appFrameworkService } from '../../core/services/app-framework.service';
import { errorText } from '../../core/domain/errors';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { pushErrorBar } from '../ui/pushErrorBar';
import { botConfigKey } from '../bot/useBotConfigsMap';
import type { AppInstance, OneBotLinkPlan } from '../../core/ipc/types';

const appLinkPlanKey = (instanceId: string, botId: string) => ['appLinkPlan', instanceId, botId] as const;

export function useAppLinkPlan(instanceId: string, botId: string, enabled: boolean) {
    const on = enabled && !!instanceId && !!botId;
    const query = useQuery<OneBotLinkPlan, Error>({
        // 关掉时换到一个不请求的空键上：只关 enabled 的话键不变，路上那次请求还算有人等，
        // 换了键它没了观察者才会被取消、signal 跟着 abort
        queryKey: on ? appLinkPlanKey(instanceId, botId) : appLinkPlanKey('', ''),
        queryFn: async ({ signal }) => {
            try {
                return await appFrameworkService.previewLink(instanceId, botId);
            } catch (err) {
                // 途中换了实例 / Bot、关了对话框或认出是 Docker Bot，这份计划没人要了，失败也不报
                if (!signal.aborted) {
                    pushErrorBar({
                        key: `app-link-preview:${instanceId}:${botId}`,
                        title: '无法生成对接计划',
                        raw: errorText(err),
                    });
                }
                throw err instanceof Error ? err : new Error(errorText(err));
            }
        },
        enabled: on,
        staleTime: 0,
        gcTime: 0,
        retry: false,
    });
    return {
        // 重新生成的途中、生成失败时都不给旧计划，免得用户对着过期的那份点「应用」
        plan: on && query.isSuccess && !query.isFetching ? query.data : null,
        previewing: on && query.isFetching,
    };
}

export function useApplyAppLink() {
    const queryClient = useQueryClient();
    return useMutation<AppInstance, unknown, { instanceId: string; botId: string; connectionName: string }>({
        mutationFn: ({ instanceId, botId }) => appFrameworkService.applyLink(instanceId, botId),
        onSuccess: (next, { botId, connectionName }) => {
            // 实例列表和应用端配置由事件桥跟着 linked 事件更新；Bot 的连接表事件桥不管，这里失效
            void queryClient.invalidateQueries({ queryKey: botConfigKey(botId) });
            pushInfoBar({
                key: `app-link:${next.id}`,
                tone: 'success',
                title: '对接完成',
                content: `Bot ${botId} 已加入连接 ${connectionName}，运行中的 Bot 会热更新。`,
                autoDismissMs: 4000,
            });
        },
        onError: (err, { instanceId }) => {
            pushErrorBar({
                key: `app-link:${instanceId}`,
                title: '对接失败',
                raw: errorText(err),
            });
        },
    });
}
