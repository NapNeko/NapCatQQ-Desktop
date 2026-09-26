// 麦麦长期记忆（知识库）：状态、导入与任务、查 / 删记忆、图谱。都要麦麦在跑、WebUI 应答了。
// 记忆数据（记录、来源、图谱）挂在同一个 data key 下：导完、删完一起刷。任务进度只在有任务在跑时轮询。

import { keepPreviousData, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { maibotMemoryService as svc } from '../../core/services/maibot-memory.service';
import { toAppConfigError } from '../../core/domain/apps/appConfigError';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { pushAppErrorBar } from './pushAppErrorBar';
import type {
    MaiBotMemoryDeleteAction,
    MaiBotMemoryDeleteResult,
    MaiBotMemoryImport,
    MaiBotMemoryQuery,
    MaiBotMemoryRecordKind,
    MaiBotMemoryTask,
    MaiBotMemoryTaskAction,
    MaiBotMemoryTaskStatus,
} from '../../core/ipc/types';

const root = (id: string) => ['maibotMemory', id] as const;
const dataKey = (id: string) => [...root(id), 'data'] as const;
const tasksKey = (id: string) => [...root(id), 'tasks'] as const;

const ACTIVE: ReadonlySet<MaiBotMemoryTaskStatus> = new Set(['queued', 'preparing', 'running', 'cancelling']);
export const taskActive = (t: Pick<MaiBotMemoryTask, 'status'>) => ACTIVE.has(t.status);

export function useMaiBotMemoryStatus(instanceId: string, enabled: boolean) {
    return useQuery({
        queryKey: [...root(instanceId), 'status'],
        queryFn: () => svc.status(instanceId),
        enabled,
        retry: false,
        staleTime: 10_000,
        // 刚打开时在初始化：隔两秒看一眼，好了自己换页面
        refetchInterval: (q) => (q.state.data?.state === 'starting' ? 2000 : false),
    });
}

export function useMaiBotMemoryImportSetup(instanceId: string, enabled: boolean) {
    return useQuery({
        queryKey: [...root(instanceId), 'importSetup'],
        queryFn: () => svc.importSetup(instanceId),
        enabled,
        retry: false,
        staleTime: 60_000,
    });
}

export function useMaiBotMemoryTasks(instanceId: string, enabled: boolean, pollMs = 1000) {
    return useQuery({
        queryKey: tasksKey(instanceId),
        queryFn: () => svc.tasks(instanceId),
        enabled,
        retry: false,
        staleTime: 5_000,
        refetchInterval: (q) => (q.state.data?.some(taskActive) ? Math.max(500, pollMs) : false),
    });
}

export function useMaiBotMemoryTask(instanceId: string, taskId: string | null, pollMs = 1000) {
    return useQuery({
        queryKey: [...tasksKey(instanceId), 'detail', taskId],
        queryFn: () => svc.task(instanceId, taskId ?? ''),
        enabled: taskId !== null,
        retry: false,
        refetchInterval: (q) => (q.state.data && taskActive(q.state.data.task) ? Math.max(500, pollMs) : false),
    });
}

export function useMaiBotMemoryImport(instanceId: string) {
    const qc = useQueryClient();
    return useMutation<MaiBotMemoryTask, unknown, MaiBotMemoryImport>({
        mutationFn: (req) => svc.importMemory(instanceId, req),
        onSuccess: () => void qc.invalidateQueries({ queryKey: tasksKey(instanceId) }),
        onError: (err) => {
            pushAppErrorBar({ key: `maibotMemoryImport-fail:${instanceId}`, title: '没导进去', raw: toAppConfigError(err).message });
        },
    });
}

export function useMaiBotMemoryTaskAction(instanceId: string) {
    const qc = useQueryClient();
    return useMutation<MaiBotMemoryTask, unknown, MaiBotMemoryTaskAction>({
        mutationFn: (a) => svc.taskAction(instanceId, a),
        onSuccess: () => void qc.invalidateQueries({ queryKey: tasksKey(instanceId) }),
        onError: (err) => {
            pushAppErrorBar({ key: `maibotMemoryTask-fail:${instanceId}`, title: '导入任务没改成', raw: toAppConfigError(err).message });
        },
    });
}

/** 导完一批要让记录、来源、图谱都刷新 */
export function useRefreshMemoryData(instanceId: string) {
    const qc = useQueryClient();
    return () => void qc.invalidateQueries({ queryKey: dataKey(instanceId) });
}

export function useMaiBotMemoryRecords(instanceId: string, query: MaiBotMemoryQuery, enabled: boolean) {
    return useQuery({
        queryKey: [...dataKey(instanceId), 'records', query],
        queryFn: () => svc.records(instanceId, query),
        enabled,
        retry: false,
        placeholderData: keepPreviousData,
        staleTime: 15_000,
    });
}

export function useMaiBotMemoryRecord(instanceId: string, kind: MaiBotMemoryRecordKind, recordId: string | null) {
    return useQuery({
        queryKey: [...dataKey(instanceId), 'record', kind, recordId],
        queryFn: () => svc.record(instanceId, kind, recordId ?? ''),
        enabled: recordId !== null,
        retry: false,
        staleTime: 15_000,
    });
}

export function useMaiBotMemorySources(instanceId: string, enabled: boolean) {
    return useQuery({
        queryKey: [...dataKey(instanceId), 'sources'],
        queryFn: () => svc.sources(instanceId),
        enabled,
        retry: false,
        staleTime: 15_000,
    });
}

export function useMaiBotMemoryDeleteOps(instanceId: string, enabled: boolean) {
    return useQuery({
        queryKey: [...dataKey(instanceId), 'deleteOps'],
        queryFn: () => svc.deleteOps(instanceId),
        enabled,
        retry: false,
        staleTime: 15_000,
    });
}

/** 预览不动数据；执行、恢复之后记录、来源、图谱、最近删除一起刷 */
export function useMaiBotMemoryDelete(instanceId: string) {
    const qc = useQueryClient();
    return useMutation<MaiBotMemoryDeleteResult, unknown, MaiBotMemoryDeleteAction>({
        mutationFn: (a) => svc.deleteAction(instanceId, a),
        onSuccess: (res, a) => {
            if (a.op === 'preview') return;
            void qc.invalidateQueries({ queryKey: dataKey(instanceId) });
            if (res.message) {
                pushInfoBar({ key: `maibotMemoryDelete:${instanceId}`, tone: 'success', title: res.message, autoDismissMs: 3000 });
            }
        },
        onError: (err) => {
            pushAppErrorBar({ key: `maibotMemoryDelete-fail:${instanceId}`, title: '没删成', raw: toAppConfigError(err).message });
        },
    });
}

export function useMaiBotMemoryGraph(instanceId: string, maxNodes: number, enabled: boolean) {
    return useQuery({
        queryKey: [...dataKey(instanceId), 'graph', maxNodes],
        queryFn: () => svc.graph(instanceId, maxNodes),
        enabled,
        retry: false,
        placeholderData: keepPreviousData,
        staleTime: 30_000,
    });
}

export function useMaiBotMemoryGraphNode(instanceId: string, nodeId: string | null) {
    return useQuery({
        queryKey: [...dataKey(instanceId), 'node', nodeId],
        queryFn: () => svc.graphNode(instanceId, nodeId ?? ''),
        enabled: nodeId !== null,
        retry: false,
        staleTime: 30_000,
    });
}

export function useMaiBotMemoryGraphSearch(instanceId: string, query: string, enabled: boolean) {
    return useQuery({
        queryKey: [...dataKey(instanceId), 'graphSearch', query],
        queryFn: () => svc.graphSearch(instanceId, query),
        enabled: enabled && query.trim().length > 0,
        retry: false,
        staleTime: 30_000,
    });
}
