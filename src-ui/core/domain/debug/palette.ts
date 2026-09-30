// 命令面板（Ctrl+K）里的一行行：分段标题、接口、收藏、「打开目录外的接口」。
//
// 没输入时先给最常回头找的：最近用过的接口、收藏，再是全部接口（按名字）；
// 输入之后按 searchActions 的得分排接口，收藏按名字 / 接口名 / 备注里含不含这段字筛。
// 敲的是一个像接口名的词、目录里又没有同名的，给一行「打开目录外的接口」：调试台允许自由输入接口名
// （NapCat 的 `_async` 变体、新版本才有的接口），面板里也不该卡住。

import type { DebugActionSafety } from '../../ipc/generated/debug/DebugActionSafety';
import type { DebugActionSummary } from '../../ipc/generated/debug/DebugActionSummary';
import type { DebugCollections } from '../../ipc/generated/debug/DebugCollections';
import type { DebugSavedRequest } from '../../ipc/generated/debug/DebugSavedRequest';
import { searchActions } from './catalogView';
import { collectionsView } from './collectionsOps';

export type PaletteRow =
    | { kind: 'header'; key: string; label: string }
    | { kind: 'action'; key: string; action: DebugActionSummary; recent: boolean }
    | {
          kind: 'saved';
          key: string;
          request: DebugSavedRequest;
          /** 目录里查到的分级；目录里没有这个接口是 null */
          safety: DebugActionSafety | null;
          folderName: string | null;
      }
    | { kind: 'free'; key: string; name: string }
    | { kind: 'more'; key: string; text: string };

export type SelectablePaletteRow = Extract<PaletteRow, { kind: 'action' | 'saved' | 'free' }>;

export function isSelectable(row: PaletteRow | undefined): row is SelectablePaletteRow {
    return !!row && (row.kind === 'action' || row.kind === 'saved' || row.kind === 'free');
}

/** 没输入时最近用过的最多列几个、收藏最多列几个（多的提示输入缩小范围） */
export const PALETTE_RECENT_LIMIT = 8;
export const PALETTE_SAVED_LIMIT = 6;
/** 输入之后收藏最多列几个 */
const SAVED_MATCH_LIMIT = 20;

/** 像接口名的词：字母、数字、下划线、点（NapCat 有 `.ocr_image` 这类点开头的） */
const ACTION_NAME = /^\.?[A-Za-z_][\w.]*$/;

export interface PaletteInput {
    actions: readonly DebugActionSummary[];
    /** 收藏还没读到 / 读失败是 null，这时不出收藏那一段 */
    collections: DebugCollections | null;
    query: string;
    /** 最近用过的接口名，新的在前 */
    recent: readonly string[];
}

function savedRows(
    collections: DebugCollections | null,
    byName: Map<string, DebugActionSummary>,
    match: (r: DebugSavedRequest) => boolean,
): Extract<PaletteRow, { kind: 'saved' }>[] {
    if (!collections) return [];
    const view = collectionsView(collections);
    const ordered: Array<{ request: DebugSavedRequest; folderName: string | null }> = [];
    for (const f of view.folders) for (const r of view.children.get(f.id) ?? []) ordered.push({ request: r, folderName: f.name });
    for (const r of view.root) ordered.push({ request: r, folderName: null });
    return ordered
        .filter(({ request }) => match(request))
        .map(({ request, folderName }) => ({
            kind: 'saved' as const,
            key: `s:${request.id}`,
            request,
            safety: byName.get(request.action)?.safety ?? null,
            folderName,
        }));
}

