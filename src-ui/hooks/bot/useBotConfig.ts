// Bot config 读 / upsert / delete hook。

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef } from 'react';
import { botService } from '../../core/services/bot.service';
import type { BotConfig } from '../../core/ipc/generated/domain/BotConfig';
import type { DriftDecision } from '../../core/ipc/generated/DriftDecision';
import { useDomainEvents } from '../events/useDomainEvents';
import { botSnapshotsKey } from './useBotSnapshots';

const botConfigKey = (botId: string | null) => ['botConfig', botId] as const;

interface Callbacks {
    /// 保存成功回调。第二个参数是后端 publish 在 `bot_state_changed` 事件
    /// payload 里的 reason 字符串，调用方可据此区分「config_hot_reloaded」/
    /// 「config_saved_pending_login」/「config_saved_pending_reload」/ 其它，
    /// 给用户更准确的 InfoBar 提示。
    onSaved?: (snapshotBotId: string, reason: string | null) => void;
    onDeleted?: () => void;
    onError?: (msg: string) => void;
}

export function useBotConfig(botId: string | null, cb: Callbacks = {}) {
    const queryClient = useQueryClient();
    const isEditMode = botId !== null;

    // 保存动作期间收到的最后一条 bot_state_changed reason。
    // 后端在 upsert 流程里发布的 reason 与命令的返回值是分两条通道走的：
    //   1) Tauri command 返回 BotActorSnapshot（不带 reason）
    //   2) BroadcastEventBus 同步发布 BotStateChanged { snapshot, reason }
    // command 返回到 onSuccess 之前事件已经到达，所以在 isPending 期间
    // 缓存当前 botId 对应的 reason，onSuccess 时取出来交给 onSaved 回调。
    const reasonRef = useRef<string | null>(null);
    const watchBotIdRef = useRef<string | null>(null);

    useDomainEvents((event) => {
        if (event.kind !== 'bot_state_changed') return;
        const watching = watchBotIdRef.current;
        if (!watching) return;
        if (event.snapshot.bot_id !== watching) return;
        reasonRef.current = event.reason ?? null;
    });

    const query = useQuery<BotConfig | null, Error>({
        queryKey: botConfigKey(botId),
        queryFn: () => (botId ? botService.getConfig(botId) : Promise.resolve(null)),
        enabled: isEditMode,
    });

    const saveMutation = useMutation({
        mutationFn: (config: BotConfig) => {
            // 在调用前把当前 botId 标记进 watcher，让事件流处理器开始捕获 reason。
            watchBotIdRef.current = String(config.bot.QQID);
            reasonRef.current = null;
            return botService.upsertConfig(config);
        },
        onSuccess: (snapshot) => {
            // 刷新所有跟此 bot 关联的列表层缓存：
            // - botSnapshots：actor 状态可能因 hot reload restart 转移
            // - botFlavors：backend_type 切换后 BotCard 徽章必须立即跟上
            // - botConfig:<id>：detail 缓存也要失效，重进配置页拿新值
            // - botConfigsMap 共用 botConfig 前缀，自动跟着失效
            queryClient.invalidateQueries({ queryKey: botSnapshotsKey });
            queryClient.invalidateQueries({ queryKey: ['botFlavors'] });
            queryClient.invalidateQueries({ queryKey: ['botConfig'] });
            const reason = reasonRef.current;
            watchBotIdRef.current = null;
            cb.onSaved?.(snapshot.bot_id, reason);
        },
        onError: (err: any) => {
            watchBotIdRef.current = null;
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
        mutationFn: ({ config, decisions }: { config: BotConfig; decisions: DriftDecision[] }) => {
            watchBotIdRef.current = String(config.bot.QQID);
            reasonRef.current = null;
            return botService.upsertConfigWithDecisions(config, decisions);
        },
        onSuccess: (snapshot) => {
            queryClient.invalidateQueries({ queryKey: botSnapshotsKey });
            queryClient.invalidateQueries({ queryKey: ['botFlavors'] });
            queryClient.invalidateQueries({ queryKey: ['botConfig'] });
            const reason = reasonRef.current;
            watchBotIdRef.current = null;
            cb.onSaved?.(snapshot.bot_id, reason);
        },
        onError: (err: any) => {
            watchBotIdRef.current = null;
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
