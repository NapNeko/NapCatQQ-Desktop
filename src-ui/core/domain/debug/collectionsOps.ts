// 收藏的整理：排序后的视图，和移动 / 改名 / 删除 / 新建这些整份替换式的改动。
//
// 收藏按整份保存（useSaveCollections），所以每个改动都是「旧的一整份 → 新的一整份」的纯函数；
// 没有实际变化时原样返回同一个对象，调用方据此跳过保存。`order` 只在同一个容器（某个文件夹或根目录）
// 里比较，移动后把受影响容器的 order 重新编成 0..n-1，不留空洞也不留重号。

import { newRequestId } from './ids';
import type { DebugChannelId } from '../../ipc/generated/debug/DebugChannelId';
import type { DebugCollections } from '../../ipc/generated/debug/DebugCollections';
import type { DebugSavedFolder } from '../../ipc/generated/debug/DebugSavedFolder';
import type { DebugSavedRequest } from '../../ipc/generated/debug/DebugSavedRequest';

export interface CollectionsView {
    folders: DebugSavedFolder[];
    /** 文件夹 id → 里面的请求（已排序） */
    children: Map<string, DebugSavedRequest[]>;
    /** 根目录的请求；指向已不存在文件夹的请求也算在这里，别让它们凭空消失 */
    root: DebugSavedRequest[];
}

/** 名字最长这么多字：再长列表里也显示不下，导入的奇怪文件也不至于撑坏界面 */
export const MAX_NAME_LENGTH = 80;

