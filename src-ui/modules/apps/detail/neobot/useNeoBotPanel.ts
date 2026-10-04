// 面板数据查询的统一入口。
//
// 把「框架不支持 / 没填密码 / 面板没起来 / 面板改版了」都收成显式状态，页面按状态给不同的话——
// 混成一个 error 的话，用户看到「加载失败」既不知道去填密码也不知道去看实例状态。

import { useQuery } from '@tanstack/react-query';
import { appFrameworkService } from '../../../../core/services/app-framework.service';
import { parseNeoBotOverview, type NeoBotOverview } from './neobotPanel';

export type PanelState =
    | { kind: 'ok'; overview: NeoBotOverview }
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

export function useNeoBotOverview(instanceId: string) {
    return useQuery<PanelState, Error>({
        queryKey: neobotPanelKey(instanceId, 'overview'),
        queryFn: async (): Promise<PanelState> => {
            const res = await appFrameworkService.panelCall(instanceId, 'GET', '/api/overview');
            if (res === null) return { kind: 'unsupported' };
            if (res.kind === 'ok') {
                const overview = parseNeoBotOverview(res.data);
                return overview ? { kind: 'ok', overview } : { kind: 'malformed' };
            }
            return { kind: res.kind, message: res.message ?? '' };
        },
        retry: false,
    });
}