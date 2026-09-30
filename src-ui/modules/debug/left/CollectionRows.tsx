// 收藏列表里的文件夹行和请求行。
//
// 行上带 data-dnd-* 标记给拖动命中用、data-flip 给换位动画用、data-nav 给键盘上下移动用；
// 拖动、键盘的逻辑都在 CollectionsPanel 里统一处理，行只管画和把点击交出去。

import { memo, useEffect, useRef } from 'react';
import {
    CornerDownRight,
    ExternalLink,
    Folder,
    FolderInput,
    FolderOpen,
    Pencil,
    Play,
    SquarePlus,
    Trash2,
} from 'lucide-react';
import { cn } from '../../../shared/utils/cn';
import {
    ContextMenu,
    ContextMenuContent,
    ContextMenuItem,
    ContextMenuLabel,
    ContextMenuSeparator,
    ContextMenuSub,
    ContextMenuSubContent,
    ContextMenuSubTrigger,
    ContextMenuTrigger,
} from '../../../shared/ui';
import { ExpandChevron } from '../../../shared/ui/motion';
import { channelShortLabel } from '../../../core/domain/debug/channelCopy';
import type { DebugActionSafety } from '../../../core/ipc/generated/debug/DebugActionSafety';
import type { DebugSavedFolder } from '../../../core/ipc/generated/debug/DebugSavedFolder';
import type { DebugSavedRequest } from '../../../core/ipc/generated/debug/DebugSavedRequest';
import { MAX_NAME_LENGTH } from '../../../core/domain/debug/collectionsOps';
import { IconAction } from './panelParts';
import { SAFETY_DOT_CLASS, SAFETY_LABEL } from '../../../core/domain/debug/safety';

export const COLLECTION_ROW_HEIGHT = 30;

/**
 * 从右键菜单点「重命名」：菜单关上时 Radix 会把焦点还给行，刚挂上的改名框一失焦就当成提交了，
 * 改名框一闪就没。选了重命名时就不还焦点（其它菜单项照常还给行）
 */
function useMenuRename(onRename: () => void) {
    const skipRestore = useRef(false);
    return {
        pick: () => {
            skipRestore.current = true;
            onRename();
        },
        onCloseAutoFocus: (e: Event) => {
            if (!skipRestore.current) return;
            skipRestore.current = false;
            e.preventDefault();
        },
    };
}

const ROW_BASE =
    'group relative flex select-none items-center gap-1.5 rounded-sm pr-1 outline-none transition-colors ' +
    'focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-brand/60';

/** 行内改名：回车 / 失焦保存，Esc 放弃 */
function RenameInput({
    value,
    label,
    onCommit,
    onCancel,
}: {
    value: string;
    label: string;
    /** byKey：回车提交（焦点该回到行上）；失焦提交是用户点了别处，不抢焦点 */
    onCommit: (v: string, byKey: boolean) => void;
    onCancel: () => void;
}) {
    const ref = useRef<HTMLInputElement>(null);
    const done = useRef(false);
    useEffect(() => {
        const el = ref.current;
        if (!el) return;
        el.focus();
        el.select();
    }, []);
    const finish = (commit: boolean, byKey: boolean) => {
        if (done.current) return;
        done.current = true;
        if (commit) onCommit(ref.current?.value ?? value, byKey);
        else onCancel();
    };
    return (
        <input
            ref={ref}
            defaultValue={value}
            aria-label={label}
            maxLength={MAX_NAME_LENGTH}
            spellCheck={false}
            onClick={(e) => e.stopPropagation()}
            onPointerDown={(e) => e.stopPropagation()}
            onKeyDown={(e) => {
                e.stopPropagation();
                if (e.nativeEvent.isComposing) return;
                if (e.key === 'Enter') {
                    e.preventDefault();
                    finish(true, true);
                } else if (e.key === 'Escape') {
                    e.preventDefault();
                    finish(false, true);
                }
            }}
            onBlur={() => finish(true, false)}
            className="h-6 min-w-0 flex-1 rounded-xs border border-brand bg-field px-1.5 text-[12px] text-text outline-none ring-2 ring-inset ring-brand/40"
        />
    );
}

