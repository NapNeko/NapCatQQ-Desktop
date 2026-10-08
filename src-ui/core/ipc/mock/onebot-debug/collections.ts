// 工作区与收藏夹的内存存储：出厂种子数据 + 导入收藏时的 id 去重合并。

import type { DebugCollections } from '../../generated/debug/DebugCollections';
import type { DebugWorkspace } from '../../generated/debug/DebugWorkspace';
import { SEED_EPOCH_MS } from './consts';

// ---------------------------------------------------------------------------
// 工作区 / 收藏
// ---------------------------------------------------------------------------

export const defaultWorkspace = (): DebugWorkspace => ({
    version: 1,
    tabs: [],
    active_tab: null,
    closed_tabs: [],
    selected_bot: null,
    channel_choice: {},
    layout: {
        left_collapsed: false,
        right_collapsed: false,
        left_width: 240,
        right_width: 380,
        right_view: 'chat',
    },
    recent_actions: [],
});

export const seedCollections = (): DebugCollections => ({
    version: 1,
    folders: [{ id: 'mock-folder-common', name: '常用', order: 0 }],
    requests: [
        {
            id: 'mock-req-login',
            name: '看看登录号',
            folder_id: 'mock-folder-common',
            action: 'get_login_info',
            params: {},
            channel: null,
            note: null,
            order: 0,
            created_at_ms: SEED_EPOCH_MS,
            updated_at_ms: SEED_EPOCH_MS,
        },
        {
            id: 'mock-req-hello',
            name: '测试群打招呼',
            folder_id: 'mock-folder-common',
            action: 'send_group_msg',
            params: { group_id: '100001', message: '大家好，这是一条调试消息' },
            channel: null,
            note: '发到测试群 1',
            order: 1,
            created_at_ms: SEED_EPOCH_MS + 60_000,
            updated_at_ms: SEED_EPOCH_MS + 60_000,
        },
        {
            id: 'mock-req-groups',
            name: '群列表（走内部通道）',
            folder_id: null,
            action: 'get_group_list',
            params: {},
            channel: { kind: 'internal' },
            note: null,
            order: 0,
            created_at_ms: SEED_EPOCH_MS + 120_000,
            updated_at_ms: SEED_EPOCH_MS + 120_000,
        },
    ],
});

export let workspaceStore: DebugWorkspace = defaultWorkspace();
export let collectionsStore: DebugCollections = seedCollections();

export function mergeCollections(incoming: DebugCollections): void {
    const folderIds = new Set(collectionsStore.folders.map((f) => f.id));
    const requestIds = new Set(collectionsStore.requests.map((r) => r.id));
    const folderMap = new Map<string, string>();
    const folderBase = collectionsStore.folders.reduce((m, f) => Math.max(m, f.order + 1), 0);
    incoming.folders.forEach((f, i) => {
        let id = f.id;
        for (let n = 2; folderIds.has(id); n += 1) id = `${f.id}-${n}`;
        folderIds.add(id);
        folderMap.set(f.id, id);
        collectionsStore.folders.push({ ...f, id, order: folderBase + i });
    });
    incoming.requests.forEach((r) => {
        let id = r.id;
        for (let n = 2; requestIds.has(id); n += 1) id = `${r.id}-${n}`;
        requestIds.add(id);
        collectionsStore.requests.push({
            ...r,
            id,
            folder_id: r.folder_id === null ? null : (folderMap.get(r.folder_id) ?? null),
        });
    });
}

export function replaceWorkspace(workspace: DebugWorkspace): void {
    workspaceStore = workspace;
}

export function replaceCollections(collections: DebugCollections): void {
    collectionsStore = collections;
}

/** 测试用：回到出厂的工作区与收藏 */
export function resetCollectionsMock(): void {
    workspaceStore = defaultWorkspace();
    collectionsStore = seedCollections();
}
