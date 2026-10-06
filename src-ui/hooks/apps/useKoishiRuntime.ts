// Koishi 的查询与操作。运行状态要实例在跑；插件表单和已装插件包停着也能取（后端在实例目录里现读）。

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { koishiService } from '../../core/services/koishi.service';
import { toAppConfigError } from '../../core/domain/apps/appConfigError';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { pushErrorBar } from '../ui/pushErrorBar';
import type {
    KoishiPackageInfo,
    KoishiPluginSchema,
    KoishiRuntimeStatus,
} from '../../core/ipc/types';

export const koishiStatusKey = (id: string, running: boolean) =>
    ['koishiStatus', id, running] as const;
export const koishiSchemaKey = (id: string, name: string) => ['koishiSchema', id, name] as const;
export const koishiPackagesKey = (id: string) => ['koishiPackages', id] as const;

const NOT_RUNNING: KoishiRuntimeStatus = {
    gate: 'not_running',
    message: null,
    bots: [],
    memory: null,
    cpu: null,
};

export function useKoishiStatus(instanceId: string, running: boolean) {
    const query = useQuery<KoishiRuntimeStatus, Error>({
        queryKey: koishiStatusKey(instanceId, running),
        queryFn: () => koishiService.status(instanceId),
        enabled: running,
        retry: false,
        // 控制台还没好、或者刚对接完 Bot 还没连上时勤快点；有 Bot 在线了跟着上游 5 秒一推的节奏慢一点看。
        // 取的是后端长连接里缓存的推送，不额外打控制台
        refetchInterval: (q) => {
            const d = q.state.data;
            return d?.gate === 'ok' && d.bots.some((b) => b.state === 'online') ? 10_000 : 3_000;
        },
    });
    return { ...query, data: running ? query.data : NOT_RUNNING };
}

/** 一个插件的表单；包版本变了（装更）由调用方把 packages 的查询作废，这里跟着重取 */
export function useKoishiPluginSchema(instanceId: string, name: string | null) {
    return useQuery<KoishiPluginSchema, Error>({
        queryKey: koishiSchemaKey(instanceId, name ?? '-'),
        queryFn: async () => {
            const [row] = await koishiService.pluginSchemas(instanceId, [name ?? '']);
            if (!row) throw new Error('没拿到插件信息');
            return row;
        },
        enabled: name !== null,
        retry: false,
        staleTime: 5 * 60_000,
    });
}

export function useKoishiPackages(instanceId: string, enabled = true) {
    return useQuery<KoishiPackageInfo[], Error>({
        queryKey: koishiPackagesKey(instanceId),
        queryFn: () => koishiService.packages(instanceId),
        enabled,
        retry: false,
        staleTime: 60_000,
    });
}

export function useKoishiRestart(instanceId: string, instanceName: string) {
    const qc = useQueryClient();
    return useMutation({
        mutationFn: () => koishiService.restart(instanceId),
        onSuccess: () => {
            pushInfoBar({
                key: `koishi-restart:${instanceId}`,
                tone: 'info',
                title: `${instanceName} 正在重启`,
                content: '几秒后回来，Bot 会自己重连',
                autoDismissMs: 5000,
            });
            setTimeout(
                () => void qc.invalidateQueries({ queryKey: ['koishiStatus', instanceId] }),
                2000,
            );
        },
        onError: (err: unknown) =>
            pushErrorBar({
                key: `koishi-restart:${instanceId}`,
                title: '重启失败',
                raw: toAppConfigError(err).message,
            }),
    });
}
