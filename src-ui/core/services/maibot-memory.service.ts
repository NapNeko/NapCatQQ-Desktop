// 麦麦长期记忆（知识库）的 IPC：状态、导入与任务、查 / 删记忆、图谱。命令字面量只在此文件出现，改名只改这一处；
// 浏览器预览走 mock。

import { invoke, isTauri, pickTextFiles } from '../ipc/transport';
import type {
    MaiBotLocalTextFile,
    MaiBotMemoryDeleteAction,
    MaiBotMemoryDeleteOp,
    MaiBotMemoryDeleteResult,
    MaiBotMemoryGraph,
    MaiBotMemoryGraphHit,
    MaiBotMemoryImport,
    MaiBotMemoryImportSetup,
    MaiBotMemoryNodeDetail,
    MaiBotMemoryQuery,
    MaiBotMemoryRecordDetail,
    MaiBotMemoryRecordKind,
    MaiBotMemoryRecordPage,
    MaiBotMemorySource,
    MaiBotMemoryStatus,
    MaiBotMemoryTask,
    MaiBotMemoryTaskAction,
    MaiBotMemoryTaskDetail,
} from '../ipc/types';
import { peekMockAppInstance as mockInst } from '../ipc/mock/app-framework.mock';
import { mockMaiBotMemory as mock } from '../ipc/mock/maibot-memory.mock';

export const maibotMemoryService = {
    status: async (instanceId: string): Promise<MaiBotMemoryStatus> => {
        if (!isTauri) return mock.status(mockInst(instanceId));
        return invoke<MaiBotMemoryStatus>('maibot_memory_status', { instanceId });
    },

    importSetup: async (instanceId: string): Promise<MaiBotMemoryImportSetup> => {
        if (!isTauri) return mock.importSetup(mockInst(instanceId));
        return invoke<MaiBotMemoryImportSetup>('maibot_memory_import_setup', { instanceId });
    },

    importMemory: async (
        instanceId: string,
        req: MaiBotMemoryImport,
    ): Promise<MaiBotMemoryTask> => {
        if (!isTauri) return mock.importMemory(mockInst(instanceId), req);
        return invoke<MaiBotMemoryTask>('maibot_memory_import', { instanceId, req });
    },

    tasks: async (instanceId: string): Promise<MaiBotMemoryTask[]> => {
        if (!isTauri) return mock.tasks(mockInst(instanceId));
        return invoke<MaiBotMemoryTask[]>('maibot_memory_tasks', { instanceId });
    },

    task: async (instanceId: string, taskId: string): Promise<MaiBotMemoryTaskDetail> => {
        if (!isTauri) return mock.task(mockInst(instanceId), taskId);
        return invoke<MaiBotMemoryTaskDetail>('maibot_memory_task', { instanceId, taskId });
    },

    taskAction: async (
        instanceId: string,
        action: MaiBotMemoryTaskAction,
    ): Promise<MaiBotMemoryTask> => {
        if (!isTauri) return mock.taskAction(mockInst(instanceId), action);
        return invoke<MaiBotMemoryTask>('maibot_memory_task_action', { instanceId, action });
    },

    records: async (
        instanceId: string,
        query: MaiBotMemoryQuery,
    ): Promise<MaiBotMemoryRecordPage> => {
        if (!isTauri) return mock.records(mockInst(instanceId), query);
        return invoke<MaiBotMemoryRecordPage>('maibot_memory_records', { instanceId, query });
    },

    record: async (
        instanceId: string,
        kind: MaiBotMemoryRecordKind,
        recordId: string,
    ): Promise<MaiBotMemoryRecordDetail> => {
        if (!isTauri) return mock.record(mockInst(instanceId), kind, recordId);
        return invoke<MaiBotMemoryRecordDetail>('maibot_memory_record', {
            instanceId,
            kind,
            recordId,
        });
    },

    sources: async (instanceId: string): Promise<MaiBotMemorySource[]> => {
        if (!isTauri) return mock.sources(mockInst(instanceId));
        return invoke<MaiBotMemorySource[]>('maibot_memory_sources', { instanceId });
    },

    deleteAction: async (
        instanceId: string,
        action: MaiBotMemoryDeleteAction,
    ): Promise<MaiBotMemoryDeleteResult> => {
        if (!isTauri) return mock.deleteAction(mockInst(instanceId), action);
        return invoke<MaiBotMemoryDeleteResult>('maibot_memory_delete', { instanceId, action });
    },

    deleteOps: async (instanceId: string): Promise<MaiBotMemoryDeleteOp[]> => {
        if (!isTauri) return mock.deleteOps(mockInst(instanceId));
        return invoke<MaiBotMemoryDeleteOp[]>('maibot_memory_delete_ops', { instanceId });
    },

    graph: async (instanceId: string, maxNodes: number): Promise<MaiBotMemoryGraph> => {
        if (!isTauri) return mock.graph(mockInst(instanceId), maxNodes);
        return invoke<MaiBotMemoryGraph>('maibot_memory_graph', { instanceId, maxNodes });
    },

    graphNode: async (instanceId: string, nodeId: string): Promise<MaiBotMemoryNodeDetail> => {
        if (!isTauri) return mock.graphNode(mockInst(instanceId), nodeId);
        return invoke<MaiBotMemoryNodeDetail>('maibot_memory_graph_node', { instanceId, nodeId });
    },

    graphSearch: async (instanceId: string, query: string): Promise<MaiBotMemoryGraphHit[]> => {
        if (!isTauri) return mock.graphSearch(mockInst(instanceId), query);
        return invoke<MaiBotMemoryGraphHit[]>('maibot_memory_graph_search', { instanceId, query });
    },

    /** 系统对话框挑文本文件；取消给空数组 */
    pickFiles: async (): Promise<string[]> => {
        if (!isTauri) return mock.pickFiles();
        return pickTextFiles('挑要导入的资料');
    },

    /** 导入前看本机文件，不碰实例 */
    localTexts: async (paths: string[]): Promise<MaiBotLocalTextFile[]> => {
        if (!isTauri) return mock.localTexts(paths);
        return invoke<MaiBotLocalTextFile[]>('maibot_local_texts', { paths });
    },
};
