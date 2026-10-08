// 收藏列表的指针拖拽状态机与键盘导航，都要直接摸 DOM，进不了 core/domain。
//
// 纯判定（落点比较、同位不动、拖拽阈值常量）在 core/domain/debug/collectionsDrag.ts；
// 这里只剩命中测试（elementFromPoint）、边缘自动滚动、幽灵跟手（直接改 transform）、
// 松手后吞掉浏览器补发的 click，以及键盘的行间移动。处理函数每次渲染重建、闭包捕获
// 当次渲染的 view / editing，和原先写在组件体内时一致。

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import type {
    KeyboardEvent as ReactKeyboardEvent,
    MutableRefObject,
    PointerEvent as ReactPointerEvent,
    RefObject,
} from 'react';
import type { DebugCollections } from '../../../core/ipc/generated/debug/DebugCollections';
import type { DebugSavedRequest } from '../../../core/ipc/generated/debug/DebugSavedRequest';
import { nudge, type CollectionsView } from '../../../core/domain/debug/collectionsOps';
import {
    applyDrop,
    AUTO_SCROLL_EDGE_PX,
    AUTO_SCROLL_MAX_SPEED,
    DRAG_THRESHOLD_PX,
    sameTarget,
    type DragSource,
    type DragState,
    type DropTarget,
} from '../../../core/domain/debug/collectionsDrag';

export interface CollectionsDragParams {
    listRef: RefObject<HTMLElement | null>;
    contentRef: RefObject<HTMLElement | null>;
    ghostRef: RefObject<HTMLElement | null>;
    dataRef: MutableRefObject<DebugCollections | undefined>;
    view: CollectionsView | null;
    /** 改名输入框开着时不起拖 */
    editing: boolean;
    commit: (fn: (c: DebugCollections) => DebugCollections) => void;
    setFolderOpen: (id: string, open: boolean) => void;
}

