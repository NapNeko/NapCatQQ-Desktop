import { createContext } from 'react';

// 仅在当前实例详情存活期间保留草稿，离开详情即释放，不写入磁盘。
export interface NeoBotDraftStore {
    instanceId: string;
    values: Map<string, unknown>;
}
export const NeoBotDraftContext = createContext<NeoBotDraftStore | null>(null);
