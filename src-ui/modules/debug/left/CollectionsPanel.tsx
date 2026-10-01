// 左栏「收藏」：存下来的请求，文件夹即集合。
//
// 单击打开（没动过的当前标签直接顶替，Ctrl / ⌘ 或中键另开）；▶ 在当前 Bot 上发（先打开成标签再发，
// 响应照常显示在中栏，危险接口先确认）。拖动排序、拖进 / 拖出文件夹，全用指针事件自己做：
// 按下移动超过几像素才算拖，松手前按 Esc 放弃；拖到列表上下边缘自动滚。键盘上 Alt+↑↓ 同样能挪位置。
// 每个改动都是整份替换保存（乐观更新，界面立刻跟手，失败会退回并弹条）。

import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { FileInput, FileOutput, FolderPlus, GripVertical, RefreshCw, Star } from 'lucide-react';
import { cn } from '../../../shared/utils/cn';
import { Button } from '../../../shared/ui';
import { BodyPortal } from '../../../shared/ui/BodyPortal';
import {
    useDebugCollections,
    useExportCollections,
    useImportCollections,
    useSaveCollections,
} from '../../../hooks/debug/useDebugCollections';
import { useDebugCatalog } from '../../../hooks/debug/useDebugCatalog';
import { useDebugCall } from '../../../hooks/debug/useDebugCall';
import { debugWorkspaceStore, useDebugChannelChoice } from '../../../hooks/debug/debugWorkspaceStore';
import { useScrollMemory } from '../../../hooks/debug/debugScrollMemory';
import { AUTO_CHANNEL } from '../../../core/domain/debug/channelPick';
import { lookupSummary } from '../../../core/domain/debug/catalogView';
import { countOmittedInValue } from '../../../core/domain/debug/omittedParams';
import { targetDisplayName } from '../../../core/domain/debug/targetGroups';
import type { DebugCollections } from '../../../core/ipc/generated/debug/DebugCollections';
import type { DebugSavedRequest } from '../../../core/ipc/generated/debug/DebugSavedRequest';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import { COLUMN_HEADER_CLASS } from '../ColumnFrame';
import { DangerConfirmDialog, dangerConfirmSkipped } from '../DangerConfirmDialog';
import { COLLECTION_ROW_HEIGHT, FolderRow, RequestRow } from './CollectionRows';
import {
    addFolder,
    collectionsView,
    deleteFolder,
    deleteRequest,
    moveFolder,
    moveRequest,
    nudge,
    renameFolder,
    renameRequest,
    type CollectionsView,
    type RequestSlot,
} from '../../../core/domain/debug/collectionsOps';
import { paramsTextOf } from '../../../core/domain/debug/historyReplay';
import { ConfirmDialog, IconAction, PanelMessage, SkeletonRows, type ConfirmRequest } from './panelParts';
import { useFlip } from './useFlip';

// 收起了哪些文件夹：界面状态，不落盘
let savedClosedFolders: ReadonlySet<string> = new Set();

/** 按下后移动超过这么多像素才算开始拖，免得手抖把单击变成拖动 */
const DRAG_THRESHOLD_PX = 5;
/** 离列表上下边缘这么近时自动滚动 */
const AUTO_SCROLL_EDGE_PX = 32;
const AUTO_SCROLL_MAX_SPEED = 14;

type DragSource = { kind: 'request' | 'folder'; id: string; label: string };

type DropTarget =
    | { kind: 'slot'; slot: RequestSlot; y: number; nested: boolean }
    | { kind: 'into'; folderId: string }
    | { kind: 'folder-slot'; index: number; y: number };

interface DragState {
    pointerId: number;
    startX: number;
    startY: number;
    x: number;
    y: number;
    source: DragSource;
    started: boolean;
    target: DropTarget | null;
    scrollFrame: number;
}

function sameTarget(a: DropTarget | null, b: DropTarget | null): boolean {
    if (a === null || b === null) return a === b;
    if (a.kind !== b.kind) return false;
    if (a.kind === 'into' && b.kind === 'into') return a.folderId === b.folderId;
    if (a.kind === 'slot' && b.kind === 'slot') return a.slot.folderId === b.slot.folderId && a.slot.index === b.slot.index && a.y === b.y;
    if (a.kind === 'folder-slot' && b.kind === 'folder-slot') return a.index === b.index && a.y === b.y;
    return false;
}

