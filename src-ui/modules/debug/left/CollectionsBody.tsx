// 收藏列表主体：读取失败 / 骨架 / 空态三种提示，以及文件夹树本体
// （文件夹块、空文件夹占位、根区分隔线、拖放指示线）。
// 列表行本身的绘制在 CollectionRows，这里只管组织和拖放反馈。

import type { Ref } from 'react';
import { FileInput, RefreshCw, Star } from 'lucide-react';
import { cn } from '../../../shared/utils/cn';
import { Button } from '../../../shared/ui';
import type { DebugCollections } from '../../../core/ipc/generated/debug/DebugCollections';
import type { DebugSavedRequest } from '../../../core/ipc/generated/debug/DebugSavedRequest';
import type { DebugSavedFolder } from '../../../core/ipc/generated/debug/DebugSavedFolder';
import type { DebugActionSummary } from '../../../core/ipc/generated/debug/DebugActionSummary';
import { lookupSummary } from '../../../core/domain/debug/catalogView';
import type { CollectionsView } from '../../../core/domain/debug/collectionsOps';
import type { DropTarget } from '../../../core/domain/debug/collectionsDrag';
import { COLLECTION_ROW_HEIGHT, FolderRow, RequestRow } from './CollectionRows';
import { PanelMessage, SkeletonRows } from './panelParts';

export interface CollectionsBodyProps {
    contentRef: Ref<HTMLDivElement>;
    data: DebugCollections | undefined;
    isError: boolean;
    errorMessage?: string;
    onRetry: () => void;
    importPending: boolean;
    onImport: () => void;
    view: CollectionsView | null;
    closed: ReadonlySet<string>;
    editing: { kind: 'request' | 'folder'; id: string } | null;
    catalogActions: readonly DebugActionSummary[] | undefined;
    /** 拖着的时候根区末尾垫一块落点区 */
    dragging: boolean;
    /** kind + id：正在被拖的那一行 */
    draggingKey: string | null;
    dropTarget: DropTarget | null;
    /** 焦点落在谁身上谁可 Tab 进来 */
    tabFor: (key: string) => number;
    sendDisabledReason: (req: DebugSavedRequest) => string | null;
    onOpen: (request: DebugSavedRequest, newTab: boolean) => void;
    onSend: (request: DebugSavedRequest) => void;
    onStartRename: (kind: 'request' | 'folder', id: string) => void;
    onCommitRename: (kind: 'request' | 'folder', id: string, name: string, byKey: boolean) => void;
    onCancelRename: (kind: 'request' | 'folder', id: string) => void;
    onDeleteRequest: (request: DebugSavedRequest) => void;
    onDeleteFolder: (id: string) => void;
    onToggleFolder: (id: string) => void;
    onMoveTo: (id: string, folderId: string | null) => void;
    shouldIgnoreClick: () => boolean;
}

export function CollectionsBody({
    contentRef,
    data,
    isError,
    errorMessage,
    onRetry,
    importPending,
    onImport,
    view,
    closed,
    editing,
    catalogActions,
    dragging,
    draggingKey,
    dropTarget,
    tabFor,
    sendDisabledReason,
    onOpen,
    onSend,
    onStartRename,
    onCommitRename,
    onCancelRename,
    onDeleteRequest,
    onDeleteFolder,
    onToggleFolder,
    onMoveTo,
    shouldIgnoreClick,
}: CollectionsBodyProps) {
    if (!data && isError) {
        return (
            <PanelMessage
                icon={Star}
                tone="danger"
                title="读不到收藏"
                hint={errorMessage}
                action={
                    <Button size="sm" variant="secondary" onClick={onRetry}>
                        <RefreshCw size={13} aria-hidden />
                        重试
                    </Button>
                }
            />
        );
    }
    if (!view) {
        return <SkeletonRows rows={5} rowHeight={COLLECTION_ROW_HEIGHT} />;
    }
    if (view.folders.length === 0 && view.root.length === 0) {
        return (
            <PanelMessage
                icon={Star}
                title="还没有收藏"
                hint="在中栏的请求上点「收藏」，或在历史里收藏一条；也可以导入别人分享的收藏文件。"
                action={
                    <Button
                        size="sm"
                        variant="secondary"
                        disabled={importPending}
                        onClick={onImport}
                    >
                        <FileInput size={13} aria-hidden />
                        导入收藏
                    </Button>
                }
            />
        );
    }

    const requestRow = (req: DebugSavedRequest, nested: boolean) => (
        <RequestRow
            key={req.id}
            request={req}
            nested={nested}
            safety={lookupSummary(catalogActions, req.action).summary?.safety ?? null}
            editing={editing?.kind === 'request' && editing.id === req.id}
            dragging={draggingKey === `r:${req.id}`}
            tabIndex={tabFor(`r:${req.id}`)}
            sendDisabledReason={sendDisabledReason(req)}
            folders={view.folders}
            onOpen={onOpen}
            onSend={onSend}
            onStartRename={onStartRename}
            onCommitRename={onCommitRename}
            onCancelRename={onCancelRename}
            onDelete={onDeleteRequest}
            onMoveTo={onMoveTo}
            shouldIgnoreClick={shouldIgnoreClick}
        />
    );

    return (
        <div ref={contentRef} className="relative pb-2">
            {view.folders.map((f: DebugSavedFolder) => {
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
                            onToggle={onToggleFolder}
                            onStartRename={onStartRename}
                            onCommitRename={onCommitRename}
                            onCancelRename={onCancelRename}
                            onDelete={onDeleteFolder}
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
                                            dropTarget?.kind === 'into' &&
                                                dropTarget.folderId === f.id &&
                                                'bg-brand-soft/60 text-brand',
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
                <div
                    aria-hidden
                    data-flip="root-divider"
                    className="mx-2 my-1 h-px bg-border-subtle/70"
                />
            )}
            {view.root.map((r) => requestRow(r, false))}
            {/* 拖动时底下留一块放到根目录末尾的地方 */}
            <div
                data-dnd-kind="root-end"
                className={cn('rounded-sm transition-colors', dragging ? 'h-10' : 'h-2')}
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