export function buildPaletteRows({ actions, collections, query, recent }: PaletteInput): PaletteRow[] {
    const list = [...actions];
    const byName = new Map<string, DebugActionSummary>();
    for (const a of list) {
        byName.set(a.name, a);
        for (const alias of a.aliases) if (!byName.has(alias)) byName.set(alias, a);
    }
    const recentSet = new Set(recent);
    const actionRow = (a: DebugActionSummary, section: string): PaletteRow => ({
        kind: 'action',
        key: `${section}:${a.name}`,
        action: a,
        recent: recentSet.has(a.name),
    });
    const rows: PaletteRow[] = [];
    const q = query.trim();

    if (q === '') {
        const recentActions: DebugActionSummary[] = [];
        for (const name of recent) {
            const a = byName.get(name);
            if (a && !recentActions.includes(a)) recentActions.push(a);
            if (recentActions.length >= PALETTE_RECENT_LIMIT) break;
        }
        if (recentActions.length > 0) {
            rows.push({ kind: 'header', key: 'h:recent', label: '最近用过' });
            for (const a of recentActions) rows.push(actionRow(a, 'r'));
        }
        const saved = savedRows(collections, byName, () => true);
        if (saved.length > 0) {
            rows.push({ kind: 'header', key: 'h:saved', label: `收藏 · ${saved.length}` });
            rows.push(...saved.slice(0, PALETTE_SAVED_LIMIT));
            if (saved.length > PALETTE_SAVED_LIMIT) {
                rows.push({ kind: 'more', key: 'm:saved', text: `还有 ${saved.length - PALETTE_SAVED_LIMIT} 个收藏，输入名字缩小范围` });
            }
        }
        if (list.length > 0) {
            const shown = new Set(recentActions);
            const rest = searchActions(list, '', []).filter((a) => !shown.has(a));
            rows.push({ kind: 'header', key: 'h:all', label: `全部接口 · ${list.length}` });
            for (const a of rest) rows.push(actionRow(a, 'a'));
        }
        return rows;
    }

    const lower = q.toLowerCase();
    const matched = searchActions(list, q, [...recent]);
    const exact = byName.has(q) || list.some((a) => a.name.toLowerCase() === lower);
    // 搜到了东西时，只有看着像完整接口名（带下划线、点开头）的才另给一行，敲「group」这种片段不必多一行噪音
    const wantFree = !exact && ACTION_NAME.test(q) && (matched.length === 0 || q.includes('_') || q.startsWith('.'));
    const free: PaletteRow | null = wantFree ? { kind: 'free', key: `f:${q}`, name: q } : null;
    if (matched.length > 0 || free) {
        rows.push({ kind: 'header', key: 'h:actions', label: matched.length > 0 ? `接口 · ${matched.length}` : '接口' });
        // 一个都没搜到时「打开目录外的接口」就是第一行，回车直接用；搜到了就放在最后，不抢最像的那个
        if (free && matched.length === 0) rows.push(free);
        for (const a of matched) rows.push(actionRow(a, 'q'));
        if (free && matched.length > 0) rows.push(free);
    }
    const saved = savedRows(
        collections,
        byName,
        (r) =>
            r.name.toLowerCase().includes(lower) ||
            r.action.toLowerCase().includes(lower) ||
            (r.note ?? '').toLowerCase().includes(lower),
    );
    if (saved.length > 0) {
        rows.push({ kind: 'header', key: 'h:saved', label: `收藏 · ${saved.length}` });
        rows.push(...saved.slice(0, SAVED_MATCH_LIMIT));
        if (saved.length > SAVED_MATCH_LIMIT) {
            rows.push({ kind: 'more', key: 'm:saved', text: `还有 ${saved.length - SAVED_MATCH_LIMIT} 个，多输几个字` });
        }
    }
    return rows;
}

/** 从 from 开始往 dir 方向找下一条能选的行；找不到留在原处（到头不绕回，免得一下子跳到另一头） */
export function stepSelectable(rows: readonly PaletteRow[], from: number, dir: 1 | -1): number {
    for (let i = from + dir; i >= 0 && i < rows.length; i += dir) {
        if (isSelectable(rows[i])) return i;
    }
    return from;
}

/** 第一条能选的行；一条都没有是 -1 */
export function firstSelectable(rows: readonly PaletteRow[]): number {
    return rows.findIndex(isSelectable);
}
