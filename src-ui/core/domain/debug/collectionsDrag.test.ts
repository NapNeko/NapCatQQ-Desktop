import { describe, expect, it } from 'vitest';
import type { DebugCollections } from '../../ipc/generated/debug/DebugCollections';
import type { DebugSavedRequest } from '../../ipc/generated/debug/DebugSavedRequest';
import { applyDrop, sameTarget, type DragSource, type DropTarget } from './collectionsDrag';
import { collectionsView } from './collectionsOps';

function req(
    id: string,
    folder: string | null,
    order: number,
    extra: Partial<DebugSavedRequest> = {},
): DebugSavedRequest {
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
        { id: 'f1', name: '第一', order: 0 },
        { id: 'f2', name: '第二', order: 1 },
    ],
    requests: [req('a', 'f1', 0), req('b', 'f1', 1), req('r1', null, 0), req('r2', null, 1)],
};

const src = (kind: 'request' | 'folder', id: string): DragSource => ({ kind, id, label: id });
const slot = (folderId: string | null, index: number): DropTarget => ({
    kind: 'slot',
    slot: { folderId, index },
    y: 0,
    nested: folderId !== null,
});
const folderSlot = (index: number): DropTarget => ({ kind: 'folder-slot', index, y: 0 });

describe('applyDrop', () => {
    it('请求落到根区指定位置：改 folder_id 并按目标容器重排', () => {
        const next = applyDrop(BASE, src('request', 'a'), slot(null, 1));
        expect(next).not.toBe(BASE);
        const v = collectionsView(next);
        expect(v.root.map((r) => r.id)).toEqual(['r1', 'a', 'r2']);
        expect(v.children.get('f1')!.map((r) => r.id)).toEqual(['b']);
        expect(next.requests.find((r) => r.id === 'a')!.folder_id).toBeNull();
    });

    it('请求拖进文件夹：追加到该文件夹末尾', () => {
        const next = applyDrop(BASE, src('request', 'r1'), { kind: 'into', folderId: 'f1' });
        const v = collectionsView(next);
        expect(v.children.get('f1')!.map((r) => r.id)).toEqual(['a', 'b', 'r1']);
        expect(v.root.map((r) => r.id)).toEqual(['r2']);
    });

    it('文件夹拖到另一个文件夹上：不是合法落点，原对象返回', () => {
        const next = applyDrop(BASE, src('folder', 'f1'), { kind: 'into', folderId: 'f2' });
        expect(next).toBe(BASE);
    });

    it('文件夹拖到文件夹槽位：按新下标排序', () => {
        const next = applyDrop(BASE, src('folder', 'f1'), folderSlot(2));
        expect(collectionsView(next).folders.map((f) => f.id)).toEqual(['f2', 'f1']);
    });

    it('同位拖放：返回同一个对象，调用方据此跳过保存', () => {
        expect(applyDrop(BASE, src('request', 'a'), slot('f1', 0))).toBe(BASE);
        expect(applyDrop(BASE, src('request', 'a'), slot('f1', 1))).toBe(BASE);
        // into = 挪到该文件夹末尾；末尾元素拖进自己的容器才是同位
        expect(applyDrop(BASE, src('request', 'b'), { kind: 'into', folderId: 'f1' })).toBe(BASE);
        expect(applyDrop(BASE, src('folder', 'f1'), folderSlot(0))).toBe(BASE);
        expect(applyDrop(BASE, src('folder', 'f1'), folderSlot(1))).toBe(BASE);
    });

    it('非末尾请求拖进自己所在文件夹 = 挪到末尾', () => {
        const next = applyDrop(BASE, src('request', 'a'), { kind: 'into', folderId: 'f1' });
        expect(next).not.toBe(BASE);
        expect(
            collectionsView(next)
                .children.get('f1')!
                .map((r) => r.id),
        ).toEqual(['b', 'a']);
    });

    it('请求拖到 folder-slot、文件夹拖到 slot：跨类型落点都不生效', () => {
        expect(applyDrop(BASE, src('request', 'r1'), folderSlot(0))).toBe(BASE);
        expect(applyDrop(BASE, src('folder', 'f2'), slot(null, 0))).toBe(BASE);
    });
});

describe('sameTarget', () => {
    it('null 只和 null 相等', () => {
        expect(sameTarget(null, null)).toBe(true);
        expect(sameTarget(null, slot(null, 0))).toBe(false);
        expect(sameTarget(slot(null, 0), null)).toBe(false);
    });

    it('slot：容器、下标、指示线位置都相同才算同一个', () => {
        expect(sameTarget(slot('f1', 1), slot('f1', 1))).toBe(true);
        expect(sameTarget(slot('f1', 1), slot(null, 1))).toBe(false);
        expect(sameTarget(slot('f1', 1), slot('f1', 2))).toBe(false);
        expect(
            sameTarget(
                { kind: 'slot', slot: { folderId: 'f1', index: 1 }, y: 30, nested: true },
                { kind: 'slot', slot: { folderId: 'f1', index: 1 }, y: 60, nested: true },
            ),
        ).toBe(false);
    });

    it('into：只看目标文件夹', () => {
        expect(sameTarget({ kind: 'into', folderId: 'f1' }, { kind: 'into', folderId: 'f1' })).toBe(
            true,
        );
        expect(sameTarget({ kind: 'into', folderId: 'f1' }, { kind: 'into', folderId: 'f2' })).toBe(
            false,
        );
    });

    it('跨 kind 一律不等', () => {
        expect(sameTarget(slot(null, 0), { kind: 'into', folderId: 'f1' })).toBe(false);
        expect(sameTarget(folderSlot(0), slot(null, 0))).toBe(false);
    });
});
