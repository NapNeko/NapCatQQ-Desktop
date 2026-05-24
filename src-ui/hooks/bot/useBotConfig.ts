// Bot config 读 / upsert / delete hook。

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { botService } from '../../core/services/bot.service';
import type { BotConfig } from '../../core/ipc/generated/domain/BotConfig';
import { botSnapshotsKey } from './useBotSnapshots';

const botConfigKey = (botId: string | null) => ['botConfig', botId] as const;

interface Callbacks {
    onSaved?: (snapshotBotId: string) => void;
    onDeleted?: () => void;
    onError?: (msg: string) => void;
}

export function useBotConfig(botId: string | null, cb: Callbacks = {}) {
    const queryClient = useQueryClient();
    const isEditMode = botId !== null;

    const query = useQuery<BotConfig | null, Error>({
        queryKey: botConfigKey(botId),
        queryFn: () => (botId ? botService.getConfig(botId) : Promise.resolve(null)),
        enabled: isEditMode,
    });

    const saveMutation = useMutation({
        mutationFn: botService.upsertConfig,
        onSuccess: (snapshot) => {
            queryClient.invalidateQueries({ queryKey: botSnapshotsKey });
            cb.onSaved?.(snapshot.bot_id);
        },
        onError: (err: any) => {
            cb.onError?.(`保存配置失败: ${err.message || err}`);
        },
    });

    const deleteMutation = useMutation({
        mutationFn: botService.deleteConfig,
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: botSnapshotsKey });
            cb.onDeleted?.();
        },
        onError: (err: any) => {
            cb.onError?.(`删除配置失败: ${err.message || err}`);
        },
    });

    return {
        config: query.data ?? null,
        isLoading: query.isLoading,
        error: query.error,
        save: saveMutation.mutate,
        isSaving: saveMutation.isPending,
        remove: () => {
            if (botId) deleteMutation.mutate(botId);
        },
        isDeleting: deleteMutation.isPending,
    };
}
