// 本机 ~/.ssh 里现成的东西：私钥候选（添加服务器选密钥）和 ssh config 里的主机（批量导入）。

import { useCallback } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { serverService } from '../../core/services/server.service';
import type { DiscoveredSshHost } from '../../core/ipc/generated/domain/DiscoveredSshHost';
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';

const SSH_CONFIG_HOSTS_KEY = ['ssh-config-hosts'] as const;

/** 每次打开都重扫：用户可能刚生成了新密钥 */
export function useLocalSshKeys(enabled: boolean) {
    return useQuery<string[], Error>({
        queryKey: ['local-ssh-keys'],
        queryFn: serverService.scanLocalSshKeys,
        enabled,
        staleTime: 0,
        gcTime: 0,
    });
}

export function useLocalSshConfigHosts(enabled: boolean) {
    return useQuery<DiscoveredSshHost[], Error>({
        queryKey: SSH_CONFIG_HOSTS_KEY,
        queryFn: serverService.discoverLocalSshHosts,
        enabled,
        // 档案增删后「已添加」必须立刻变；不受 dev 全局 30s staleTime 拖累。
        staleTime: 0,
        refetchOnMount: 'always',
    });
}

/**
 * 批量导入逐个加档案，成败由导入框汇总成一条提示，
 * 所以不走 useServerManager 的添加（那边每加一台弹一条）。
 */
export function useImportSshProfiles() {
    const queryClient = useQueryClient();
    const add = useMutation({ mutationFn: (profile: ServerProfile) => serverService.add(profile) });
    const addProfile = add.mutateAsync;

    const importProfiles = useCallback(
        async (
            items: { alias: string; profile: ServerProfile }[],
        ): Promise<{ created: string[]; failed: { alias: string; error: unknown }[] }> => {
            const created: string[] = [];
            const failed: { alias: string; error: unknown }[] = [];
            try {
                for (const item of items) {
                    try {
                        await addProfile(item.profile);
                        created.push(item.alias);
                    } catch (error) {
                        failed.push({ alias: item.alias, error });
                    }
                }
            } finally {
                await queryClient.invalidateQueries({ queryKey: ['servers'] });
                await queryClient.invalidateQueries({ queryKey: SSH_CONFIG_HOSTS_KEY });
            }
            return { created, failed };
        },
        [addProfile, queryClient],
    );

    return importProfiles;
}
