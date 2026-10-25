// 面板凭据写入本机密钥库后，独立验证面板是否接受登录。

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { appFrameworkService } from '../../core/services/app-framework.service';
import { parseNeoBotAuthStatus } from '../../core/domain/apps/neobotPanels';
import { meetsNeoBotVersion } from '../../core/domain/apps/neobotCapabilities';
import { record, text } from '../../core/domain/apps/neobotWorkspace';
import { usePanelJson } from './useNeoBotPanel';
import type { AppInstance } from '../../core/ipc/types';

/** 一次「保存并验证」的结果 */
export type VerifyOutcome =
    | { state: 'ok'; text: string }
    | { state: 'bad'; text: string }
    | { state: 'unknown'; text: string };

const appPanelPasswordKey = (instanceId: string) => ['appPanelPassword', instanceId] as const;

/** 拿一个需要登录的接口探一次：密码存了不算数，面板认了才算。 */
async function probePanelLogin(instance: AppInstance): Promise<VerifyOutcome> {
    const probe = await appFrameworkService.panelCall(instance.id, 'GET', '/api/overview');
    if (probe === null) {
        return { state: 'unknown', text: '该框架不支持面板转发，密码已保存。' };
    }
    switch (probe.kind) {
        case 'ok':
            return { state: 'ok', text: '验证成功，面板密码可用。' };
        case 'unauthorized':
            return {
                state: 'bad',
                text: '验证失败，请重新填写当前面板密码。',
            };
        case 'not_found':
            return {
                state: 'unknown',
                text: '这个 NeoBot 版本没有 /api/overview，验证不了；密码已保存。',
            };
        case 'unreachable':
            return { state: 'unknown', text: '面板打不通，没法验证：' + (probe.message ?? '') };
        default:
            return {
                state: 'unknown',
                text: '没能验证：' + (probe.message ?? '面板拒绝了这次请求'),
            };
    }
}

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
        retry: false,
        mutationFn: async (value: string): Promise<VerifyOutcome> => {
            // 先存：panelCall 要从密钥库取凭据才能发起登录
            await appFrameworkService.setPanelPassword(instanceId, value);
            if (!value.trim()) return { state: 'unknown', text: '已清除保存的面板密码。' };
            // 立刻用需要登录的接口验一次，把结果说出来
            try {
                return await probePanelLogin(instance);
            } catch {
                return {
                    state: 'unknown',
                    text: '密码已保存，面板暂时无法验证。可稍后验证已存密码。',
                };
            }
        },
        onSuccess: () => {
            void queryClient.invalidateQueries({ queryKey: appPanelPasswordKey(instanceId) });
            void queryClient.invalidateQueries({ queryKey: ['neobotPanel', instanceId] });
        },
    });

    // 「不动密码，只问面板现在还认不认」：改过密码的用户回来第一件事通常是这个
    const verify = useMutation({
        retry: false,
        mutationFn: () => probePanelLogin(instance),
        onSuccess: () => {
            void queryClient.invalidateQueries({ queryKey: ['neobotPanel', instanceId] });
        },
    });

    const setup = useMutation({
        retry: false,
        mutationFn: async (value: string): Promise<VerifyOutcome> => {
            const result = await appFrameworkService.panelCall(
                instanceId,
                'POST',
                '/api/auth/setup',
                { password: value, confirm: value },
            );
            if (!result || result.kind !== 'ok' || record(result.data).ok === false)
                throw new Error(
                    result?.message || text(record(result?.data).error) || '首次设置密码失败',
                );
            // 设置成功后才记入本机密钥库，不把失败的密码当作可用凭据。
            await appFrameworkService.setPanelPassword(instanceId, value);
            return { state: 'ok', text: '面板密码已设置并记住。' };
        },
        onSuccess: () => {
            void queryClient.invalidateQueries({ queryKey: appPanelPasswordKey(instanceId) });
            void queryClient.invalidateQueries({ queryKey: ['neobotPanel', instanceId] });
        },
    });

    return { state, auth, loginSupported, save, setup, verify };
}
