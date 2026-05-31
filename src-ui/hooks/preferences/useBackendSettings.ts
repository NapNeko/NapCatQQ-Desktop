// 后端持久化设置（轮询间隔 / 性能监控 / GitHub PAT）读写 hook。
//
// 与 preferencesStore（纯客户端偏好）分工：这边只管需要落后端的设置。读走 useQuery
// 缓存，存走 mutation + 全局 InfoBar 反馈。设置页拿一份 server 值做草稿，保存后
// invalidate 让缓存跟上。

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { settingsService, type BackendSettings } from '../../core/services/settings.service';
import { pushInfoBar } from '../ui/globalInfoBarStore';

const backendSettingsKey = ['appSettings'] as const;

interface Callbacks {
    onSaved?: () => void;
}

export function useBackendSettings(cb: Callbacks = {}) {
    const queryClient = useQueryClient();

    const query = useQuery<BackendSettings, Error>({
        queryKey: backendSettingsKey,
        queryFn: settingsService.get,
    });

    const saveMutation = useMutation({
        mutationFn: settingsService.set,
        onSuccess: (_void, saved) => {
            // 写回缓存而非 invalidate：避免保存后立刻重读又触发一次 IPC，
            // 同时让设置页草稿与 server 值瞬间一致。
            queryClient.setQueryData(backendSettingsKey, saved);
            pushInfoBar({ key: 'app-settings-save', tone: 'success', title: '设置已保存' });
            cb.onSaved?.();
        },
        onError: (err: Error) => {
            pushInfoBar({
                key: 'app-settings-save',
                tone: 'danger',
                title: '设置保存失败',
                content: err.message || String(err),
            });
        },
    });

    return {
        settings: query.data ?? null,
        isLoading: query.isLoading,
        error: query.error,
        save: saveMutation.mutate,
        isSaving: saveMutation.isPending,
    };
}