/** 把落点换成新的整份收藏；落点等于原位时返回原对象 */
function applyDrop(c: DebugCollections, source: DragSource, target: DropTarget): DebugCollections {
    if (source.kind === 'folder') return target.kind === 'folder-slot' ? moveFolder(c, source.id, target.index) : c;
    if (target.kind === 'slot') return moveRequest(c, source.id, target.slot);
    if (target.kind === 'into') return moveRequest(c, source.id, { folderId: target.folderId, index: Number.MAX_SAFE_INTEGER });
    return c;
}

export const CollectionsPanel = memo(function CollectionsPanel({ target }: { target: DebugTarget | null }) {
    const listRef = useRef<HTMLDivElement>(null);
    const contentRef = useRef<HTMLDivElement>(null);
    const ghostRef = useRef<HTMLDivElement>(null);

    const collectionsQuery = useDebugCollections();
    const data = collectionsQuery.data;
    const save = useSaveCollections();
    const importFile = useImportCollections();
    const exportFile = useExportCollections();
    const catalogQuery = useDebugCatalog(target);
    const catalog = catalogQuery.data;
    const choice = useDebugChannelChoice(target?.bot_id ?? null);
    const { send } = useDebugCall();

    const view: CollectionsView | null = useMemo(() => (data ? collectionsView(data) : null), [data]);

    const [closed, setClosedState] = useState<ReadonlySet<string>>(savedClosedFolders);
    const [editing, setEditing] = useState<{ kind: 'request' | 'folder'; id: string } | null>(null);
    const [confirm, setConfirm] = useState<ConfirmRequest | null>(null);
    const [focusKey, setFocusKey] = useState<string | null>(null);

    useScrollMemory('collections', listRef);
    // 数据换了或折叠状态变了都可能让行换位置
    const layoutVersion = useMemo(() => ({ data, closed }), [data, closed]);
    const flip = useFlip(listRef, layoutVersion);

    // 保存都经这里：没变化不存；换位前记下行的位置，数据换了之后滑过去
    const dataRef = useRef(data);
    dataRef.current = data;
    const commit = useCallback(
        (fn: (c: DebugCollections) => DebugCollections) => {
            const current = dataRef.current;
            if (!current) return;
            const next = fn(current);
            if (next === current) return;
            flip();
            save.mutate(next);
        },
        [flip, save],
    );

    const closedRef = useRef(closed);
    closedRef.current = closed;
    const setFolderOpen = useCallback(
        (id: string, open: boolean) => {
            const prev = closedRef.current;
            if (prev.has(id) === !open) return;
            flip();
            const next = new Set(prev);
            if (open) next.delete(id);
            else next.add(id);
            savedClosedFolders = next;
            closedRef.current = next;
            setClosedState(next);
        },
        [flip],
    );
    const toggleFolder = useCallback((id: string) => setFolderOpen(id, closed.has(id)), [closed, setFolderOpen]);

    // ---- 打开 / 发送
    const openSaved = useCallback((req: DebugSavedRequest, newTab: boolean): string => {
        const tabId = debugWorkspaceStore.openAction(req.action, { newTab, paramsText: paramsTextOf(req.params) });
        debugWorkspaceStore.setTabChannel(tabId, req.channel);
        return tabId;
    }, []);

    const sendDisabledReason = useCallback(
        (req: DebugSavedRequest): string | null => {
            if (!target) return '先在顶栏选一个 Bot';
            if (!target.running) return 'Bot 没在运行';
            // 分级要靠目录判断，没读出来之前不让一键发（读失败时照样能发，但会先确认）
            if (!catalog && catalogQuery.isPending) return '接口目录还没读出来';
            // 老收藏里可能夹着存盘时瘦身留下的占位（超长字符串 / 整份摘要）：发出去的只是占位文字，
            // 打开补全原文再来一键发
            const omitted = countOmittedInValue(req.params);
            if (omitted > 0) return `有 ${omitted} 处超长参数在存盘时被省略，打开补全后再发`;
            return null;
        },
        [target, catalog, catalogQuery.isPending],
    );

    const doSend = useCallback(
        (req: DebugSavedRequest) => {
            if (!target || !target.running) return;
            const tabId = openSaved(req, false);
            void send(tabId, {
                bot_id: target.bot_id,
                channel: req.channel ?? choice?.call ?? AUTO_CHANNEL,
                action: req.action,
                params: req.params ?? {},
                timeout_ms: null,
                origin: 'editor',
            });
        },
        [target, choice, openSaved, send],
    );

    // 一键发送前的确认：和中栏发送用同一个确认框、同一份「本次不再询问」
    const [sendConfirm, setSendConfirm] = useState<{ req: DebugSavedRequest; reason?: string; summaryFrom?: string | null } | null>(null);
    const [sendConfirmOpen, setSendConfirmOpen] = useState(false);

    const onSend = useCallback(
        (req: DebugSavedRequest) => {
            if (sendDisabledReason(req) || !target) return;
            // 分级查法和中栏同一套：名字、别名、还有 NapCat 的 _async / _rate_limited 变体，都按原接口认
            const { summary, summaryFrom } = lookupSummary(catalog?.actions, req.action);
            const safety = summary?.safety ?? null;
            if (safety === 'read_only' || safety === 'side_effect' || dangerConfirmSkipped(target.bot_id, req.action)) {
                doSend(req);
                return;
            }
            // 查不到分级（目录读失败、目录里没有这个接口）时宁可多问一句：一键发送比在编辑器里发更容易点错
            const reason =
                safety === 'dangerous'
                    ? undefined
                    : `${catalog ? '当前 Bot 的接口目录里没有这个接口' : '接口目录没读出来'}，没法判断它会不会造成难以撤销的后果`;
            setSendConfirm({ req, reason, summaryFrom });
            setSendConfirmOpen(true);
        },
        [sendDisabledReason, target, catalog, doSend],
    );

    // ---- 改名 / 删除 / 新建 / 移动
    const navRows = () => Array.from(listRef.current?.querySelectorAll<HTMLElement>('[data-nav]') ?? []);
    const navRow = (key: string) => listRef.current?.querySelector<HTMLElement>(`[data-nav="${key}"]`) ?? null;
    /** 下一帧（行已经按新状态画好）把焦点放回某一行 */
    const focusRowSoon = useCallback((key: string) => {
        requestAnimationFrame(() => listRef.current?.querySelector<HTMLElement>(`[data-nav="${key}"]`)?.focus());
    }, []);

    /**
     * 从键盘删一行后焦点去哪：没删（取消了）回到这一行；删了就落到下一行（没有就上一行），
     * 删文件夹时跳过它里面的请求
     */
    const focusAfterDelete = (key: string): (() => HTMLElement | null) => {
        const rows = navRows();
        const idx = rows.findIndex((r) => r.dataset.nav === key);
        const block = key.startsWith('f:') ? listRef.current?.querySelector(`[data-dnd-block="${key.slice(2)}"]`) : null;
        const outside = (r: HTMLElement) => r.dataset.nav !== key && !(block && block.contains(r));
        const neighbor = (rows.slice(idx + 1).find(outside) ?? rows.slice(0, Math.max(0, idx)).reverse().find(outside))?.dataset.nav;
        return () => navRow(key) ?? (neighbor ? navRow(neighbor) : null) ?? navRows()[0] ?? null;
    };

    const startRename = useCallback((kind: 'request' | 'folder', id: string) => setEditing({ kind, id }), []);
    const cancelRename = useCallback(
        (kind: 'request' | 'folder', id: string) => {
            setEditing(null);
            focusRowSoon(`${kind === 'folder' ? 'f' : 'r'}:${id}`);
        },
        [focusRowSoon],
    );
    /** byKey：回车提交的焦点回到行上；失焦提交的（点了别处）不抢焦点 */
    const commitRename = useCallback(
        (kind: 'request' | 'folder', id: string, name: string, byKey: boolean) => {
            setEditing(null);
            commit((c) => (kind === 'folder' ? renameFolder(c, id, name) : renameRequest(c, id, name, Date.now())));
            if (byKey) focusRowSoon(`${kind === 'folder' ? 'f' : 'r'}:${id}`);
        },
        [commit, focusRowSoon],
    );

    const askDeleteRequest = useCallback(
        (req: DebugSavedRequest, fromKeyboard = false) =>
            setConfirm({
                title: '删除这个收藏？',
                description: `「${req.name}」（${req.action}）会被删掉，不能撤销。`,
                confirmLabel: '删除',
                onConfirm: () => commit((c) => deleteRequest(c, req.id)),
                restoreFocus: fromKeyboard ? focusAfterDelete(`r:${req.id}`) : undefined,
            }),
        // focusAfterDelete 只读 DOM 和 ref，不随渲染变
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [commit],
    );

    const askDeleteFolder = useCallback(
        (id: string, fromKeyboard = false) => {
            const folder = view?.folders.find((f) => f.id === id);
            if (!folder) return;
            const count = view?.children.get(id)?.length ?? 0;
            setConfirm({
                title: '删除这个文件夹？',
                description:
                    count > 0
                        ? `「${folder.name}」和里面的 ${count} 个请求会一起删掉，不能撤销。想留下请求的话，先把它们拖出来。`
                        : `空文件夹「${folder.name}」会被删掉。`,
                confirmLabel: '删除',
                onConfirm: () => commit((c) => deleteFolder(c, id)),
                restoreFocus: fromKeyboard ? focusAfterDelete(`f:${id}`) : undefined,
            });
        },
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [view, commit],
    );

    const newFolder = () => {
        const current = dataRef.current;
        if (!current) return;
        const { next, id } = addFolder(current);
        flip();
        save.mutate(next);
        setEditing({ kind: 'folder', id });
    };

    const moveTo = useCallback(
        (id: string, folderId: string | null) => {
            if (folderId) setFolderOpen(folderId, true);
            commit((c) => moveRequest(c, id, { folderId, index: Number.MAX_SAFE_INTEGER }));
        },
        [commit, setFolderOpen],
    );

    // ---- 拖动
    const drag = useRef<DragState | null>(null);
    const [dragView, setDragView] = useState<{ source: DragSource; target: DropTarget | null } | null>(null);
    const ignoreClickUntil = useRef(0);
    const awaitingRelease = useRef(false);
    const shouldIgnoreClick = useCallback(
        () => awaitingRelease.current || performance.now() < ignoreClickUntil.current,
        [],
    );
    /** 松手后浏览器可能还会补一个 click，别让它把请求打开：从松手那一刻起算一小段 */
    const suppressClickAfterRelease = (released: boolean) => {
        if (released) {
            ignoreClickUntil.current = performance.now() + 250;
            return;
        }
        // 按 Esc 放弃、指针捕获被抢走时手还按着：等真正松手再开始算
        if (awaitingRelease.current) return;
        awaitingRelease.current = true;
        // 触屏、笔、被系统手势打断时收到的是 pointercancel 而不是 pointerup，两个都算松手
        const done = () => {
            window.removeEventListener('pointerup', done, true);
            window.removeEventListener('pointercancel', done, true);
            window.clearTimeout(fallback);
            awaitingRelease.current = false;
            ignoreClickUntil.current = performance.now() + 250;
        };
        // 保险：万一等不到松手事件，也别一直吞掉点击
        const fallback = window.setTimeout(done, 2000);
        window.addEventListener('pointerup', done, true);
        window.addEventListener('pointercancel', done, true);
    };

    const hitTest = (x: number, y: number, source: DragSource): DropTarget | null => {
        const list = listRef.current;
        const content = contentRef.current;
        const current = dataRef.current;
        if (!list || !content || !view || !current) return null;
        const listRect = list.getBoundingClientRect();
        const contentTop = content.getBoundingClientRect().top;
        // 指针拖出列表时按最近的边来算，自动滚动时落点跟着变
        const px = Math.min(Math.max(x, listRect.left + 8), listRect.right - 8);
        const py = Math.min(Math.max(y, listRect.top + 2), listRect.bottom - 2);
        // 没有这个 API 的环境（jsdom）当作落在空白处：放到根目录末尾
        const hit = typeof document.elementFromPoint === 'function' ? document.elementFromPoint(px, py) : null;
        const el = hit instanceof Element ? hit.closest<HTMLElement>('[data-dnd-kind]') : null;
        const kind = el?.dataset.dndKind;

        let result: DropTarget | null = null;
        if (source.kind === 'folder') {
            const block = hit instanceof Element ? hit.closest<HTMLElement>('[data-dnd-block]') : null;
            if (block) {
                const r = block.getBoundingClientRect();
                const idx = view.folders.findIndex((f) => f.id === block.dataset.dndBlock);
                const before = py < r.top + r.height / 2;
                result = { kind: 'folder-slot', index: idx + (before ? 0 : 1), y: (before ? r.top : r.bottom) - contentTop };
            } else {
                const blocks = content.querySelectorAll<HTMLElement>('[data-dnd-block]');
                const last = blocks[blocks.length - 1];
                result = { kind: 'folder-slot', index: view.folders.length, y: last ? last.getBoundingClientRect().bottom - contentTop : 0 };
            }
        } else if (el && kind === 'request') {
            const r = el.getBoundingClientRect();
            const folderId = el.dataset.dndFolder || null;
            const known = folderId !== null && view.children.has(folderId) ? folderId : null;
            const list2 = known === null ? view.root : (view.children.get(known) ?? []);
            const idx = list2.findIndex((q) => q.id === el.dataset.dndId);
            const before = py < r.top + r.height / 2;
            result = {
                kind: 'slot',
                slot: { folderId: known, index: idx + (before ? 0 : 1) },
                y: (before ? r.top : r.bottom) - contentTop,
                nested: known !== null,
            };
        } else if (el && (kind === 'folder' || kind === 'folder-empty')) {
            const folderId = kind === 'folder' ? el.dataset.dndId : el.dataset.dndFolder;
            if (folderId) result = { kind: 'into', folderId };
        } else {
            // 列表空白处 / 最底下：放到根目录末尾
            const rows = content.querySelectorAll<HTMLElement>('[data-dnd-kind="request"][data-dnd-folder=""]');
            const last = rows[rows.length - 1];
            const blocks = content.querySelectorAll<HTMLElement>('[data-dnd-block]');
            const lastBlock = blocks[blocks.length - 1];
            const edge = last ?? lastBlock;
            result = {
                kind: 'slot',
                slot: { folderId: null, index: view.root.length },
                y: edge ? edge.getBoundingClientRect().bottom - contentTop : 0,
                nested: false,
            };
        }
        // 放下去和原来一样的位置不画指示线
        return result && applyDrop(current, source, result) !== current ? result : null;
    };

    const placeGhost = (x: number, y: number) => {
        const g = ghostRef.current;
        if (g) g.style.transform = `translate3d(${x + 14}px, ${y + 10}px, 0)`;
    };

    const updateDrag = (d: DragState) => {
        placeGhost(d.x, d.y);
        const next = hitTest(d.x, d.y, d.source);
        if (!sameTarget(next, d.target)) {
            d.target = next;
            setDragView({ source: d.source, target: next });
        }
        // 自动滚动：一帧滚一点，滚的同时落点跟着重算
        const list = listRef.current;
        if (!list) return;
        const rect = list.getBoundingClientRect();
        const speed =
            d.y < rect.top + AUTO_SCROLL_EDGE_PX
                ? -Math.ceil(((rect.top + AUTO_SCROLL_EDGE_PX - d.y) / AUTO_SCROLL_EDGE_PX) * AUTO_SCROLL_MAX_SPEED)
                : d.y > rect.bottom - AUTO_SCROLL_EDGE_PX
                  ? Math.ceil(((d.y - (rect.bottom - AUTO_SCROLL_EDGE_PX)) / AUTO_SCROLL_EDGE_PX) * AUTO_SCROLL_MAX_SPEED)
                  : 0;
        if (speed !== 0 && !d.scrollFrame) {
            d.scrollFrame = requestAnimationFrame(() => {
                d.scrollFrame = 0;
                if (drag.current !== d || !d.started) return;
                const before = list.scrollTop;
                list.scrollTop += Math.max(-AUTO_SCROLL_MAX_SPEED, Math.min(AUTO_SCROLL_MAX_SPEED, speed));
                if (list.scrollTop !== before) updateDrag(d);
            });
        }
    };

    /** released：指针已经松开（pointerup / pointercancel）；Esc 和捕获丢失时手可能还按着 */
    const endDrag = (commitDrop: boolean, released: boolean) => {
        const d = drag.current;
        drag.current = null;
        if (!d) return;
        if (d.scrollFrame) cancelAnimationFrame(d.scrollFrame);
        if (!d.started) return;
        document.body.style.cursor = '';
        document.body.style.userSelect = '';
        const list = listRef.current;
        if (list?.hasPointerCapture?.(d.pointerId)) list.releasePointerCapture(d.pointerId);
        setDragView(null);
        suppressClickAfterRelease(released);
        const t = d.target;
        if (!commitDrop || !t) return;
        if (t.kind === 'into') setFolderOpen(t.folderId, true);
        commit((c) => applyDrop(c, d.source, t));
    };

    // 拖着的时候按 Esc 放弃；组件没了（切面板、切路由）也收拾干净
    useEffect(() => {
        if (!dragView) return;
        const onKey = (e: KeyboardEvent) => {
            if (e.key !== 'Escape') return;
            e.preventDefault();
            e.stopPropagation();
            endDrag(false, false);
        };
        window.addEventListener('keydown', onKey, true);
        return () => window.removeEventListener('keydown', onKey, true);
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [dragView !== null]);
    useEffect(
        () => () => {
            if (drag.current?.started) {
                document.body.style.cursor = '';
                document.body.style.userSelect = '';
            }
            drag.current = null;
        },
        [],
    );

    useLayoutEffect(() => {
        const d = drag.current;
        if (dragView && d) placeGhost(d.x, d.y);
    }, [dragView]);

    const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
        if (e.button !== 0 || editing || !view) return;
        const t = e.target as Element;
        if (t.closest('button, input, a')) return;
        const row = t.closest<HTMLElement>('[data-dnd-kind="request"], [data-dnd-kind="folder"]');
        const id = row?.dataset.dndId;
        if (!row || !id) return;
        const kind = row.dataset.dndKind === 'folder' ? 'folder' : 'request';
        const label =
            kind === 'folder'
                ? (view.folders.find((f) => f.id === id)?.name ?? '')
                : (dataRef.current?.requests.find((r) => r.id === id)?.name ?? '');
        drag.current = {
            pointerId: e.pointerId,
            startX: e.clientX,
            startY: e.clientY,
            x: e.clientX,
            y: e.clientY,
            source: { kind, id, label },
            started: false,
            target: null,
            scrollFrame: 0,
        };
    };

    const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
        const d = drag.current;
        if (!d || e.pointerId !== d.pointerId) return;
        d.x = e.clientX;
        d.y = e.clientY;
        if (!d.started) {
            if (Math.hypot(d.x - d.startX, d.y - d.startY) < DRAG_THRESHOLD_PX) return;
            d.started = true;
            e.currentTarget.setPointerCapture?.(e.pointerId);
            document.body.style.cursor = 'grabbing';
            document.body.style.userSelect = 'none';
            window.getSelection?.()?.removeAllRanges();
            setDragView({ source: d.source, target: null });
        }
        updateDrag(d);
    };

    // ---- 键盘：↑↓ Home End 在行间移动，→ ← 展开收起，回车打开，F2 改名，Delete 删除，Alt+↑↓ 挪位置
    const onListKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
        if (e.nativeEvent.isComposing || editing) return;
        const row = (e.target as Element).closest<HTMLElement>('[data-nav]');
        if (!row || row !== e.target) return;
        const key = row.dataset.nav ?? '';
        const [kind, id] = [key.slice(0, 1), key.slice(2)];
        const rows = navRows();
        const idx = rows.indexOf(row);
        const focusAt = (i: number) => rows[Math.max(0, Math.min(rows.length - 1, i))]?.focus();

        if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
            e.preventDefault();
            commit((c) => nudge(c, kind === 'f' ? 'folder' : 'request', id, e.key === 'ArrowUp' ? -1 : 1));
            return;
        }
        switch (e.key) {
            case 'ArrowDown':
                e.preventDefault();
                focusAt(idx + 1);
                return;
            case 'ArrowUp':
                e.preventDefault();
                focusAt(idx - 1);
                return;
            case 'Home':
                e.preventDefault();
                focusAt(0);
                return;
            case 'End':
                e.preventDefault();
                focusAt(rows.length - 1);
                return;
            case 'ArrowRight':
                if (kind === 'f') {
                    e.preventDefault();
                    if (closed.has(id)) setFolderOpen(id, true);
                    else focusAt(idx + 1);
                }
                return;
            case 'ArrowLeft':
                e.preventDefault();
                if (kind === 'f') setFolderOpen(id, false);
                else {
                    const folderId = row.dataset.dndFolder;
                    if (folderId) listRef.current?.querySelector<HTMLElement>(`[data-nav="f:${folderId}"]`)?.focus();
                }
                return;
            case 'Enter': {
                e.preventDefault();
                if (kind === 'f') toggleFolder(id);
                else {
                    const req = dataRef.current?.requests.find((r) => r.id === id);
                    if (req) openSaved(req, e.ctrlKey || e.metaKey);
                }
                return;
            }
            case 'F2':
                e.preventDefault();
                startRename(kind === 'f' ? 'folder' : 'request', id);
                return;
            case 'Delete': {
                e.preventDefault();
                if (kind === 'f') askDeleteFolder(id, true);
                else {
                    const req = dataRef.current?.requests.find((r) => r.id === id);
                    if (req) askDeleteRequest(req, true);
                }
            }
        }
    };

    // ---- 渲染
    const total = data?.requests.length ?? 0;
    const empty = !!view && view.folders.length === 0 && view.root.length === 0;
    // 焦点落在谁身上谁可 Tab 进来；还没落过时第一行可 Tab
    const firstKey = view ? (view.folders[0] ? `f:${view.folders[0].id}` : view.root[0] ? `r:${view.root[0].id}` : null) : null;
    const focusValid =
        focusKey !== null &&
        !!view &&
        (focusKey.startsWith('f:')
            ? view.folders.some((f) => `f:${f.id}` === focusKey)
            : !!data?.requests.some((r) => `r:${r.id}` === focusKey));
    const tabFor = (key: string) => ((focusValid ? key === focusKey : key === firstKey) ? 0 : -1);

    const draggingKey = dragView ? `${dragView.source.kind === 'folder' ? 'f' : 'r'}:${dragView.source.id}` : null;
    const dropTarget = dragView?.target ?? null;

    const requestRow = (req: DebugSavedRequest, nested: boolean) => (
        <RequestRow
            key={req.id}
            request={req}
            nested={nested}
            safety={lookupSummary(catalog?.actions, req.action).summary?.safety ?? null}
            editing={editing?.kind === 'request' && editing.id === req.id}
            dragging={draggingKey === `r:${req.id}`}
            tabIndex={tabFor(`r:${req.id}`)}
            sendDisabledReason={sendDisabledReason(req)}
            folders={view?.folders ?? []}
            onOpen={openSaved}
            onSend={onSend}
            onStartRename={startRename}
            onCommitRename={commitRename}
            onCancelRename={cancelRename}
            onDelete={askDeleteRequest}
            onMoveTo={moveTo}
            shouldIgnoreClick={shouldIgnoreClick}
        />
    );

    let bodyContent: React.ReactNode;
    if (!data && collectionsQuery.isError) {
        bodyContent = (
            <PanelMessage
                icon={Star}
                tone="danger"
                title="读不到收藏"
                hint={collectionsQuery.error?.message}
                action={
                    <Button size="sm" variant="secondary" onClick={() => void collectionsQuery.refetch()}>
                        <RefreshCw size={13} aria-hidden />
                        重试
                    </Button>
                }
            />
        );
    } else if (!view) {
        bodyContent = <SkeletonRows rows={5} rowHeight={COLLECTION_ROW_HEIGHT} />;
    } else if (empty) {
        bodyContent = (
            <PanelMessage
                icon={Star}
                title="还没有收藏"
                hint="在中栏的请求上点「收藏」，或在历史里收藏一条；也可以导入别人分享的收藏文件。"
                action={
                    <Button size="sm" variant="secondary" disabled={importFile.isPending} onClick={() => importFile.mutate()}>
                        <FileInput size={13} aria-hidden />
                        导入收藏
                    </Button>
                }
            />
        );
    } else {
        bodyContent = (
            <div ref={contentRef} className="relative pb-2">
                {view.folders.map((f) => {
                    const children = view.children.get(f.id) ?? [];
                    const open = !closed.has(f.id);
                    return (
                        <div key={f.id} data-dnd-block={f.id}>
                            <FolderRow
                                folder={f}
                                count={children.length}
                                open={open}
                                editing={editing?.kind === 'folder' && editing.id === f.id}
                                dropInto={dropTarget?.kind === 'into' && dropTarget.folderId === f.id}
                                dragging={draggingKey === `f:${f.id}`}
                                tabIndex={tabFor(`f:${f.id}`)}
                                onToggle={toggleFolder}
                                onStartRename={startRename}
                                onCommitRename={commitRename}
                                onCancelRename={cancelRename}
                                onDelete={askDeleteFolder}
                            />
                            {open && (
                                <div role="group">
                                    {children.length > 0 ? (
                                        children.map((r) => requestRow(r, true))
                                    ) : (
                                        <div
                                            data-dnd-kind="folder-empty"
                                            data-dnd-folder={f.id}
                                            data-flip={`e:${f.id}`}
                                            className={cn(
                                                'flex h-7 items-center rounded-sm pl-7 text-[11px] text-text-tertiary',
                                                dropTarget?.kind === 'into' && dropTarget.folderId === f.id && 'bg-brand-soft/60 text-brand',
                                            )}
                                        >
                                            空文件夹，把请求拖进来
                                        </div>
                                    )}
                                </div>
                            )}
                        </div>
                    );
                })}
                {view.root.length > 0 && view.folders.length > 0 && (
                    <div aria-hidden data-flip="root-divider" className="mx-2 my-1 h-px bg-border-subtle/70" />
                )}
                {view.root.map((r) => requestRow(r, false))}
                {/* 拖动时底下留一块放到根目录末尾的地方 */}
                <div
                    data-dnd-kind="root-end"
                    className={cn('rounded-sm transition-colors', dragView ? 'h-10' : 'h-2')}
                    aria-hidden
                />
                {dropTarget && dropTarget.kind !== 'into' && (
                    <div
                        aria-hidden
                        className="pointer-events-none absolute right-1 top-0 h-0.5 rounded-pill bg-brand"
                        style={{
                            left: dropTarget.kind === 'slot' && dropTarget.nested ? 24 : 4,
                            transform: `translateY(${dropTarget.y - 1}px)`,
                        }}
                    />
                )}
            </div>
        );
    }

    return (
        <div className="flex min-h-0 flex-1 flex-col">
            <div className={cn(COLUMN_HEADER_CLASS, 'gap-1 pl-3')}>
                <span className="min-w-0 flex-1 truncate text-[11px] text-text-tertiary">
                    {view ? (total > 0 ? `${total} 个请求` : '收藏') : '收藏'}
                    {view && view.folders.length > 0 ? ` · ${view.folders.length} 个文件夹` : ''}
                </span>
                <IconAction icon={FolderPlus} label="新建文件夹" size="md" tooltipSide="bottom" disabledReason={view ? null : '收藏还没读出来'} onClick={newFolder} />
                <IconAction icon={FileInput} label="导入收藏" size="md" tooltipSide="bottom" busy={importFile.isPending} onClick={() => importFile.mutate()} />
                <IconAction
                    icon={FileOutput}
                    label="导出收藏"
                    size="md"
                    tooltipSide="bottom"
                    busy={exportFile.isPending}
                    disabledReason={total === 0 && (view?.folders.length ?? 0) === 0 ? '还没有收藏' : null}
                    onClick={() => exportFile.mutate()}
                />
            </div>
            {/* 滚动容器一直挂着：滚动记忆认的是同一个元素 */}
            <div
                ref={listRef}
                role={view && !empty ? 'tree' : undefined}
                aria-label={view && !empty ? '收藏' : undefined}
                onPointerDown={onPointerDown}
                onPointerMove={onPointerMove}
                onPointerUp={() => endDrag(true, true)}
                onPointerCancel={() => endDrag(false, true)}
                // 捕获被别处抢走（不是正常松手）：当作放弃，不落到半路的位置上
                onLostPointerCapture={() => drag.current?.started && endDrag(false, false)}
                onKeyDown={onListKeyDown}
                onFocus={(e) => {
                    const key = (e.target as HTMLElement).dataset?.nav;
                    if (key) setFocusKey(key);
                }}
                className="flex min-h-0 flex-1 flex-col overflow-y-auto px-1.5 pt-1"
            >
                {bodyContent}
            </div>
            {dragView && (
                <BodyPortal>
                    <div
                        ref={ghostRef}
                        aria-hidden
                        className="pointer-events-none fixed left-0 top-0 z-[60] flex max-w-[240px] items-center gap-1.5 rounded-sm border border-border-subtle bg-elevated px-2 py-1 text-[12px] text-text shadow-popover"
                    >
                        <GripVertical size={12} aria-hidden className="shrink-0 text-text-tertiary" />
                        <span className="truncate">{dragView.source.label}</span>
                    </div>
                </BodyPortal>
            )}
            <ConfirmDialog request={confirm} onClose={() => setConfirm(null)} />
            {target && sendConfirm && (
                <DangerConfirmDialog
                    open={sendConfirmOpen}
                    onOpenChange={setSendConfirmOpen}
                    botId={target.bot_id}
                    botName={targetDisplayName(target)}
                    action={sendConfirm.req.action}
                    consequenceAction={sendConfirm.summaryFrom ?? sendConfirm.req.action}
                    reason={sendConfirm.reason}
                    note={`这是收藏「${sendConfirm.req.name}」，会发给 ${targetDisplayName(target)}。`}
                    params={plainParams(sendConfirm.req.params)}
                    onConfirm={() => {
                        const req = sendConfirm.req;
                        // 确认框开着的这段时间里 Bot 可能停了：真发之前再看一眼
                        if (!sendDisabledReason(req)) doSend(req);
                    }}
                />
            )}
        </div>
    );
});

function plainParams(params: unknown): Record<string, unknown> {
    return params && typeof params === 'object' && !Array.isArray(params) ? (params as Record<string, unknown>) : {};
}
