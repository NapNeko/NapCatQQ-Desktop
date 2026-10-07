// NeoBot 详情「部署」页的数据层：部署进度查询 + 一键生成 OneBot access token。
//
// token 生成要带 deploy_status 的 revision（面板用它挡并发写），所以查询与写操作成对放在这里，
// 组件不再自己拼 panelCall。写完成之后作废部署查询，让页面重新读面板的最新状态。

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { appFrameworkService } from '../../core/services/app-framework.service';
// TODO: 能力表/解析器待下沉 core/domain/apps/neobot/，届时消掉 hooks→modules 跨层
import { meetsNeoBotVersion } from '../../modules/apps/detail/neobot/neobotCapabilities';
import {
    parseNeoBotDeployStatus,
    type NeoBotDeployStatus,
} from '../../modules/apps/detail/neobot/neobotDeploy';
import { neobotPanelKey, usePanelJson } from './useNeoBotPanel';
import type { AppInstance } from '../../core/ipc/types';

export function useNeoBotDeploy(instance: AppInstance) {
    const instanceId = instance.id;
    const queryClient = useQueryClient();
    // 快捷部署是 1.2.4a1 才有的菜单。更老的版本（例如 1.2.3）上这个接口直接 404——
    // 与其把「面板返回 404」拿给用户，不如提前说清楚，并给一条现在就能走的路。
    const deploySupported = meetsNeoBotVersion(instance.installed_version, 'deployApi');
    const query = usePanelJson<NeoBotDeployStatus>(
        instanceId,
        'deploy',
        '/api/deploy/status',
        parseNeoBotDeployStatus,
        deploySupported,
    );
    const status = query.data?.kind === 'ok' ? query.data.data : null;

    const generateToken = useMutation({
        mutationFn: async () => {
            const res = await appFrameworkService.panelCall(
                instanceId,
                'POST',
                '/api/deploy/onebot-token',
                { revision: status?.revision },
            );
            if (res === null) throw new Error('该框架不支持面板操作');
            if (res.kind !== 'ok') throw new Error(res.message || '面板拒绝了这次操作');
            return res;
        },
        onSettled: () => {
            void queryClient.invalidateQueries({
                queryKey: neobotPanelKey(instanceId, 'deploy'),
            });
        },
    });

    return { query, deploySupported, generateToken };
}
