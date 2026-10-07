// 「Web 控制台」页的面板凭据数据层：是否已记住密码、面板自己的登录状态、保存并验证。
//
// 三条实测出来的教训（都不是想当然能想对的，改动前先看原注释版组件文件头）：
//   1. 面板没设密码时 /api/* 一律 403，得先探 /api/auth/status 分清「还没设密码」与「已设密码等你填」。
//   2. 登录接口本身是 1.2.4a1 才有的，更老的版本要说清楚「这个版本不支持」。
//   3. 存完必须验一次并把结果说出来——上一版存了就走，密码错了也没有任何反馈（实测反馈）。

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { appFrameworkService } from '../../core/services/app-framework.service';
// TODO: 解析器待下沉 core/domain/apps/neobot/，届时消掉这条 hooks→modules 跨层
import { parseNeoBotAuthStatus } from '../../modules/apps/detail/neobot/neobotPanels';
import { meetsNeoBotVersion } from '../../modules/apps/detail/neobot/neobotCapabilities';
import { usePanelJson } from './useNeoBotPanel';
import type { AppInstance } from '../../core/ipc/types';

/** 一次「保存并验证」的结果 */
export type VerifyOutcome =
    | { state: 'ok'; text: string }
    | { state: 'bad'; text: string }
    | { state: 'unknown'; text: string };

const appPanelPasswordKey = (instanceId: string) => ['appPanelPassword', instanceId] as const;

export function useNeoBotPanelCredential(instance: AppInstance) {
    const instanceId = instance.id;
    const queryClient = useQueryClient();
    // 登录是 1.2.4a1 起才有的能力；更老的版本连登录接口都没有
    const loginSupported = meetsNeoBotVersion(instance.installed_version, 'panelLogin');

    const state = useQuery<boolean, Error>({
        queryKey: appPanelPasswordKey(instanceId),
        queryFn: () => appFrameworkService.panelPasswordSet(instanceId),
    });
    // 探面板自己的登录状态；探不到（面板没起来）就退回「填密码」的说法，不挡用户
    const auth = usePanelJson(
        instanceId,
        'authStatus',
        '/api/auth/status',
        parseNeoBotAuthStatus,
        loginSupported,
    );

    const save = useMutation({
        mutationFn: async (value: string): Promise<VerifyOutcome> => {
            // 先存：panelCall 要从密钥库取凭据才能发起登录
            await appFrameworkService.setPanelPassword(instanceId, value);
            if (!value.trim()) return { state: 'unknown', text: '已清除保存的面板密码。' };
            // 立刻用需要登录的接口验一次，把结果说出来
            const probe = await appFrameworkService.panelCall(instanceId, 'GET', '/api/overview');
            if (probe === null) {
                return { state: 'unknown', text: '该框架不支持面板转发，密码已保存。' };
            }
            switch (probe.kind) {
                case 'ok':
                    return { state: 'ok', text: '登录成功：面板已接受这个密码。' };
                case 'unauthorized':
                    return {
                        state: 'bad',
                        text: '登录失败：面板不接受这个密码，请核对后重填（面板登录时用的那个）。',
                    };
                case 'not_found':
                    return {
                        state: 'unknown',
                        text: '这个 NeoBot 版本没有 /api/overview，验证不了；密码已保存。',
                    };
                case 'unreachable':
                    return {
                        state: 'unknown',
                        text: '面板打不通，没法验证：' + (probe.message ?? ''),
                    };
                default:
                    return {
                        state: 'unknown',
                        text: '没能验证：' + (probe.message ?? '面板拒绝了这次请求'),
                    };
            }
        },
        onSuccess: () => {
            void queryClient.invalidateQueries({ queryKey: appPanelPasswordKey(instanceId) });
        },
    });

    return { state, auth, loginSupported, save };
}
