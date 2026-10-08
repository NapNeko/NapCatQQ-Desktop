// 左栏「收藏」：存下来的请求，文件夹即集合。
//
// 单击打开（没动过的当前标签直接顶替，Ctrl / ⌘ 或中键另开）；▶ 在当前 Bot 上发（先打开成标签再发，
// 响应照常显示在中栏，危险接口先确认）。拖动排序、拖进 / 拖出文件夹，全用指针事件自己做：
// 按下移动超过几像素才算拖，松手前按 Esc 放弃；拖到列表上下边缘自动滚。键盘上 Alt+↑↓ 同样能挪位置。
// 每个改动都是整份替换保存（乐观更新，界面立刻跟手，失败会退回并弹条）。
//
// 这里只留编排：拖拽 / 键盘导航状态机在 collectionsDragParts，纯判定在
// core/domain/debug/collectionsDrag，列表行在 CollectionRows，列表区与顶部工具条各成一个子件。

import { memo, useCallback, useMemo, useRef, useState } from 'react';
import { GripVertical } from 'lucide-react';
import { BodyPortal } from '../../../shared/ui/BodyPortal';
import {
    useDebugCollections,
    useExportCollections,
    useImportCollections,
    useSaveCollections,
} from '../../../hooks/debug/useDebugCollections';
import { useDebugCatalog } from '../../../hooks/debug/useDebugCatalog';
import { useDebugCall } from '../../../hooks/debug/useDebugCall';
import {
    debugWorkspaceStore,
    useDebugChannelChoice,
} from '../../../hooks/debug/debugWorkspaceStore';
import { useScrollMemory } from '../../../hooks/debug/debugScrollMemory';
import { AUTO_CHANNEL } from '../../../core/domain/debug/channelPick';
import { lookupSummary } from '../../../core/domain/debug/catalogView';
import { countOmittedInValue } from '../../../core/domain/debug/omittedParams';
import { targetDisplayName } from '../../../core/domain/debug/targetGroups';
import type { DebugCollections } from '../../../core/ipc/generated/debug/DebugCollections';
import type { DebugSavedRequest } from '../../../core/ipc/generated/debug/DebugSavedRequest';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import { DangerConfirmDialog, dangerConfirmSkipped } from '../DangerConfirmDialog';
import {
    addFolder,
    collectionsView,
    deleteFolder,
    deleteRequest,
    moveRequest,
    renameFolder,
    renameRequest,
    type CollectionsView,
} from '../../../core/domain/debug/collectionsOps';
import { plainParams } from '../../../core/domain/debug/collectionsDrag';
import { paramsTextOf } from '../../../core/domain/debug/historyReplay';
import { ConfirmDialog, type ConfirmRequest } from './panelParts';
import { useFlip } from './useFlip';
import {
    collectionsFocusAfterDelete,
    useCollectionsDrag,
    useCollectionsKeyNav,
} from './collectionsDragParts';
import { CollectionsBody } from './CollectionsBody';
import { CollectionsHeader } from './CollectionsHeader';

// 收起了哪些文件夹：界面状态，不落盘
let savedClosedFolders: ReadonlySet<string> = new Set();

