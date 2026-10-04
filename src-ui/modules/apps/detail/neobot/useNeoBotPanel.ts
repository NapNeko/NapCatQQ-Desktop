// 面板数据查询的统一入口。
//
// 把「框架不支持 / 没填密码 / 面板没起来 / 面板改版了」都收成显式状态，页面按状态给不同的话——
// 混成一个 error 的话，用户看到「加载失败」既不知道去填密码也不知道去看实例状态。

import { useQuery } from '@tanstack/react-query';
import { appFrameworkService } from '../../../../core/services/app-framework.service';
import { parseNeoBotOverview, type NeoBotOverview } from './neobotPanel';

export type PanelState<T> =
    | { kind: 'ok'; data: T }
    /** 该框架不提供面板转发（panelCall 返回 null） */
    | { kind: 'unsupported' }
    /** 没填面板密码，或密码不对 */
    | { kind: 'unauthorized'; message: string }
    | { kind: 'unreachable'; message: string }
    | { kind: 'failed'; message: string }
    /** 面板答了但形状不认识（多半是面板改版） */
    | { kind: 'malformed' };

export const neobotPanelKey = (instanceId: string, what: string) =>
    ['neobotPanel', instanceId, what] as const;

/**
 * 取一个面板端点并收窄成 T。
 *
 * path 必须是面板自己的 /api/ 路径（Rust 侧有白名单，写错会被拒）。
 * parse 返回 null 就归为 malformed —— 那是「面板答了但不是我们要的形状」，
 * 与「面板打不通」是不同的故障，提示也该不同。
 */
export function usePanelJson<T>(
    instanceId: string,
    what: string,
    path: string,
    parse: (raw: unknown) => T | null,
) {
    return useQuery<PanelState<T>, Error>({
        queryKey: neobotPanelKey(instanceId, what),
        queryFn: async (): Promise<PanelState<T>> => {
            const res = await appFrameworkService.panelCall(instanceId, 'GET', path);
            if (res === null) return { kind: 'unsupported' };
            if (res.kind === 'ok') {
                const data = parse(res.data);
                return data !== null ? { kind: 'ok', data } : { kind: 'malformed' };
            }
            return { kind: res.kind, message: res.message ?? '' };
        },
        retry: false,
    });
}

export const useNeoBotOverview = (instanceId: string) =>
    usePanelJson<NeoBotOverview>(instanceId, 'overview', '/api/overview', parseNeoBotOverview);