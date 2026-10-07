// NeoBot 详情「插件」页的数据层：已装列表查询 + 装/启停/重载/卸载动作。
//
// 所有动作都是 POST 到面板，成功后重取列表——面板自己会落盘，桌面端不另存状态。

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { appFrameworkService } from '../../core/services/app-framework.service';
// TODO: 解析器待下沉 core/domain/apps/neobot/，届时消掉 hooks→modules 跨层
import {
    parseNeoBotPlugins,
    type NeoBotPlugins,
} from '../../modules/apps/detail/neobot/neobotPanels';
import { neobotPanelKey, usePanelJson } from './useNeoBotPanel';

export function useNeoBotPlugins(instanceId: string) {
    const query = usePanelJson<NeoBotPlugins>(
        instanceId,
        'plugins',
        '/api/plugins',
        parseNeoBotPlugins,
    );
    const queryClient = useQueryClient();
    /** 一次插件动作：POST 完重取列表。面板是唯一事实来源，桌面端不乐观更新。 */
    const action = useMutation({
        mutationFn: async (req: { path: string; body?: unknown }) => {
            const res = await appFrameworkService.panelCall(instanceId, 'POST', req.path, req.body);
            if (res === null) throw new Error('该框架不支持面板操作');
            if (res.kind !== 'ok') throw new Error(res.message || '面板拒绝了这次操作');
            return res;
        },
        onSettled: () => {
            void queryClient.invalidateQueries({
                queryKey: neobotPanelKey(instanceId, 'plugins'),
            });
        },
    });

    return { query, action };
}