// ---------------------------------------------------------------------------
// 文件夹
// ---------------------------------------------------------------------------

export interface FolderRowProps {
    folder: DebugSavedFolder;
    count: number;
    open: boolean;
    editing: boolean;
    /** 拖着请求悬在这个文件夹上：松手就放进来 */
    dropInto: boolean;
    /** 这个文件夹正被拖着 */
    dragging: boolean;
    tabIndex: number;
    onToggle: (id: string) => void;
    onStartRename: (kind: 'folder', id: string) => void;
    onCommitRename: (kind: 'folder', id: string, name: string, byKey: boolean) => void;
    onCancelRename: (kind: 'folder', id: string) => void;
    onDelete: (id: string) => void;
}

export const FolderRow = memo(function FolderRow({
    folder,
    count,
    open,
    editing,
    dropInto,
    dragging,
    tabIndex,
    onToggle,
    onStartRename,
    onCommitRename,
    onCancelRename,
    onDelete,
}: FolderRowProps) {
    const Icon = open ? FolderOpen : Folder;
    const menuRename = useMenuRename(() => onStartRename('folder', folder.id));
    return (
        <ContextMenu>
            <ContextMenuTrigger asChild disabled={editing}>
                <div
                    role="treeitem"
                    aria-level={1}
                    aria-expanded={open}
                    aria-label={`文件夹 ${folder.name}，${count} 个请求`}
                    tabIndex={tabIndex}
                    data-nav={`f:${folder.id}`}
                    data-flip={`f:${folder.id}`}
                    data-dnd-kind="folder"
                    data-dnd-id={folder.id}
                    onClick={() => !editing && onToggle(folder.id)}
                    style={{ height: COLLECTION_ROW_HEIGHT }}
                    className={cn(
                        ROW_BASE,
                        'cursor-pointer pl-1',
                        dropInto ? 'bg-brand-soft ring-1 ring-inset ring-brand' : 'hover:bg-elevated/35',
                        dragging && 'opacity-40',
                    )}
                >
                    <ExpandChevron open={open} size={13} />
                    <Icon size={14} strokeWidth={2} aria-hidden className={cn('shrink-0', dropInto ? 'text-brand' : 'text-text-tertiary')} />
                    {editing ? (
                        <RenameInput
                            value={folder.name}
                            label="文件夹名"
                            onCommit={(name, byKey) => onCommitRename('folder', folder.id, name, byKey)}
                            onCancel={() => onCancelRename('folder', folder.id)}
                        />
                    ) : (
                        <>
                            <span className="min-w-0 flex-1 truncate text-[12px] font-medium text-text-secondary">{folder.name}</span>
                            <span className="shrink-0 pr-1 text-[10px] tabular-nums text-text-tertiary transition-opacity group-focus-within:opacity-0 group-hover:opacity-0">
                                {count}
                            </span>
                            <span className="pointer-events-none absolute right-1 flex items-center gap-0.5 rounded-sm bg-surface/90 opacity-0 transition-opacity group-focus-within:pointer-events-auto group-focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100">
                                <IconAction icon={Pencil} label="重命名" focusable={false} onClick={() => onStartRename('folder', folder.id)} />
                                <IconAction icon={Trash2} label="删除文件夹" tone="danger" focusable={false} onClick={() => onDelete(folder.id)} />
                            </span>
                        </>
                    )}
                </div>
            </ContextMenuTrigger>
            <ContextMenuContent className="w-44" onCloseAutoFocus={menuRename.onCloseAutoFocus}>
                <ContextMenuLabel className="truncate text-2xs">{folder.name}</ContextMenuLabel>
                <ContextMenuSeparator />
                <ContextMenuItem onClick={menuRename.pick}>
                    <Pencil size={13} />
                    <span>重命名</span>
                </ContextMenuItem>
                <ContextMenuItem tone="danger" onClick={() => onDelete(folder.id)}>
                    <Trash2 size={13} className="text-danger" />
                    <span>删除文件夹</span>
                </ContextMenuItem>
            </ContextMenuContent>
        </ContextMenu>
    );
});

