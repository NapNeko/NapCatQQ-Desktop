// 群文件对话框：浏览、搜索、下载、上传与整理。QQ 群文件只有一层文件夹，路径最多两级。
import { useEffect, useMemo, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
    ArrowDownToLine,
    ChevronRight,
    Clock,
    FolderPlus,
    Pencil,
    RefreshCw,
    Search,
    Trash2,
    Upload,
    FolderInput,
    X,
} from 'lucide-react';
import { Dialog, DialogContent, DialogTitle } from '../../../shared/ui/Dialog';
import { Progress } from '../../../shared/ui/Progress';
import { Select } from '../../../shared/ui/Select';
import { Spinner } from '../../../shared/ui/Spinner';
import {
    ContextMenu,
    ContextMenuContent,
    ContextMenuTrigger,
} from '../../../shared/ui/ContextMenu';
import { ActionMotionIcon } from '../../../shared/ui/motion/ActionMotionIcon';
import { cn } from '../../../shared/utils/cn';
import { useTauriFileDrop } from '../../../hooks/ui/useTauriFileDrop';
import { chatGroupFilesService } from '../../../core/services/chat-group-files.service';
import {
    canChangeFile,
    canManageFolders,
    expiryLabel,
    fileDate,
    groupFileRows,
    ROOT_FOLDER,
    type GroupFileRow,
    type GroupFileSort,
} from '../../../core/domain/chat/groupFiles';
import { fileSizeLabel } from '../../../core/domain/debug/chatFormat';
import {
    downloadKey,
    scopeOf,
    startDownload,
    startUpload,
    useFileTransfers,
    type FileTransfer,
} from '../../../hooks/chat/fileTransfers';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import type { GroupFile } from '../../../core/ipc/generated/chat/GroupFile';
import type { GroupFolder } from '../../../core/ipc/generated/chat/GroupFolder';
import type { GroupFileAction } from '../../../core/ipc/generated/chat/GroupFileAction';
import {
    ConfirmDialog,
    FileGlyph,
    MoveDialog,
    NameDialog,
    RowContextItems,
    RowMoreMenu,
    TransferLine,
    type RowAction,
} from './groupFileParts';
import './group-files.css';

const SORTS = [
    { value: 'time', label: '按时间' },
    { value: 'name', label: '按名称' },
    { value: 'size', label: '按大小' },
] as const;
const errorText = (error: unknown) => (error instanceof Error ? error.message : String(error));
type Pending =
    | { kind: 'rename'; file: GroupFile }
    | { kind: 'renameFolder'; folder: GroupFolder }
    | { kind: 'move'; file: GroupFile }
    | { kind: 'delete'; file: GroupFile }
    | { kind: 'deleteFolder'; folder: GroupFolder }
    | { kind: 'create' };

