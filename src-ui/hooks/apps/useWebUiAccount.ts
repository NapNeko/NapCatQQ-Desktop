// 账号密码类 WebUI 的账号视图 + 重置密码。实例启停会改 can_reset，所以订阅实例列表变化后重拉。

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { appFrameworkService } from '../../core/services/app-framework.service';
import { toAppConfigError } from '../../core/domain/apps/appConfigError';
import type { AppConfigError, AppWebUiAccount } from '../../core/ipc/types';

export const webUiAccountKey = (instanceId: string) => ['appWebUiAccount', instanceId] as const;

export function useWebUiAccount(instanceId: string | null, enabled = true, stateTag?: string) {
    const queryClient = useQueryClient();
    const key = webUiAccountKey(instanceId ?? '');

    const query = useQuery<AppWebUiAccount | null, Error>({
        // stateTag 进 key：running ↔ stopped 切换时 can_reset 会变
        queryKey: [...key, stateTag ?? ''],
        queryFn: () => appFrameworkService.webuiAccount(instanceId!),
        enabled: enabled && !!instanceId,
        staleTime: 15_000,
    });

    const reset = useMutation<AppWebUiAccount, AppConfigError, string | null>({
        mutationFn: (password) =>
            appFrameworkService
                .resetWebUiPassword(instanceId!, password)
                .catch((e) => Promise.reject(toAppConfigError(e))),
        onSuccess: (account) => {
            queryClient.setQueryData<AppWebUiAccount | null>([...key, stateTag ?? ''], account);
            queryClient.invalidateQueries({ queryKey: key });
        },
    });

    return {
        account: query.data ?? null,
        isLoading: query.isLoading,
        error: query.error ?? null,
        reload: query.refetch,
        reset: reset.mutateAsync,
        isResetting: reset.isPending,
    };
}
