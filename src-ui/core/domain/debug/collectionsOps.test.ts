import { describe, expect, it } from 'vitest';
import type { DebugCollections } from '../../ipc/generated/debug/DebugCollections';
import type { DebugSavedRequest } from '../../ipc/generated/debug/DebugSavedRequest';
import {
    addFolder,
    addRequest,
    collectionsView,
    deleteFolder,
    deleteRequest,
    findSameRequest,
    moveFolder,
    moveRequest,
    nudge,
    renameFolder,
    renameRequest,
    suggestedRequestName,
    uniqueName,
} from './collectionsOps';

function req(id: string, folder: string | null, order: number, extra: Partial<DebugSavedRequest> = {}): DebugSavedRequest {
    return {
        id,
        name: id,
        folder_id: folder,
        action: `act_${id}`,
        params: {},
        channel: null,
        note: null,
        order,
        created_at_ms: 1000 + order,
        updated_at_ms: 1000 + order,
        ...extra,
    };
}

const BASE: DebugCollections = {
    version: 1,
    folders: [
        { id: 'f2', name: '第二', order: 1 },
        { id: 'f1', name: '第一', order: 0 },
    ],
    requests: [req('a', 'f1', 0), req('b', 'f1', 1), req('c', 'f1', 2), req('r1', null, 0), req('r2', null, 1), req('orphan', 'gone', 5)],
};

const ids = (list: DebugSavedRequest[]) => list.map((r) => r.id);

describe('collectionsView', () => {
    it('按 order 排好，指向不存在文件夹的请求归到根目录', () => {
        const v = collectionsView(BASE);
        expect(v.folders.map((f) => f.id)).toEqual(['f1', 'f2']);
        expect(ids(v.children.get('f1') ?? [])).toEqual(['a', 'b', 'c']);
        expect(v.children.get('f2')).toEqual([]);
        expect(ids(v.root)).toEqual(['r1', 'r2', 'orphan']);
    });
});

describe('moveRequest', () => {
    it('同一文件夹里往下挪，order 重新编号', () => {
        const next = moveRequest(BASE, 'a', { folderId: 'f1', index: 3 });
        const v = collectionsView(next);
        expect(ids(v.children.get('f1') ?? [])).toEqual(['b', 'c', 'a']);
        expect((v.children.get('f1') ?? []).map((r) => r.order)).toEqual([0, 1, 2]);
    });

    it('同一文件夹里往上挪', () => {
        const v = collectionsView(moveRequest(BASE, 'c', { folderId: 'f1', index: 0 }));
        expect(ids(v.children.get('f1') ?? [])).toEqual(['c', 'a', 'b']);
    });

    it('放回原位（上下各一格）不算改动，返回同一个对象', () => {
        expect(moveRequest(BASE, 'b', { folderId: 'f1', index: 1 })).toBe(BASE);
        expect(moveRequest(BASE, 'b', { folderId: 'f1', index: 2 })).toBe(BASE);
    });

    it('挪进另一个文件夹：改 folder_id，两边都重新编号', () => {
        const next = moveRequest(BASE, 'a', { folderId: 'f2', index: 0 });
        const v = collectionsView(next);
        expect(ids(v.children.get('f2') ?? [])).toEqual(['a']);
        expect(next.requests.find((r) => r.id === 'a')?.folder_id).toBe('f2');
        expect((v.children.get('f1') ?? []).map((r) => [r.id, r.order])).toEqual([
            ['b', 0],
            ['c', 1],
        ]);
    });

    it('从文件夹挪到根目录的指定位置', () => {
        const v = collectionsView(moveRequest(BASE, 'b', { folderId: null, index: 1 }));
        expect(ids(v.root)).toEqual(['r1', 'b', 'r2', 'orphan']);
    });

    it('孤儿请求在根目录里挪动时顺手修正 folder_id', () => {
        const next = moveRequest(BASE, 'orphan', { folderId: null, index: 0 });
        expect(next.requests.find((r) => r.id === 'orphan')?.folder_id).toBeNull();
        expect(ids(collectionsView(next).root)).toEqual(['orphan', 'r1', 'r2']);
    });

    it('目标文件夹不存在、请求不存在时不改', () => {
        expect(moveRequest(BASE, 'a', { folderId: 'nope', index: 0 })).toBe(BASE);
        expect(moveRequest(BASE, 'nope', { folderId: null, index: 0 })).toBe(BASE);
    });

    it('超出的下标就是末尾', () => {
        const v = collectionsView(moveRequest(BASE, 'r1', { folderId: 'f1', index: 99 }));
        expect(ids(v.children.get('f1') ?? [])).toEqual(['a', 'b', 'c', 'r1']);
    });
});