export function GroupFilesDialog({
    open,
    onOpenChange,
    target,
    groupId,
    groupName,
    connected,
    refreshSignal,
}: {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    target: DebugTarget;
    groupId: string;
    groupName: string;
    connected: boolean;
    refreshSignal: string;
}) {
    const queries = useQueryClient();
    const scope = scopeOf(target);
    const [folder, setFolder] = useState<GroupFolder | null>(null);
    const folderId = folder?.folderId ?? ROOT_FOLDER;
    const [query, setQuery] = useState('');
    const [sort, setSort] = useState<GroupFileSort>('time');
    const [limits, setLimits] = useState<Record<string, number>>({});
    const [pending, setPending] = useState<Pending | null>(null);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState('');
    const baseKey = ['chat', 'group-files', scope, groupId] as const;
    const listing = useQuery({
        queryKey: [...baseKey, 'list', folderId, limits[folderId] ?? 0],
        queryFn: () =>
            chatGroupFilesService.list(target, groupId, folder?.folderId ?? null, limits[folderId]),
        enabled: open && connected,
        staleTime: 15_000,
        placeholderData: (previous) => previous,
    });
    const space = useQuery({
        queryKey: [...baseKey, 'space'],
        queryFn: () => chatGroupFilesService.space(target, groupId),
        enabled: open && connected,
        staleTime: 30_000,
    });
    const role = useQuery({
        queryKey: [...baseKey, 'role'],
        queryFn: () => chatGroupFilesService.selfRole(target, groupId),
        enabled: open && connected,
        staleTime: 300_000,
    });
    const roots = useQuery({
        queryKey: [...baseKey, 'list', ROOT_FOLDER, limits[ROOT_FOLDER] ?? 0],
        queryFn: () => chatGroupFilesService.list(target, groupId, null, limits[ROOT_FOLDER]),
        enabled: open && connected && !!pending && pending.kind === 'move',
        staleTime: 15_000,
    });
    const refresh = () => {
        void queries.invalidateQueries({ queryKey: baseKey });
    };
    useEffect(() => {
        if (open && refreshSignal) refresh();
    }, [refreshSignal]);
    useEffect(() => {
        if (!open) {
            setPending(null);
            setError('');
            setQuery('');
        }
    }, [open]);
    const selfRole = role.data ?? null;
    const selfId = String(target.qq_id);
    const data = listing.data;
    const rows = useMemo(
        () => (data ? groupFileRows(data.folders, data.files, sort, query) : []),
        [data, sort, query],
    );
    const transfers = useFileTransfers();
    const downloads = useMemo(
        () =>
            new Map(
                transfers
                    .filter((t) => t.kind === 'download' && t.scope === scope)
                    .map((t) => [t.key, t]),
            ),
        [transfers, scope],
    );
    const uploads = transfers.filter(
        (t) =>
            t.kind === 'upload' &&
            t.scope === scope &&
            t.groupId === groupId &&
            t.folderId === folderId,
    );
    const download = (file: GroupFile) =>
        void startDownload(
            target,
            { kind: 'group', groupId, fileId: file.fileId, busid: file.busid || null },
            file.name,
        );
    const upload = (files: { path: string; name: string }[]) => {
        if (!connected) {
            setError('连接断开时不能上传');
            return;
        }
        for (const file of files) void startUpload(target, groupId, folderId, file, refresh);
    };
    const pick = () => {
        void chatGroupFilesService
            .pickUploads()
            .then(upload)
            .catch((e) => setError(errorText(e)));
    };
    const { dragging } = useTauriFileDrop(open && connected && !pending, (paths) =>
        upload(paths.map((path) => ({ path, name: path.split(/[\\/]/).pop() || path }))),
    );
    const act = async (action: GroupFileAction) => {
        setBusy(true);
        setError('');
        try {
            await chatGroupFilesService.act(target, groupId, action);
            setPending(null);
            refresh();
        } catch (e) {
            setError(errorText(e));
            setPending(null);
            refresh();
        } finally {
            setBusy(false);
        }
    };
    const enter = (next: GroupFolder | null) => {
        setFolder(next);
        setQuery('');
        setError('');
    };
    const manage = canManageFolders(selfRole);
    const fileActions = (file: GroupFile): RowAction[] => {
        const own = canChangeFile(file, selfId, selfRole);
        return [
            {
                key: 'download',
                label: '下载',
                icon: ArrowDownToLine,
                onSelect: () => download(file),
            },
            ...(file.deadTime
                ? [
                      {
                          key: 'persist',
                          label: '转为永久文件',
                          icon: Clock,
                          disabled: !own,
                          onSelect: () => void act({ kind: 'persist', fileId: file.fileId }),
                      },
                  ]
                : []),
            {
                key: 'rename',
                label: '重命名',
                icon: Pencil,
                disabled: !own,
                onSelect: () => setPending({ kind: 'rename', file }),
            },
            {
                key: 'move',
                label: '移动到…',
                icon: FolderInput,
                disabled: !own,
                onSelect: () => setPending({ kind: 'move', file }),
            },
            {
                key: 'delete',
                label: '删除',
                icon: Trash2,
                tone: 'danger',
                disabled: !own,
                onSelect: () => setPending({ kind: 'delete', file }),
            },
        ];
    };
    const folderActions = (entry: GroupFolder): RowAction[] =>
        manage
            ? [
                  ...(data?.features.renameFolder
                      ? [
                            {
                                key: 'rename',
                                label: '重命名',
                                icon: Pencil,
                                onSelect: () => setPending({ kind: 'renameFolder', folder: entry }),
                            },
                        ]
                      : []),
                  {
                      key: 'delete',
                      label: '删除文件夹',
                      icon: Trash2,
                      tone: 'danger',
                      onSelect: () => setPending({ kind: 'deleteFolder', folder: entry }),
                  },
              ]
            : [];
    const used = space.data;
    const usedPercent =
        used && used.totalSpace ? Math.min(100, (used.usedSpace / used.totalSpace) * 100) : 0;
    const limit = data?.limit ?? 0;
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                size="lg"
                hideClose
                className="native-group-files-dialog"
                aria-describedby={undefined}
            >
                <DialogTitle className="native-chat-search-title">
                    <span>群文件</span>
                    {groupName}
                </DialogTitle>
                <button
                    type="button"
                    className="native-chat-icon native-chat-search-close"
                    aria-label="关闭群文件"
                    onClick={() => onOpenChange(false)}
                >
                    <X size={16} />
                </button>
                <div className="native-group-files-body">
                    <div className="native-group-files-toolbar">
                        <nav className="native-group-files-path" aria-label="当前位置">
                            <button type="button" disabled={!folder} onClick={() => enter(null)}>
                                全部文件
                            </button>
                            {folder && (
                                <>
                                    <ChevronRight size={13} aria-hidden />
                                    <span aria-current="page" title={folder.name}>
                                        {folder.name}
                                    </span>
                                </>
                            )}
                        </nav>
                        <label className="native-group-files-search">
                            <Search size={14} aria-hidden />
                            <input
                                value={query}
                                placeholder={folder ? '在此文件夹中搜索' : '搜索文件名或上传者'}
                                aria-label="搜索群文件"
                                onChange={(e) => setQuery(e.target.value)}
                            />
                            {query && (
                                <button
                                    type="button"
                                    aria-label="清除搜索"
                                    onClick={() => setQuery('')}
                                >
                                    <X size={12} />
                                </button>
                            )}
                        </label>
                        <div className="native-group-files-sort">
                            <Select<GroupFileSort>
                                items={SORTS}
                                value={sort}
                                onValueChange={setSort}
                            />
                        </div>
                        <button
                            type="button"
                            className="native-group-file-icon-button"
                            aria-label="刷新"
                            title="刷新"
                            disabled={!connected || listing.isFetching}
                            onClick={refresh}
                        >
                            <ActionMotionIcon
                                icon={RefreshCw}
                                motion={listing.isFetching ? 'spin' : 'none'}
                                size={15}
                            />
                        </button>
                        {!folder && manage && (
                            <button
                                type="button"
                                className="native-group-file-icon-button"
                                aria-label="新建文件夹"
                                title="新建文件夹"
                                disabled={!connected}
                                onClick={() => setPending({ kind: 'create' })}
                            >
                                <FolderPlus size={16} />
                            </button>
                        )}
                        <button
                            type="button"
                            className="native-group-files-upload"
                            disabled={!connected}
                            onClick={pick}
                        >
                            <Upload size={14} aria-hidden />
                            上传
                        </button>
                    </div>
                    {error && (
                        <div className="native-group-files-error" role="alert">
                            <span>{error}</span>
                            <button
                                type="button"
                                aria-label="关闭提示"
                                onClick={() => setError('')}
                            >
                                <X size={12} />
                            </button>
                        </div>
                    )}
                    {uploads.length > 0 && (
                        <ul className="native-group-files-uploads" aria-label="上传">
                            {uploads.map((transfer) => (
                                <li key={transfer.key}>
                                    <FileGlyph name={transfer.name} />
                                    <span className="native-group-file-name" title={transfer.name}>
                                        {transfer.name}
                                    </span>
                                    <TransferLine transfer={transfer} compact />
                                </li>
                            ))}
                        </ul>
                    )}
                    <div
                        className={cn('native-group-files-list', dragging && 'is-dragging')}
                        role="list"
                        aria-label={folder ? `${folder.name}中的文件` : '群文件'}
                        aria-busy={listing.isFetching}
                    >
                        {!connected ? (
                            <p className="native-group-files-empty">
                                连接断开，重新连接后可以浏览群文件
                            </p>
                        ) : listing.isPending ? (
                            <p className="native-group-files-empty">
                                <Spinner size="sm" />
                                正在读取
                            </p>
                        ) : listing.isError && !data ? (
                            <p className="native-group-files-empty">
                                {errorText(listing.error)}
                                <button
                                    type="button"
                                    className="native-chat-text-button"
                                    onClick={() => void listing.refetch()}
                                >
                                    重试
                                </button>
                            </p>
                        ) : !rows.length ? (
                            <p className="native-group-files-empty">
                                {query
                                    ? '没有匹配的文件'
                                    : folder
                                      ? '这个文件夹是空的'
                                      : '还没有群文件'}
                            </p>
                        ) : (
                            rows.map((row) => (
                                <Row
                                    key={
                                        row.kind === 'folder'
                                            ? `d:${row.folder.folderId}`
                                            : `f:${row.file.fileId}`
                                    }
                                    row={row}
                                    transfer={
                                        row.kind === 'file'
                                            ? downloads.get(
                                                  downloadKey(scope, {
                                                      kind: 'group',
                                                      groupId,
                                                      fileId: row.file.fileId,
                                                      busid: null,
                                                  }),
                                              )
                                            : undefined
                                    }
                                    actions={
                                        row.kind === 'folder'
                                            ? folderActions(row.folder)
                                            : fileActions(row.file)
                                    }
                                    onOpen={() =>
                                        row.kind === 'folder'
                                            ? enter(row.folder)
                                            : download(row.file)
                                    }
                                    onRetry={
                                        row.kind === 'file' ? () => download(row.file) : undefined
                                    }
                                />
                            ))
                        )}
                        {data?.more &&
                            !query &&
                            (limit < 5000 ? (
                                <button
                                    type="button"
                                    className="native-group-files-more"
                                    disabled={listing.isFetching}
                                    onClick={() =>
                                        setLimits((current) => ({
                                            ...current,
                                            [folderId]: Math.min(5000, limit * 2),
                                        }))
                                    }
                                >
                                    {listing.isFetching ? '正在读取' : '加载更多'}
                                </button>
                            ) : (
                                <p className="native-group-files-note">只显示前 5000 项</p>
                            ))}
                        {dragging && (
                            <div className="native-group-files-drop">
                                松开上传到{folder ? `「${folder.name}」` : '群文件'}
                            </div>
                        )}
                    </div>
                    <footer className="native-group-files-footer">
                        {used ? (
                            <>
                                <span>
                                    已用 {fileSizeLabel(used.usedSpace)}
                                    {used.totalSpace ? ` / ${fileSizeLabel(used.totalSpace)}` : ''}
                                </span>
                                <Progress
                                    size="sm"
                                    value={usedPercent}
                                    className="native-group-files-space"
                                    aria-label="群文件空间"
                                />
                                <span>
                                    {used.fileCount}
                                    {used.limitCount ? ` / ${used.limitCount}` : ''} 个文件
                                </span>
                            </>
                        ) : (
                            <span>{space.isError ? '空间信息读取失败' : ''}</span>
                        )}
                    </footer>
                </div>
                <NameDialog
                    open={pending?.kind === 'create'}
                    title="新建文件夹"
                    label="文件夹名称"
                    busy={busy}
                    onClose={() => setPending(null)}
                    onSubmit={(name) => void act({ kind: 'createFolder', name })}
                />
                <NameDialog
                    open={pending?.kind === 'rename'}
                    title="重命名文件"
                    label="文件名"
                    initial={pending?.kind === 'rename' ? pending.file.name : ''}
                    busy={busy}
                    onClose={() => setPending(null)}
                    onSubmit={(name) =>
                        pending?.kind === 'rename' &&
                        void act({
                            kind: 'renameFile',
                            fileId: pending.file.fileId,
                            parent: folderId,
                            name,
                        })
                    }
                />
                <NameDialog
                    open={pending?.kind === 'renameFolder'}
                    title="重命名文件夹"
                    label="文件夹名称"
                    initial={pending?.kind === 'renameFolder' ? pending.folder.name : ''}
                    busy={busy}
                    onClose={() => setPending(null)}
                    onSubmit={(name) =>
                        pending?.kind === 'renameFolder' &&
                        void act({ kind: 'renameFolder', folderId: pending.folder.folderId, name })
                    }
                />
                <MoveDialog
                    open={pending?.kind === 'move'}
                    name={pending?.kind === 'move' ? pending.file.name : ''}
                    from={folderId}
                    folders={roots.data?.folders ?? (folder ? [] : (data?.folders ?? []))}
                    busy={busy}
                    onClose={() => setPending(null)}
                    onSubmit={(to) =>
                        pending?.kind === 'move' &&
                        void act({
                            kind: 'moveFile',
                            fileId: pending.file.fileId,
                            name: pending.file.name,
                            from: folderId,
                            to,
                        })
                    }
                />
                <ConfirmDialog
                    open={pending?.kind === 'delete'}
                    title={`删除「${pending?.kind === 'delete' ? pending.file.name : ''}」？`}
                    description="群成员都将无法再下载这个文件，删除后不能恢复。"
                    confirm="删除"
                    busy={busy}
                    onClose={() => setPending(null)}
                    onConfirm={() =>
                        pending?.kind === 'delete' &&
                        void act({
                            kind: 'deleteFile',
                            fileId: pending.file.fileId,
                            busid: pending.file.busid,
                        })
                    }
                />
                <ConfirmDialog
                    open={pending?.kind === 'deleteFolder'}
                    title={`删除文件夹「${pending?.kind === 'deleteFolder' ? pending.folder.name : ''}」？`}
                    description={`里面的 ${pending?.kind === 'deleteFolder' ? pending.folder.fileCount : 0} 个文件会一起删除，不能恢复。`}
                    confirm="删除"
                    busy={busy}
                    onClose={() => setPending(null)}
                    onConfirm={() =>
                        pending?.kind === 'deleteFolder' &&
                        void act({ kind: 'deleteFolder', folderId: pending.folder.folderId })
                    }
                />
            </DialogContent>
        </Dialog>
    );
}

