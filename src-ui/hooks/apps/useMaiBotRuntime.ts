// 麦麦运行期接口的查询与操作。都要实例在跑；没在跑时查询不发，状态直接当 not_running。

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { appFrameworkService } from '../../core/services/app-framework.service';
import { toAppConfigError } from '../../core/domain/apps/appConfigError';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { pushErrorBar } from '../ui/pushErrorBar';
import type {
    MaiBotAPIProvider,
    MaiBotChatSession,
    MaiBotMCPServerItemConfig,
    MaiBotMcpStatus,
    MaiBotRuntimeStatus,
    MaiBotStatsSummary,
} from '../../core/ipc/types';

// 运行状态进键：停了再起是另一份数据，不会拿停之前的「运行中」顶着
export const maibotStatusKey = (id: string, running: boolean) => ['maibotStatus', id, running] as const;
export const maibotStatsKey = (id: string, hours: number) => ['maibotStats', id, hours] as const;
export const maibotSessionsKey = (id: string) => ['maibotSessions', id] as const;
export const maibotMcpStatusKey = (id: string) => ['maibotMcpStatus', id] as const;

const NOT_RUNNING: MaiBotRuntimeStatus = { gate: 'not_running', message: '启动麦麦后才能看运行状态' };

function fail(title: string, key: string) {
    return (err: unknown) => {
        pushErrorBar({ key, title, raw: toAppConfigError(err).message });
    };
}

export function useMaiBotStatus(instanceId: string, running: boolean) {
    const query = useQuery<MaiBotRuntimeStatus, Error>({
        queryKey: maibotStatusKey(instanceId, running),
        queryFn: () => appFrameworkService.maibotStatus(instanceId),
        enabled: running,
        retry: false,
        // 刚启动 WebUI 还没起来时勤快点看，起来了就慢下来
        refetchInterval: (q) => (q.state.data?.gate === 'ok' ? 30_000 : 3_000),
    });
    return { ...query, data: running ? query.data : NOT_RUNNING };
}

export function useMaiBotStats(instanceId: string, hours: number, enabled: boolean) {
    return useQuery<MaiBotStatsSummary, Error>({
        queryKey: maibotStatsKey(instanceId, hours),
        queryFn: () => appFrameworkService.maibotStats(instanceId, hours),
        enabled,
        retry: false,
        staleTime: 60_000,
        refetchInterval: enabled ? 120_000 : false,
    });
}

export function useMaiBotChatSessions(instanceId: string, enabled: boolean) {
    return useQuery<MaiBotChatSession[], Error>({
        queryKey: maibotSessionsKey(instanceId),
        queryFn: () => appFrameworkService.maibotChatSessions(instanceId),
        enabled,
        retry: false,
        staleTime: 30_000,
    });
}

export function useMaiBotMcpStatus(instanceId: string, enabled: boolean) {
    return useQuery<MaiBotMcpStatus, Error>({
        queryKey: maibotMcpStatusKey(instanceId),
        queryFn: () => appFrameworkService.maibotMcpStatus(instanceId),
        enabled,
        retry: false,
        refetchInterval: enabled ? 15_000 : false,
    });
}

export function useMaiBotRestart(instanceId: string, instanceName: string) {
    const qc = useQueryClient();
    return useMutation({
        mutationFn: () => appFrameworkService.maibotRestart(instanceId),
        onSuccess: () => {
            pushInfoBar({
                key: `maibot-restart:${instanceId}`,
                tone: 'info',
                title: `${instanceName} 正在重启`,
                content: '十几秒后回来，这期间收到的消息不会回',
                autoDismissMs: 5000,
            });
            // 上游半秒后才退出，立刻查会看到旧进程还活着
            setTimeout(() => void qc.invalidateQueries({ queryKey: ['maibotStatus', instanceId] }), 1500);
        },
        onError: fail('重启失败', `maibot-restart:${instanceId}`),
    });
}

/** 拉模型列表、测连接、试连 MCP：结果只给发起的那张卡用，不进缓存 */
export function useMaiBotProbes(instanceId: string) {
    const providerModels = useMutation({
        mutationFn: (provider: MaiBotAPIProvider) => appFrameworkService.maibotProviderModels(instanceId, provider),
    });
    const testProvider = useMutation({
        mutationFn: (provider: MaiBotAPIProvider) => appFrameworkService.maibotTestProvider(instanceId, provider),
    });
    const testMcp = useMutation({
        mutationFn: (server: MaiBotMCPServerItemConfig) => appFrameworkService.maibotTestMcp(instanceId, server),
    });
    return { providerModels, testProvider, testMcp };
}
