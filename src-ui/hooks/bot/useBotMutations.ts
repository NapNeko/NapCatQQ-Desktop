// Bot start/stop/batch* 等 mutation 的统一入口。
// 把 onSuccess 文案模板和 query cache 同步集中在一处。

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { botService } from '../../core/services/bot.service';
import type { BatchResultResponse, BotActorSnapshot } from '../../core/ipc/types';
import { botSnapshotsKey } from './useBotSnapshots';

export type ActionMessage = { type: 'success' | 'error'; text: string };

interface MutationCallbacks {
    onMessage?: (msg: ActionMessage) => void;
}

function applySnapshotUpdate(
    queryClient: ReturnType<typeof useQueryClient>,
    updated: BotActorSnapshot,
) {
    queryClient.setQueryData<BotActorSnapshot[]>(botSnapshotsKey, (old) =>
        old ? old.map((s) => (s.bot_id === updated.bot_id ? updated : s)) : old,
    );
}

function describeBatch(verb: string, res: BatchResultResponse): ActionMessage {
    const sCount = res.succeeded.length;
    const fCount = res.failed.length;
    const failDetail =
        fCount > 0
            ? ` 失败项: ${res.failed.map(([id, reason]) => `${id}(${reason})`).join(', ')}`
            : '';
    return {
        type: fCount === 0 ? 'success' : 'error',
        text: `${verb}指令执行完毕。成功: ${sCount} 个, 失败: ${fCount} 个。${failDetail}`,
    };
}

export function useBotMutations({ onMessage }: MutationCallbacks = {}) {
    const queryClient = useQueryClient();
    const refetch = () => queryClient.invalidateQueries({ queryKey: botSnapshotsKey });

    const startMutation = useMutation({
        mutationFn: botService.start,
        onSuccess: (snap) => {
            applySnapshotUpdate(queryClient, snap);
            onMessage?.({ type: 'success', text: `已发送启动指令给 Bot: ${snap.bot_id}` });
        },
        onError: (err: any) => {
            onMessage?.({ type: 'error', text: `启动失败: ${err.message || err}` });
        },
    });

    const stopMutation = useMutation({
        mutationFn: botService.stop,
        onSuccess: (snap) => {
            applySnapshotUpdate(queryClient, snap);
            onMessage?.({ type: 'success', text: `已发送停止指令给 Bot: ${snap.bot_id}` });
        },
        onError: (err: any) => {
            onMessage?.({ type: 'error', text: `停止失败: ${err.message || err}` });
        },
    });

    const batchStartMutation = useMutation({
        mutationFn: botService.batchStart,
        onSuccess: (res) => {
            refetch();
            onMessage?.(describeBatch('批量启动', res));
        },
        onError: (err: any) => {
            onMessage?.({ type: 'error', text: `批量启动失败: ${err.message || err}` });
        },
    });

    const batchStopMutation = useMutation({
        mutationFn: botService.batchStop,
        onSuccess: (res) => {
            refetch();
            onMessage?.(describeBatch('批量停止', res));
        },
        onError: (err: any) => {
            onMessage?.({ type: 'error', text: `批量停止失败: ${err.message || err}` });
        },
    });

    const batchDeleteMutation = useMutation({
        mutationFn: botService.batchDelete,
        onSuccess: (res) => {
            refetch();
            const sCount = res.succeeded.length;
            const fCount = res.failed.length;
            const failDetail =
                fCount > 0
                    ? ` 失败项: ${res.failed.map(([id, reason]) => `${id}(${reason})`).join(', ')}`
                    : '';
            onMessage?.({
                type: fCount === 0 ? 'success' : 'error',
                text: `批量删除执行完毕。成功删除: ${sCount} 个实例, 失败: ${fCount} 个。${failDetail}`,
            });
        },
        onError: (err: any) => {
            onMessage?.({ type: 'error', text: `批量删除失败: ${err.message || err}` });
        },
    });

    return {
        startBot: (botId: string) => startMutation.mutate(botId),
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
