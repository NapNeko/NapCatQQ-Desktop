// Bot start/stop/batch* 等 mutation 的统一入口。
// 把 onSuccess 文案模板和 query cache 同步集中在一处。

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { botService } from '../../core/services/bot.service';
import type { BatchResultResponse, BotActorSnapshot } from '../../core/ipc/types';
import { botSnapshotsKey } from './useBotSnapshots';
import { updateBotSnapshots } from './botSnapshotCache';
import { botLifecycleMutationKey } from './botLifecycleMutationKey';

export type ActionMessage = { type: 'success' | 'error'; text: string };

interface MutationCallbacks {
    onMessage?: (msg: ActionMessage) => void;
}

function applySnapshotUpdate(
    queryClient: ReturnType<typeof useQueryClient>,
    updated: BotActorSnapshot,
) {
    queryClient.setQueryData<BotActorSnapshot[]>(botSnapshotsKey, (old) =>
        updateBotSnapshots(old, updated),
    );
}

function describeBatch(verb: string, res: BatchResultResponse): ActionMessage {
    const sCount = res.succeeded.length;
    const fCount = res.failed.length;
    if (fCount > 0) {
        console.error(`[bots] ${verb} 失败项`, res.failed);
    }
    return {
        type: fCount === 0 ? 'success' : 'error',
        text:
            fCount === 0
                ? `${verb}指令执行完毕。成功: ${sCount} 个`
                : `${verb}完成，${sCount} 个成功，${fCount} 个失败`,
    };
}

export function useBotMutations({ onMessage }: MutationCallbacks = {}) {
    const queryClient = useQueryClient();
    const refetch = () => queryClient.invalidateQueries({ queryKey: botSnapshotsKey });

    const startMutation = useMutation({
        mutationKey: [...botLifecycleMutationKey, 'start'],
        mutationFn: botService.start,
        onSettled: refetch,
        onSuccess: (snap) => {
            applySnapshotUpdate(queryClient, snap);
            onMessage?.({ type: 'success', text: `已发送启动指令给 Bot: ${snap.bot_id}` });
        },
    });

    const stopMutation = useMutation({
        mutationKey: [...botLifecycleMutationKey, 'stop'],
        mutationFn: botService.stop,
        onSettled: refetch,
        onSuccess: (snap) => {
            applySnapshotUpdate(queryClient, snap);
            onMessage?.({ type: 'success', text: `已发送停止指令给 Bot: ${snap.bot_id}` });
        },
        onError: (err: any) => {
            onMessage?.({ type: 'error', text: `停止失败: ${err.message || err}` });
        },
    });

    const batchStartMutation = useMutation({
        mutationKey: [...botLifecycleMutationKey, 'batchStart'],
        mutationFn: botService.batchStart,
        onSettled: refetch,
        onSuccess: (res) => {
            onMessage?.(describeBatch('批量启动', res));
        },
        onError: (err: any) => {
            onMessage?.({ type: 'error', text: `批量启动失败: ${err.message || err}` });
        },
    });

    const batchStopMutation = useMutation({
        mutationKey: [...botLifecycleMutationKey, 'batchStop'],
        mutationFn: botService.batchStop,
        onSettled: refetch,
        onSuccess: (res) => {
            onMessage?.(describeBatch('批量停止', res));
        },
        onError: (err: any) => {
            onMessage?.({ type: 'error', text: `批量停止失败: ${err.message || err}` });
        },
    });

    const batchDeleteMutation = useMutation({
        mutationKey: [...botLifecycleMutationKey, 'batchDelete'],
        mutationFn: botService.batchDelete,
        onSettled: refetch,
        onSuccess: (res) => {
            const sCount = res.succeeded.length;
            const fCount = res.failed.length;
            if (fCount > 0) console.error('[bots] 批量删除失败项', res.failed);
            onMessage?.({
                type: fCount === 0 ? 'success' : 'error',
                text:
                    fCount === 0
                        ? `批量删除执行完毕。成功删除: ${sCount} 个实例`
                        : `批量删除完成，${sCount} 个成功，${fCount} 个失败`,
            });
        },
        onError: (err: any) => {
            onMessage?.({ type: 'error', text: `批量删除失败: ${err.message || err}` });
        },
    });

    return {
        startBot: (botId: string) => startMutation.mutate(botId),
        startBotAsync: (botId: string) => startMutation.mutateAsync(botId),
        stopBot: (botId: string) => stopMutation.mutate(botId),
        batchStart: (ids: string[]) => batchStartMutation.mutate(ids),
        batchStop: (ids: string[]) => batchStopMutation.mutate(ids),
        batchDelete: (ids: string[]) => batchDeleteMutation.mutate(ids),
        isPending:
            startMutation.isPending ||
            stopMutation.isPending ||
            batchStartMutation.isPending ||
            batchStopMutation.isPending ||
            batchDeleteMutation.isPending,
        refetch,
    };
}
