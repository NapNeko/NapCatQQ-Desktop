// 面板查询区分鉴权、连接与数据格式错误。

import { useQuery } from '@tanstack/react-query';
import { appFrameworkService } from '../../core/services/app-framework.service';
import {
    asRecord,
    parseNeoBotOverview,
    type NeoBotOverview,
} from '../../core/domain/apps/neobotPanel';
import type { PanelState } from '../../core/domain/apps/neobotPanelState';

export type { PanelState };

export const neobotPanelKey = (instanceId: string, what: string) =>
    ['neobotPanel', instanceId, what] as const;

/**
 * 取一个面板端点并收窄成 T。
 *
 * path 必须是面板自己的 /api/ 路径（Rust 侧有白名单，写错会被拒）。
 * parse 返回 null 就归为 malformed —— 那是「面板答了但不是我们要的形状」，
 * 与「面板打不通」是不同的故障，提示也该不同。
 */
/**
 * 与上面同一个查询，但可以整条关掉（`enabled=false`）。
 * 用在「这个 NeoBot 版本根本没有这个接口」时：不去打，页面提前给一句人话，
 * 而不是拿一个 404 回来再翻译。
 */
export function usePanelJson<T>(
    instanceId: string,
    what: string,
    path: string,
    parse: (raw: unknown) => T | null,
    enabled = true,
    pollMs: number | false = false,
) {
    return useQuery<PanelState<T>, Error>({
        queryKey: neobotPanelKey(instanceId, what),
        enabled,
        queryFn: async (): Promise<PanelState<T>> => {
            const res = await appFrameworkService.panelCall(instanceId, 'GET', path);
            if (res === null) return { kind: 'unsupported' };
            if (res.kind === 'ok') {
                const data = parse(res.data);
                return data !== null ? { kind: 'ok', data } : { kind: 'malformed' };
            }
            // Rust 那边的 not_found 在这里变成 notFound，好让页面区分「去填密码」与「去升级 NeoBot」
            if (res.kind === 'not_found') return { kind: 'notFound', message: res.message ?? '' };
            return { kind: res.kind, message: res.message ?? '' };
        },
        retry: false,
        refetchInterval: (query) => {
            const state = query.state.data;
            const status = state?.kind === 'ok' ? asRecord(state.data)?.status : undefined;
            return ['done', 'failed', 'noop'].includes(String(status)) ? false : pollMs;
        },
        refetchIntervalInBackground: false,
    });
}

export const useNeoBotOverview = (instanceId: string) =>
    usePanelJson<NeoBotOverview>(instanceId, 'overview', '/api/overview', parseNeoBotOverview);
