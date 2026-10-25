// 诊断助手页面的状态与服务入口。

import { useEffect, useSyncExternalStore } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { assistantService } from '../../core/services/assistant.service';
import type { AssistantSettings } from '../../core/domain/assistant/types';
import { assistantStore } from './assistantStore';

export const assistantModelStatusKey = ['assistantModelStatus'] as const;

export function useAssistant() {
    const state = useSyncExternalStore(
        assistantStore.subscribe,
        assistantStore.getSnapshot,
        assistantStore.getSnapshot,
    );
    useEffect(() => {
        void assistantStore.ensure();
    }, []);
    return { ...state, actions: assistantStore, preview: assistantService.preview };
}

export function useAssistantSettings(options: { discoverProviders?: boolean } = {}) {
    const client = useQueryClient();
    const settings = useQuery({
        queryKey: ['assistantSettings'],
        queryFn: assistantService.settings,
    });
    const providers = useQuery({
        queryKey: ['assistantProviders'],
        queryFn: assistantService.providers,
        staleTime: 30_000,
        enabled: options.discoverProviders ?? false,
    });
    const modelStatus = useQuery({
        queryKey: assistantModelStatusKey,
        queryFn: assistantService.modelStatus,
    });
    const save = useMutation({
        mutationFn: (value: AssistantSettings) => assistantService.saveSettings(value),
        onSuccess: async () => {
            await Promise.all([
                client.invalidateQueries({ queryKey: ['assistantSettings'] }),
                client.invalidateQueries({ queryKey: ['assistantProviders'] }),
                client.invalidateQueries({ queryKey: assistantModelStatusKey }),
            ]);
        },
    });
    return { settings, providers, modelStatus, save, preview: assistantService.preview };
}