function compareRequests(a: DebugSavedRequest, b: DebugSavedRequest): number {
    return (
        a.order - b.order ||
        a.created_at_ms - b.created_at_ms ||
        (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
    );
}

function compareFolders(a: DebugSavedFolder, b: DebugSavedFolder): number {
    return a.order - b.order || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
}

export function collectionsView(c: DebugCollections): CollectionsView {
    const folders = [...c.folders].sort(compareFolders);
    const known = new Set(folders.map((f) => f.id));
    const children = new Map<string, DebugSavedRequest[]>(folders.map((f) => [f.id, []]));
    const root: DebugSavedRequest[] = [];
    for (const r of c.requests) {
        const bucket =
            r.folder_id !== null && known.has(r.folder_id) ? children.get(r.folder_id) : undefined;
        if (bucket) bucket.push(r);
        else root.push(r);
    }
    for (const list of children.values()) list.sort(compareRequests);
    root.sort(compareRequests);
    return { folders, children, root };
}

/** 请求所在的容器：文件夹 id，或 null 表示根目录（含孤儿） */
function containerOf(
    view: CollectionsView,
    id: string,
): { folderId: string | null; list: DebugSavedRequest[] } | null {
    const inRoot = view.root.findIndex((r) => r.id === id);
    if (inRoot >= 0) return { folderId: null, list: view.root };
    for (const [folderId, list] of view.children) {
        if (list.some((r) => r.id === id)) return { folderId, list };
    }
    return null;
}

export interface RequestSlot {
    /** 目标容器：文件夹 id，null 是根目录 */
    folderId: string | null;
    /** 插在目标容器的第几个位置之前（按「被拖的那条还在原处」时的下标算），超出就是末尾 */
    index: number;
}

/** 把一条请求挪到某个容器的某个位置 */
export function moveRequest(c: DebugCollections, id: string, to: RequestSlot): DebugCollections {
    const view = collectionsView(c);
    const from = containerOf(view, id);
    if (!from) return c;
    if (to.folderId !== null && !view.children.has(to.folderId)) return c;
    const moving = from.list.find((r) => r.id === id);
    if (!moving) return c;

    const fromIndex = from.list.indexOf(moving);
    const sameContainer = from.folderId === to.folderId;
    const targetList = sameContainer
        ? from.list
        : ((to.folderId === null ? view.root : view.children.get(to.folderId)) ?? []);
    let index = Math.max(0, Math.min(to.index, targetList.length));
    if (sameContainer) {
        // 原位上下各一格都是「没动」
        if (index === fromIndex || index === fromIndex + 1) return c;
        if (index > fromIndex) index -= 1;
    }

    const without = (list: DebugSavedRequest[]) => list.filter((r) => r.id !== id);
    const nextTarget = sameContainer ? without(targetList) : [...targetList];
    nextTarget.splice(index, 0, { ...moving, folder_id: to.folderId });

    const patch = new Map<string, DebugSavedRequest>();
    const renumber = (list: DebugSavedRequest[]) =>
        list.forEach((r, i) => {
            if (r.order !== i || r.id === id) patch.set(r.id, { ...r, order: i });
        });
    renumber(nextTarget);
    if (!sameContainer) renumber(without(from.list));

    return { ...c, requests: c.requests.map((r) => patch.get(r.id) ?? r) };
}

/** 把一个文件夹挪到第 index 个位置之前（按它还在原处时的下标算） */
export function moveFolder(c: DebugCollections, id: string, index: number): DebugCollections {
    const { folders } = collectionsView(c);
    const fromIndex = folders.findIndex((f) => f.id === id);
    if (fromIndex < 0) return c;
    let to = Math.max(0, Math.min(index, folders.length));
    if (to === fromIndex || to === fromIndex + 1) return c;
    if (to > fromIndex) to -= 1;
    const next = folders.filter((f) => f.id !== id);
    next.splice(to, 0, folders[fromIndex]);
    const order = new Map(next.map((f, i) => [f.id, i]));
    return {
        ...c,
        folders: c.folders.map((f) =>
            order.get(f.id) === f.order ? f : { ...f, order: order.get(f.id) ?? f.order },
        ),
    };
}

/** 键盘上移 / 下移一格：请求只在自己的容器里挪，文件夹在文件夹之间挪 */
export function nudge(
    c: DebugCollections,
    kind: 'request' | 'folder',
    id: string,
    dir: -1 | 1,
): DebugCollections {
    const view = collectionsView(c);
    if (kind === 'folder') {
        const idx = view.folders.findIndex((f) => f.id === id);
        if (idx < 0) return c;
        return moveFolder(c, id, dir < 0 ? idx - 1 : idx + 2);
    }
    const from = containerOf(view, id);
    if (!from) return c;
    const idx = from.list.findIndex((r) => r.id === id);
    if (dir < 0 && idx === 0) return c;
    return moveRequest(c, id, { folderId: from.folderId, index: dir < 0 ? idx - 1 : idx + 2 });
}

function cleanName(name: string): string {
    return name.trim().replace(/\s+/g, ' ').slice(0, MAX_NAME_LENGTH);
}

/** 空名字不改（保持原名）；同名不算改动 */
export function renameRequest(
    c: DebugCollections,
    id: string,
    name: string,
    nowMs: number,
): DebugCollections {
    const clean = cleanName(name);
    if (!clean) return c;
    let changed = false;
    const requests = c.requests.map((r) => {
        if (r.id !== id || r.name === clean) return r;
        changed = true;
        return { ...r, name: clean, updated_at_ms: nowMs };
    });
    return changed ? { ...c, requests } : c;
}

export function renameFolder(c: DebugCollections, id: string, name: string): DebugCollections {
    const clean = cleanName(name);
    if (!clean) return c;
    let changed = false;
    const folders = c.folders.map((f) => {
        if (f.id !== id || f.name === clean) return f;
        changed = true;
        return { ...f, name: clean };
    });
    return changed ? { ...c, folders } : c;
}

export function deleteRequest(c: DebugCollections, id: string): DebugCollections {
    return c.requests.some((r) => r.id === id)
        ? { ...c, requests: c.requests.filter((r) => r.id !== id) }
        : c;
}

/** 删文件夹连同里面的请求一起删（确认框里写明了数量） */
export function deleteFolder(c: DebugCollections, id: string): DebugCollections {
    if (!c.folders.some((f) => f.id === id)) return c;
    return {
        ...c,
        folders: c.folders.filter((f) => f.id !== id),
        requests: c.requests.filter((r) => r.folder_id !== id),
    };
}

/** 「新建文件夹」「新建文件夹 2」…：撞名时往后编号 */
export function uniqueName(existing: readonly string[], base: string): string {
    const taken = new Set(existing);
    if (!taken.has(base)) return base;
    for (let n = 2; ; n += 1) {
        const candidate = `${base} ${n}`;
        if (!taken.has(candidate)) return candidate;
    }
}

/** 新文件夹放在所有文件夹的最后 */
export function addFolder(
    c: DebugCollections,
    name?: string,
): { next: DebugCollections; id: string } {
    const id = newRequestId();
    const order = c.folders.reduce((max, f) => Math.max(max, f.order + 1), 0);
    const folderName =
        cleanName(name ?? '') ||
        uniqueName(
            c.folders.map((f) => f.name),
            '新建文件夹',
        );
    return { next: { ...c, folders: [...c.folders, { id, name: folderName, order }] }, id };
}

export interface NewSavedRequest {
    name: string;
    action: string;
    params: unknown;
    channel: DebugChannelId | null;
    note?: string | null;
    folderId?: string | null;
}

/** 新收藏放在目标容器的末尾 */
export function addRequest(
    c: DebugCollections,
    draft: NewSavedRequest,
    nowMs: number,
): { next: DebugCollections; id: string } {
    const id = newRequestId();
    const folderId =
        draft.folderId && c.folders.some((f) => f.id === draft.folderId) ? draft.folderId : null;
    const view = collectionsView(c);
    const siblings = folderId === null ? view.root : (view.children.get(folderId) ?? []);
    const order = siblings.reduce((max, r) => Math.max(max, r.order + 1), 0);
    const request: DebugSavedRequest = {
        id,
        name: cleanName(draft.name) || draft.action,
        folder_id: folderId,
        action: draft.action,
        params: draft.params ?? {},
        channel: draft.channel,
        note: draft.note ?? null,
        order,
        created_at_ms: nowMs,
        updated_at_ms: nowMs,
    };
    return { next: { ...c, requests: [...c.requests, request] }, id };
}

/** 键按字母排的 JSON：`{a,b}` 和 `{b,a}` 算同一份参数 */
function stableJson(v: unknown): string {
    if (Array.isArray(v)) return `[${v.map(stableJson).join(',')}]`;
    if (v && typeof v === 'object') {
        const obj = v as Record<string, unknown>;
        return `{${Object.keys(obj)
            .sort()
            .map((k) => `${JSON.stringify(k)}:${stableJson(obj[k])}`)
            .join(',')}}`;
    }
    return JSON.stringify(v) ?? 'null';
}

/** 同一个动作、同样的参数已经收藏过了就返回那一条，避免从历史里点两下存出两份 */
export function findSameRequest(
    c: DebugCollections,
    action: string,
    params: unknown,
): DebugSavedRequest | null {
    const key = stableJson(params ?? {});
    return (
        c.requests.find((r) => r.action === action && stableJson(r.params ?? {}) === key) ?? null
    );
}

/** 收藏时名字框里的默认值：简介够短就「简介（接口名）」，一眼认得出是干什么的；否则就是接口名 */
export function suggestedRequestName(action: string, summary: string | null | undefined): string {
    const s = summary?.trim().replace(/\s+/g, ' ');
    const name = s && s.length <= 24 ? `${s}（${action}）` : action;
    return name.slice(0, MAX_NAME_LENGTH);
}
