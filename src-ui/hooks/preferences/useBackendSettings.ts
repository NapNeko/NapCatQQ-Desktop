// 后端持久化设置读写 hook。
//
// 设置页统一草稿（settings-draft）在保存时：先 set_app_settings，再写入 localStorage 偏好。
// 读走 useQuery；保存成功后 setQueryData 更新后端缓存。

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { settingsService } from '../../core/services/settings.service';
import {
    applyClientPrefsFromDraft,
    backendSlice,
    type SettingsDraft,
} from '../../modules/settings/settings-draft';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { reloadBotRuntimeMetricsCatalogSettings } from '../bot/botRuntimeMetricsCatalog';

const backendSettingsKey = ['appSettings'] as const;

interface Callbacks {
    onSaved?: () => void;
}

export function useBackendSettings(cb: Callbacks = {}) {
    const queryClient = useQueryClient();

    const query = useQuery({
        queryKey: backendSettingsKey,
        queryFn: settingsService.get,
    });

    const saveMutation = useMutation({
        mutationFn: async (draft: SettingsDraft) => {
            const savedBackend = backendSlice(draft);
            await settingsService.set(savedBackend);
            // 等待主题切换动画完成（如果有），再推 InfoBar，避免动画期间弹出
            await applyClientPrefsFromDraft(draft);
            return savedBackend;
        },
        onSuccess: (savedBackend) => {
            queryClient.setQueryData(backendSettingsKey, savedBackend);
            void reloadBotRuntimeMetricsCatalogSettings();
            pushInfoBar({
                key: 'app-settings-save',
                tone: 'success',
                title: '设置已保存',
            });
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