// ---------------------------------------------------------------------------
// 请求
// ---------------------------------------------------------------------------

export interface RequestRowProps {
    request: DebugSavedRequest;
    nested: boolean;
    /** 目录里查到的分级；目录里没有这个接口时为 null（按「有副作用」处理，点是灰的） */
    safety: DebugActionSafety | null;
    editing: boolean;
    dragging: boolean;
    tabIndex: number;
    /** 不能发时的原因（没选 Bot、Bot 没在跑、流式接口……） */
    sendDisabledReason: string | null;
    folders: DebugSavedFolder[];
    onOpen: (request: DebugSavedRequest, newTab: boolean) => void;
    onSend: (request: DebugSavedRequest) => void;
    onStartRename: (kind: 'request', id: string) => void;
    onCommitRename: (kind: 'request', id: string, name: string, byKey: boolean) => void;
    onCancelRename: (kind: 'request', id: string) => void;
    onDelete: (request: DebugSavedRequest) => void;
    onMoveTo: (id: string, folderId: string | null) => void;
    /** 刚拖完松手时的那次 click 不算打开 */
    shouldIgnoreClick: () => boolean;
}

export const RequestRow = memo(function RequestRow({
    request,
    nested,
    safety,
    editing,
    dragging,
    tabIndex,
    sendDisabledReason,
    folders,
    onOpen,
    onSend,
    onStartRename,
    onCommitRename,
    onCancelRename,
    onDelete,
    onMoveTo,
    shouldIgnoreClick,
}: RequestRowProps) {
    const title = [
        request.action,
        safety ? SAFETY_LABEL[safety] : '目录里没有这个接口',
        request.channel ? `固定走 ${channelShortLabel(request.channel)}` : null,
        request.note,
        '单击打开，Ctrl+单击另开标签，拖动或 Alt+↑↓ 调整位置',
    ]
        .filter(Boolean)
        .join(' · ');
    const menuRename = useMenuRename(() => onStartRename('request', request.id));

    return (
        <ContextMenu>
            <ContextMenuTrigger asChild disabled={editing}>
                <div
                    role="treeitem"
                    aria-level={nested ? 2 : 1}
                    aria-label={`${request.name}（${request.action}）`}
                    tabIndex={tabIndex}
                    title={editing ? undefined : title}
                    data-nav={`r:${request.id}`}
                    data-flip={`r:${request.id}`}
                    data-dnd-kind="request"
                    data-dnd-id={request.id}
                    data-dnd-folder={request.folder_id ?? ''}
                    onClick={(e) => {
                        if (editing || shouldIgnoreClick()) return;
                        onOpen(request, e.ctrlKey || e.metaKey);
                    }}
                    onMouseDown={(e) => {
                        if (e.button === 1) e.preventDefault();
                    }}
                    onAuxClick={(e) => {
                        if (e.button === 1 && !editing) onOpen(request, true);
                    }}
                    style={{ height: COLLECTION_ROW_HEIGHT }}
                    className={cn(
                        ROW_BASE,
                        'cursor-pointer hover:bg-elevated/35',
                        nested ? 'pl-7' : 'pl-2',
                        dragging && 'opacity-40',
                    )}
                >
                    <span
                        aria-hidden
                        className={cn('h-[7px] w-[7px] shrink-0 rounded-full', safety ? SAFETY_DOT_CLASS[safety] : 'bg-text-tertiary/40')}
                    />
                    {editing ? (
                        <RenameInput
                            value={request.name}
                            label="收藏名"
                            onCommit={(name, byKey) => onCommitRename('request', request.id, name, byKey)}
                            onCancel={() => onCancelRename('request', request.id)}
                        />
                    ) : (
                        <>
                            <span className="min-w-0 flex-1 truncate text-[12px] text-text">{request.name}</span>
                            <span className="min-w-0 max-w-[45%] shrink truncate font-mono text-[11px] text-text-tertiary transition-opacity group-focus-within:opacity-0 group-hover:opacity-0">
                                {request.action}
                            </span>
                            <span
                                className={cn(
                                    'pointer-events-none absolute right-1 flex items-center gap-0.5 rounded-sm bg-surface/90 opacity-0 transition-opacity',
                                    // 被拖着的那一行不露操作按钮
                                    !dragging &&
                                        'group-focus-within:pointer-events-auto group-focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100',
                                )}
                            >
                                <IconAction
                                    icon={Play}
                                    label="在当前 Bot 上发送"
                                    tone="brand"
                                    disabledReason={sendDisabledReason}
                                    focusable={false}
                                    onClick={() => onSend(request)}
                                />
                                <IconAction
                                    icon={ExternalLink}
                                    label="打开"
                                    focusable={false}
                                    onClick={(e) => onOpen(request, e.ctrlKey || e.metaKey)}
                                />
                                <IconAction icon={Pencil} label="重命名" focusable={false} onClick={() => onStartRename('request', request.id)} />
                                <IconAction icon={Trash2} label="删除" tone="danger" focusable={false} onClick={() => onDelete(request)} />
                            </span>
                        </>
                    )}
                </div>
            </ContextMenuTrigger>
            <ContextMenuContent className="w-48" onCloseAutoFocus={menuRename.onCloseAutoFocus}>
                <ContextMenuLabel className="truncate text-2xs">{request.name}</ContextMenuLabel>
                <ContextMenuSeparator />
                <ContextMenuItem onClick={() => onOpen(request, false)}>
                    <ExternalLink size={13} />
                    <span>打开</span>
                </ContextMenuItem>
                <ContextMenuItem onClick={() => onOpen(request, true)}>
                    <SquarePlus size={13} />
                    <span>在新标签打开</span>
                </ContextMenuItem>
                <ContextMenuItem disabled={!!sendDisabledReason} onClick={() => onSend(request)}>
                    <Play size={13} />
                    <span>{sendDisabledReason ? `发送（${sendDisabledReason}）` : '在当前 Bot 上发送'}</span>
                </ContextMenuItem>
                <ContextMenuSeparator />
                <ContextMenuItem onClick={menuRename.pick}>
                    <Pencil size={13} />
                    <span>重命名</span>
                </ContextMenuItem>
                <ContextMenuSub>
                    <ContextMenuSubTrigger>
                        <FolderInput size={13} />
                        <span>移到</span>
                    </ContextMenuSubTrigger>
                    <ContextMenuSubContent className="w-44">
                        <ContextMenuItem disabled={request.folder_id === null} onClick={() => onMoveTo(request.id, null)}>
                            <CornerDownRight size={13} />
                            <span>根目录</span>
                        </ContextMenuItem>
                        {folders.map((f) => (
                            <ContextMenuItem key={f.id} disabled={request.folder_id === f.id} onClick={() => onMoveTo(request.id, f.id)}>
                                <Folder size={13} />
                                <span className="truncate">{f.name}</span>
                            </ContextMenuItem>
                        ))}
                    </ContextMenuSubContent>
                </ContextMenuSub>
                <ContextMenuSeparator />
                <ContextMenuItem tone="danger" onClick={() => onDelete(request)}>
                    <Trash2 size={13} className="text-danger" />
                    <span>删除</span>
                </ContextMenuItem>
            </ContextMenuContent>
        </ContextMenu>
    );
});