function Row({
    row,
    transfer,
    actions,
    onOpen,
    onRetry,
}: {
    row: GroupFileRow;
    transfer?: FileTransfer;
    actions: RowAction[];
    onOpen: () => void;
    onRetry?: () => void;
}) {
    const name = row.kind === 'folder' ? row.folder.name : row.file.name;
    const expiry = row.kind === 'file' ? expiryLabel(row.file.deadTime) : '';
    const meta =
        row.kind === 'folder'
            ? [
                  `${row.folder.fileCount} 个文件`,
                  row.folder.creatorName,
                  fileDate(row.folder.createTime),
              ]
            : [
                  fileSizeLabel(row.file.size),
                  row.file.uploaderName || row.file.uploader,
                  fileDate(row.file.uploadTime),
              ];
    return (
        <ContextMenu>
            <ContextMenuTrigger asChild>
                <div
                    role="listitem"
                    className={cn('native-group-file-row', row.kind === 'folder' && 'is-folder')}
                >
                    <button
                        type="button"
                        className="native-group-file-main"
                        onClick={onOpen}
                        title={row.kind === 'folder' ? `打开 ${name}` : `下载 ${name}`}
                        aria-label={row.kind === 'folder' ? `打开文件夹 ${name}` : `下载 ${name}`}
                    >
                        <FileGlyph name={name} folder={row.kind === 'folder'} />
                        <span className="native-group-file-text">
                            <span className="native-group-file-name">{name}</span>
                            <span className="native-group-file-meta">
                                {meta.filter(Boolean).join(' · ')}
                                {expiry && (
                                    <em
                                        className={cn(
                                            expiry === '已过期' || expiry === '今天过期'
                                                ? 'is-urgent'
                                                : '',
                                        )}
                                    >
                                        {expiry}
                                    </em>
                                )}
                                {row.kind === 'file' && row.file.downloadTimes > 0 && (
                                    <span> · 下载 {row.file.downloadTimes} 次</span>
                                )}
                            </span>
                        </span>
                    </button>
                    {transfer && <TransferLine transfer={transfer} onRetry={onRetry} />}
                    <span className="native-group-file-actions">
                        {/* 整行已经是下载按钮，这里只是给鼠标的提示，不再占一个 Tab 停靠点 */}
                        {row.kind === 'file' && transfer?.state !== 'running' && (
                            <span
                                className="native-group-file-icon-button"
                                aria-hidden
                                onClick={onOpen}
                            >
                                <ArrowDownToLine size={15} />
                            </span>
                        )}
                        {row.kind === 'folder' && (
                            <ChevronRight size={15} className="text-text-tertiary" aria-hidden />
                        )}
                        <RowMoreMenu actions={actions} label={name} />
                    </span>
                </div>
            </ContextMenuTrigger>
            {actions.length > 0 && (
                <ContextMenuContent className="z-[80]" aria-label={`${name}的操作`}>
                    <RowContextItems actions={actions} />
                </ContextMenuContent>
            )}
        </ContextMenu>
    );
}
