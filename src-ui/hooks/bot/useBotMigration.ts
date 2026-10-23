// 迁移预检与后台任务轮询；弹窗关闭后继续跟踪已发起的任务。

import { useEffect, useRef, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { botMigrationService } from '../../core/services/bot-migration.service';
import type { BotMigrationJob } from '../../core/ipc/generated/domain/BotMigrationJob';
import type { BotMigrationPhase } from '../../core/ipc/generated/domain/BotMigrationPhase';
import { APP_INSTANCES_KEY } from '../apps/appInstancesCache';
import { botSnapshotsKey } from './useBotSnapshots';

export function isBotMigrationRunning(phase: BotMigrationPhase): boolean {
    return !['completed', 'failed', 'rolled_back', 'recovery_required'].includes(phase);
}

export function useBotMigration(
    botId: string,
    targetServerId: string,
    open: boolean,
    planningAgain = false,
) {
    const queryClient = useQueryClient();
    const [tracking, setTracking] = useState(false);
    const observedPhase = useRef('');
    const jobKey = ['botMigration', botId] as const;
    const available = botMigrationService.available;

    const jobQuery = useQuery({
        queryKey: jobKey,
        queryFn: () => botMigrationService.get(botId),
        enabled: available && (open || tracking),
        retry: false,
        refetchInterval: (query) =>
            query.state.data && isBotMigrationRunning(query.state.data.phase) ? 1_500 : false,
        refetchIntervalInBackground: true,
        staleTime: 0,
    });
    const job = jobQuery.data ?? null;
    const running = job != null && isBotMigrationRunning(job.phase);

    const previewQuery = useQuery({
        queryKey: ['botMigrationPreview', botId, targetServerId],
        queryFn: () =>
            botMigrationService.preview({ bot_id: botId, target_server_id: targetServerId }),
        enabled:
            available &&
            open &&
            !!targetServerId &&
            jobQuery.isSuccess &&
            (!job || planningAgain) &&
            !running &&
            !jobQuery.isFetching,
        retry: false,
        staleTime: 0,
    });

    const storeJob = async (updated: BotMigrationJob) => {
        // 后台旧快照不能覆盖刚返回的启动/回退状态，否则可能提前停止轮询。
        await queryClient.cancelQueries({ queryKey: jobKey, exact: true });
        queryClient.setQueryData(jobKey, updated);
        setTracking(isBotMigrationRunning(updated.phase));
    };
    const startMutation = useMutation({
        mutationFn: () => {
            const plan = previewQuery.data;
            if (!plan || previewQuery.isFetching || plan.blockers.length || running) {
                throw new Error('预检尚未通过，或已有迁移任务正在运行。');
            }
            return botMigrationService.start(
                { bot_id: botId, target_server_id: targetServerId },
                plan.fingerprint,
            );
        },
        onSuccess: storeJob,
        onError: () => {
            // 启动请求可能已被接收；重新读取任务，避免重复提交。
            void jobQuery.refetch();
            void previewQuery.refetch();
        },
    });
    const rollbackMutation = useMutation({
        mutationFn: (acceptDataLoss: boolean) => {
            if (!job || (running && job.phase !== 'awaiting_login') || !job.rollback_available) {
                throw new Error('当前任务暂时无法回退。');
            }
            return botMigrationService.rollback(job.id, acceptDataLoss);
        },
        onSuccess: storeJob,
        onError: () => void jobQuery.refetch(),
    });

    useEffect(() => {
        setTracking(running);
        if (!job) return;
        const phaseKey = `${job.id}:${job.phase}`;
        if (observedPhase.current === phaseKey) return;
        observedPhase.current = phaseKey;
        if (
            ['awaiting_login', 'completed', 'failed', 'rolled_back', 'recovery_required'].includes(
                job.phase,
            )
        ) {
            void queryClient.invalidateQueries({ queryKey: botSnapshotsKey });
            void queryClient.invalidateQueries({ queryKey: ['botConfig', botId] });
            void queryClient.invalidateQueries({ queryKey: APP_INSTANCES_KEY });
            for (const app of job.apps) {
                void queryClient.invalidateQueries({
                    queryKey: ['appInstanceConfig', app.instance_id],
                });
                void queryClient.invalidateQueries({
                    queryKey: ['appConfigText', app.instance_id],
                });
            }
        }
    }, [botId, job, queryClient, running]);

    return {
        available,
        job,
        running,
        jobQuery,
        previewQuery,
        startMutation,
        rollbackMutation,
    };
}