export function useCollectionsDrag({
    listRef,
    contentRef,
    ghostRef,
    dataRef,
    view,
    editing,
    commit,
    setFolderOpen,
}: CollectionsDragParams) {
    const drag = useRef<DragState | null>(null);
    const [dragView, setDragView] = useState<{
        source: DragSource;
        target: DropTarget | null;
    } | null>(null);
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
        const hit =
            typeof document.elementFromPoint === 'function'
                ? document.elementFromPoint(px, py)
                : null;
        const el = hit instanceof Element ? hit.closest<HTMLElement>('[data-dnd-kind]') : null;
        const kind = el?.dataset.dndKind;

        let result: DropTarget | null = null;
        if (source.kind === 'folder') {
            const block =
                hit instanceof Element ? hit.closest<HTMLElement>('[data-dnd-block]') : null;
            if (block) {
                const r = block.getBoundingClientRect();
                const idx = view.folders.findIndex((f) => f.id === block.dataset.dndBlock);
                const before = py < r.top + r.height / 2;
                result = {
                    kind: 'folder-slot',
                    index: idx + (before ? 0 : 1),
                    y: (before ? r.top : r.bottom) - contentTop,
                };
            } else {
                const blocks = content.querySelectorAll<HTMLElement>('[data-dnd-block]');
                const last = blocks[blocks.length - 1];
                result = {
                    kind: 'folder-slot',
                    index: view.folders.length,
                    y: last ? last.getBoundingClientRect().bottom - contentTop : 0,
                };
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
            const rows = content.querySelectorAll<HTMLElement>(
                '[data-dnd-kind="request"][data-dnd-folder=""]',
            );
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
                ? -Math.ceil(
                      ((rect.top + AUTO_SCROLL_EDGE_PX - d.y) / AUTO_SCROLL_EDGE_PX) *
                          AUTO_SCROLL_MAX_SPEED,
                  )
                : d.y > rect.bottom - AUTO_SCROLL_EDGE_PX
                  ? Math.ceil(
                        ((d.y - (rect.bottom - AUTO_SCROLL_EDGE_PX)) / AUTO_SCROLL_EDGE_PX) *
                            AUTO_SCROLL_MAX_SPEED,
                    )
                  : 0;
        if (speed !== 0 && !d.scrollFrame) {
            d.scrollFrame = requestAnimationFrame(() => {
                d.scrollFrame = 0;
                if (drag.current !== d || !d.started) return;
                const before = list.scrollTop;
                list.scrollTop += Math.max(
                    -AUTO_SCROLL_MAX_SPEED,
                    Math.min(AUTO_SCROLL_MAX_SPEED, speed),
                );
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
        const onKey = (e: globalThis.KeyboardEvent) => {
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

    const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
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

    const onPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
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

    return {
        drag,
        dragView,
        shouldIgnoreClick,
        onPointerDown,
        onPointerMove,
        endDrag,
    };
}

export interface CollectionsKeyNavParams {
    listRef: RefObject<HTMLElement | null>;
    dataRef: MutableRefObject<DebugCollections | undefined>;
    /** 改名输入框开着时列表快捷键全让位 */
    editing: boolean;
    closed: ReadonlySet<string>;
    setFolderOpen: (id: string, open: boolean) => void;
    toggleFolder: (id: string) => void;
    commit: (fn: (c: DebugCollections) => DebugCollections) => void;
    startRename: (kind: 'request' | 'folder', id: string) => void;
    askDeleteRequest: (req: DebugSavedRequest, fromKeyboard?: boolean) => void;
    askDeleteFolder: (id: string, fromKeyboard?: boolean) => void;
    openSaved: (req: DebugSavedRequest, newTab: boolean) => string;
}

function navRows(listRef: RefObject<HTMLElement | null>) {
    return Array.from(listRef.current?.querySelectorAll<HTMLElement>('[data-nav]') ?? []);
}

/**
 * 从键盘删一行后焦点去哪：没删（取消了）回到这一行；删了就落到下一行（没有就上一行），
 * 删文件夹时跳过它里面的请求。返回的取值函数等对话框关掉再调：
 * 那时列表已按删除结果画好（没删就还回到原行）。
 */
export function collectionsFocusAfterDelete(
    listRef: RefObject<HTMLElement | null>,
    key: string,
): () => HTMLElement | null {
    const navRow = (rowKey: string) =>
        listRef.current?.querySelector<HTMLElement>(`[data-nav="${rowKey}"]`) ?? null;
    const rows = navRows(listRef);
    const idx = rows.findIndex((r) => r.dataset.nav === key);
    const block = key.startsWith('f:')
        ? listRef.current?.querySelector(`[data-dnd-block="${key.slice(2)}"]`)
        : null;
    const outside = (r: HTMLElement) => r.dataset.nav !== key && !(block && block.contains(r));
    const neighbor = (
        rows.slice(idx + 1).find(outside) ?? rows.slice(0, Math.max(0, idx)).reverse().find(outside)
    )?.dataset.nav;
    return () => navRow(key) ?? (neighbor ? navRow(neighbor) : null) ?? navRows(listRef)[0] ?? null;
}

/** ↑↓ Home End 行间移动，→ ← 展开收起，回车打开，F2 改名，Delete 删除，Alt+↑↓ 挪位置 */
export function useCollectionsKeyNav({
    listRef,
    dataRef,
    editing,
    closed,
    setFolderOpen,
    toggleFolder,
    commit,
    startRename,
    askDeleteRequest,
    askDeleteFolder,
    openSaved,
}: CollectionsKeyNavParams) {
    const onListKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
        if (e.nativeEvent.isComposing || editing) return;
        const row = (e.target as Element).closest<HTMLElement>('[data-nav]');
        if (!row || row !== e.target) return;
        const key = row.dataset.nav ?? '';
        const [kind, id] = [key.slice(0, 1), key.slice(2)];
        const rows = navRows(listRef);
        const idx = rows.indexOf(row);
        const focusAt = (i: number) => rows[Math.max(0, Math.min(rows.length - 1, i))]?.focus();

        if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
            e.preventDefault();
            commit((c) =>
                nudge(c, kind === 'f' ? 'folder' : 'request', id, e.key === 'ArrowUp' ? -1 : 1),
            );
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
                    if (folderId)
                        listRef.current
                            ?.querySelector<HTMLElement>(`[data-nav="f:${folderId}"]`)
                            ?.focus();
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

    return { onListKeyDown };
}