describe('moveFolder / nudge', () => {
    it('文件夹换位', () => {
        const next = moveFolder(BASE, 'f1', 2);
        expect(collectionsView(next).folders.map((f) => f.id)).toEqual(['f2', 'f1']);
        expect(moveFolder(BASE, 'f1', 0)).toBe(BASE);
        expect(moveFolder(BASE, 'f1', 1)).toBe(BASE);
    });

    it('Alt+↑↓ 挪一格，到头了不动', () => {
        expect(ids(collectionsView(nudge(BASE, 'request', 'a', 1)).children.get('f1') ?? [])).toEqual(['b', 'a', 'c']);
        expect(ids(collectionsView(nudge(BASE, 'request', 'b', -1)).children.get('f1') ?? [])).toEqual(['b', 'a', 'c']);
        expect(nudge(BASE, 'request', 'a', -1)).toBe(BASE);
        expect(nudge(BASE, 'request', 'c', 1)).toBe(BASE);
        expect(collectionsView(nudge(BASE, 'folder', 'f1', 1)).folders.map((f) => f.id)).toEqual(['f2', 'f1']);
        expect(nudge(BASE, 'folder', 'f1', -1)).toBe(BASE);
    });
});

describe('改名 / 删除 / 新建', () => {
    it('改名去掉首尾空白；空名和同名不改', () => {
        const next = renameRequest(BASE, 'a', '  看看  登录号 ', 5000);
        const a = next.requests.find((r) => r.id === 'a');
        expect(a?.name).toBe('看看 登录号');
        expect(a?.updated_at_ms).toBe(5000);
        expect(renameRequest(BASE, 'a', '   ', 5000)).toBe(BASE);
        expect(renameRequest(BASE, 'a', 'a', 5000)).toBe(BASE);
        expect(renameFolder(BASE, 'f1', '常用').folders.find((f) => f.id === 'f1')?.name).toBe('常用');
        expect(renameFolder(BASE, 'f1', '')).toBe(BASE);
    });

    it('删文件夹连同里面的请求', () => {
        const next = deleteFolder(BASE, 'f1');
        expect(next.folders.map((f) => f.id)).toEqual(['f2']);
        expect(ids(next.requests)).toEqual(['r1', 'r2', 'orphan']);
        expect(deleteFolder(BASE, 'nope')).toBe(BASE);
        expect(ids(deleteRequest(BASE, 'r1').requests)).not.toContain('r1');
        expect(deleteRequest(BASE, 'nope')).toBe(BASE);
    });

    it('新文件夹放最后、撞名时编号', () => {
        const { next, id } = addFolder(BASE);
        const created = next.folders.find((f) => f.id === id);
        expect(created).toMatchObject({ name: '新建文件夹', order: 2 });
        const again = addFolder(next);
        expect(again.next.folders.find((f) => f.id === again.id)?.name).toBe('新建文件夹 2');
        expect(uniqueName(['x', 'x 2'], 'x')).toBe('x 3');
    });

    it('新收藏放在目标容器末尾；目标文件夹不存在就进根目录', () => {
        const { next, id } = addRequest(BASE, { name: '', action: 'get_login_info', params: { a: 1 }, channel: null, folderId: 'f1' }, 9000);
        const created = next.requests.find((r) => r.id === id);
        expect(created).toMatchObject({ name: 'get_login_info', folder_id: 'f1', order: 3, created_at_ms: 9000 });
        const other = addRequest(BASE, { name: 'x', action: 'y', params: {}, channel: null, folderId: 'gone' }, 1);
        expect(other.next.requests.find((r) => r.id === other.id)?.folder_id).toBeNull();
    });

    it('同一接口同样参数（键顺序不同）算已收藏', () => {
        const c: DebugCollections = { ...BASE, requests: [req('s', null, 0, { action: 'send_group_msg', params: { group_id: 1, message: 'hi' } })] };
        expect(findSameRequest(c, 'send_group_msg', { message: 'hi', group_id: 1 })?.id).toBe('s');
        expect(findSameRequest(c, 'send_group_msg', { message: 'yo', group_id: 1 })).toBeNull();
    });

    it('收藏名默认值：简介短就带上简介，长了或没有就是接口名', () => {
        expect(suggestedRequestName('send_group_msg', '发送群消息')).toBe('发送群消息（send_group_msg）');
        expect(suggestedRequestName('get_x', '  ')).toBe('get_x');
        expect(suggestedRequestName('get_x', null)).toBe('get_x');
        expect(suggestedRequestName('get_x', '很长很长很长很长很长很长很长很长很长很长很长很长的简介')).toBe('get_x');
    });
});
