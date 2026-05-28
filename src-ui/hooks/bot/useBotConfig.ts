// Bot config 读 / upsert / delete hook。

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { botService } from '../../core/services/bot.service';
import type { BotConfig } from '../../core/ipc/generated/domain/BotConfig';
import type { DriftDecision } from '../../core/ipc/generated/DriftDecision';
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
            // 刷新所有跟此 bot 关联的列表层缓存：
            // - botSnapshots：actor 状态可能因 hot reload restart 转移
            // - botFlavors：backend_type 切换后 BotCard 徽章必须立即跟上
            // - botConfig:<id>：detail 缓存也要失效，重进配置页拿新值
            // - botConfigsMap 共用 botConfig 前缀，自动跟着失效
            queryClient.invalidateQueries({ queryKey: botSnapshotsKey });
            queryClient.invalidateQueries({ queryKey: ['botFlavors'] });
            queryClient.invalidateQueries({ queryKey: ['botConfig'] });
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
            queryClient.invalidateQueries({ queryKey: ['botFlavors'] });
            queryClient.invalidateQueries({ queryKey: ['botConfig'] });
            cb.onDeleted?.();
        },
        onError: (err: any) => {
            cb.onError?.(`删除配置失败: ${err.message || err}`);
        },
    });

    const saveWithDecisionsMutation = useMutation({
        mutationFn: ({ config, decisions }: { config: BotConfig; decisions: DriftDecision[] }) =>
            botService.upsertConfigWithDecisions(config, decisions),
        onSuccess: (snapshot) => {
            queryClient.invalidateQueries({ queryKey: botSnapshotsKey });
            queryClient.invalidateQueries({ queryKey: ['botFlavors'] });
            queryClient.invalidateQueries({ queryKey: ['botConfig'] });
            cb.onSaved?.(snapshot.bot_id);
        },
        onError: (err: any) => {
            cb.onError?.(`保存配置失败: ${err.message || err}`);
        },
    });

    return {
        config: query.data ?? null,
        isLoading: query.isLoading,
        error: query.error,
        save: saveMutation.mutate,
        saveWithDecisions: (config: BotConfig, decisions: DriftDecision[]) =>
            saveWithDecisionsMutation.mutate({ config, decisions }),
        isSaving: saveMutation.isPending || saveWithDecisionsMutation.isPending,
        remove: () => {
            if (botId) deleteMutation.mutate(botId);
        },
        isDeleting: deleteMutation.isPending,
    };
}