export const CollectionsPanel = memo(function CollectionsPanel({
    target,
}: {
    target: DebugTarget | null;
}) {
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

    const view: CollectionsView | null = useMemo(
        () => (data ? collectionsView(data) : null),
        [data],
    );

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
    const toggleFolder = useCallback(
        (id: string) => setFolderOpen(id, closed.has(id)),
        [closed, setFolderOpen],
    );

    // ---- 打开 / 发送
    const openSaved = useCallback((req: DebugSavedRequest, newTab: boolean): string => {
        const tabId = debugWorkspaceStore.openAction(req.action, {
            newTab,
            paramsText: paramsTextOf(req.params),
        });
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
    const [sendConfirm, setSendConfirm] = useState<{
        req: DebugSavedRequest;
        reason?: string;
        summaryFrom?: string | null;
    } | null>(null);
    const [sendConfirmOpen, setSendConfirmOpen] = useState(false);

    const onSend = useCallback(
        (req: DebugSavedRequest) => {
            if (sendDisabledReason(req) || !target) return;
            // 分级查法和中栏同一套：名字、别名、还有 NapCat 的 _async / _rate_limited 变体，都按原接口认
            const { summary, summaryFrom } = lookupSummary(catalog?.actions, req.action);
            const safety = summary?.safety ?? null;
            if (
                safety === 'read_only' ||
                safety === 'side_effect' ||
                dangerConfirmSkipped(target.bot_id, req.action)
            ) {
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
    const focusRowSoon = useCallback((key: string) => {
        requestAnimationFrame(() =>
            listRef.current?.querySelector<HTMLElement>(`[data-nav="${key}"]`)?.focus(),
        );
    }, []);

    const startRename = useCallback(
        (kind: 'request' | 'folder', id: string) => setEditing({ kind, id }),
        [],
    );
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
            commit((c) =>
                kind === 'folder'
                    ? renameFolder(c, id, name)
                    : renameRequest(c, id, name, Date.now()),
            );
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
                restoreFocus: fromKeyboard
                    ? collectionsFocusAfterDelete(listRef, `r:${req.id}`)
                    : undefined,
            }),
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
                restoreFocus: fromKeyboard
                    ? collectionsFocusAfterDelete(listRef, `f:${id}`)
                    : undefined,
            });
        },
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

    // ---- 拖动 / 键盘导航（状态机在 collectionsDragParts）
    const { drag, dragView, shouldIgnoreClick, onPointerDown, onPointerMove, endDrag } =
        useCollectionsDrag({
            listRef,
            contentRef,
            ghostRef,
            dataRef,
            view,
            editing: editing !== null,
            commit,
            setFolderOpen,
        });

    const { onListKeyDown } = useCollectionsKeyNav({
        listRef,
        dataRef,
        editing: editing !== null,
        closed,
        setFolderOpen,
        toggleFolder,
        commit,
        startRename,
        askDeleteRequest,
        askDeleteFolder,
        openSaved,
    });

    // ---- 渲染
    const total = data?.requests.length ?? 0;
    const empty = !!view && view.folders.length === 0 && view.root.length === 0;
    // 焦点落在谁身上谁可 Tab 进来；还没落过时第一行可 Tab
    const firstKey = view
        ? view.folders[0]
            ? `f:${view.folders[0].id}`
            : view.root[0]
              ? `r:${view.root[0].id}`
              : null
        : null;
    const focusValid =
        focusKey !== null &&
        !!view &&
        (focusKey.startsWith('f:')
            ? view.folders.some((f) => `f:${f.id}` === focusKey)
            : !!data?.requests.some((r) => `r:${r.id}` === focusKey));
    const tabFor = (key: string) => ((focusValid ? key === focusKey : key === firstKey) ? 0 : -1);

    const draggingKey = dragView
        ? `${dragView.source.kind === 'folder' ? 'f' : 'r'}:${dragView.source.id}`
        : null;
    const dropTarget = dragView?.target ?? null;

    return (
        <div className="flex min-h-0 flex-1 flex-col">
            <CollectionsHeader
                view={view}
                total={total}
                onNewFolder={newFolder}
                importPending={importFile.isPending}
                onImport={() => importFile.mutate()}
                exportPending={exportFile.isPending}
                onExport={() => exportFile.mutate()}
            />
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
                <CollectionsBody
                    contentRef={contentRef}
                    data={data}
                    isError={collectionsQuery.isError}
                    errorMessage={collectionsQuery.error?.message}
                    onRetry={() => void collectionsQuery.refetch()}
                    importPending={importFile.isPending}
                    onImport={() => importFile.mutate()}
                    view={view}
                    closed={closed}
                    editing={editing}
                    catalogActions={catalog?.actions}
                    dragging={dragView !== null}
                    draggingKey={draggingKey}
                    dropTarget={dropTarget}
                    tabFor={tabFor}
                    sendDisabledReason={sendDisabledReason}
                    onOpen={openSaved}
                    onSend={onSend}
                    onStartRename={startRename}
                    onCommitRename={commitRename}
                    onCancelRename={cancelRename}
                    onDeleteRequest={askDeleteRequest}
                    onDeleteFolder={askDeleteFolder}
                    onToggleFolder={toggleFolder}
                    onMoveTo={moveTo}
                    shouldIgnoreClick={shouldIgnoreClick}
                />
            </div>
            {dragView && (
                <BodyPortal>
                    <div
                        ref={ghostRef}
                        aria-hidden
                        className="pointer-events-none fixed left-0 top-0 z-[60] flex max-w-[240px] items-center gap-1.5 rounded-sm border border-border-subtle bg-elevated px-2 py-1 text-[12px] text-text shadow-popover"
                    >
                        <GripVertical
                            size={12}
                            aria-hidden
                            className="shrink-0 text-text-tertiary"
                        />
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
