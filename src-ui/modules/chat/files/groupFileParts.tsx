// 群文件对话框里的小件：文件图标、传输进度、改名 / 移动 / 新建弹窗、行操作菜单。
import { useEffect, useState, type ReactNode } from 'react';
import {
    Check,
    File,
    FileArchive,
    FileAudio,
    FileCode,
    FileImage,
    FileSpreadsheet,
    FileText,
    FileVideo,
    Folder,
    FolderOpen,
    MoreHorizontal,
    Package,
    Presentation,
    RotateCcw,
    X,
} from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Progress,
    TextField,
} from '../../../shared/ui';
import { Popover, PopoverContent, PopoverTrigger } from '../../../shared/ui/Popover';
import { ContextMenuItem } from '../../../shared/ui/ContextMenu';
import { cn } from '../../../shared/utils/cn';
import {
    fileKind,
    transferPercent,
    ROOT_FOLDER,
    type FileKind,
} from '../../../core/domain/chat/groupFiles';
import { fileSizeLabel } from '../../../core/domain/debug/chatFormat';
import type { GroupFolder } from '../../../core/ipc/generated/chat/GroupFolder';
import {
    cancelTransfer,
    dismissTransfer,
    openDownload,
    type FileTransfer,
} from '../../../hooks/chat/fileTransfers';

const ICONS: Record<FileKind, LucideIcon> = {
    image: FileImage,
    video: FileVideo,
    audio: FileAudio,
    archive: FileArchive,
    sheet: FileSpreadsheet,
    slide: Presentation,
    doc: FileText,
    code: FileCode,
    app: Package,
    other: File,
};

export function FileGlyph({ name, folder }: { name: string; folder?: boolean }) {
    const Icon = folder ? Folder : ICONS[fileKind(name)];
    return (
        <span
            className={cn('native-group-file-glyph', folder ? 'is-folder' : `is-${fileKind(name)}`)}
            aria-hidden
        >
            <Icon size={18} strokeWidth={1.6} />
        </span>
    );
}

export function transferText(transfer: FileTransfer): string {
    if (transfer.state === 'failed') return transfer.error || '传输失败';
    if (transfer.state === 'cancelled') return '已取消';
    if (transfer.state === 'done') return transfer.kind === 'download' ? '已下载' : '已上传';
    const done = transfer.progress?.done_bytes ?? 0;
    const total = transfer.progress?.total_bytes;
    const verb =
        transfer.kind === 'download'
            ? '下载中'
            : transfer.progress?.stage === 'calling'
              ? '正在发到群里'
              : '上传中';
    if (!transfer.progress) return transfer.kind === 'download' ? '正在获取链接' : '准备上传';
    return total
        ? `${verb} ${fileSizeLabel(done)} / ${fileSizeLabel(total)}`
        : `${verb} ${fileSizeLabel(done)}`;
}

/** 行内传输状态：进行中给进度和取消，下完给打开 / 定位，失败给原因 */
export function TransferLine({
    transfer,
    onRetry,
    compact,
}: {
    transfer: FileTransfer;
    onRetry?: () => void;
    compact?: boolean;
}) {
    const percent = transferPercent(transfer.progress);
    const [openError, setOpenError] = useState('');
    const open = (reveal: boolean) => {
        setOpenError('');
        void openDownload(transfer, reveal).catch((error) =>
            setOpenError(error instanceof Error ? error.message : String(error)),
        );
    };
    return (
        <span
            className={cn(
                'native-group-file-transfer',
                `is-${transfer.state}`,
                compact && 'is-compact',
            )}
            role="status"
        >
            {transfer.state === 'running' && (
                <Progress
                    size="sm"
                    value={percent ?? undefined}
                    indeterminate={percent === null}
                    aria-label={transferText(transfer)}
                />
            )}
            <span className="native-group-file-transfer-row">
                <span
                    className="native-group-file-transfer-text"
                    title={openError || transferText(transfer)}
                >
                    {openError || transferText(transfer)}
                </span>
                {transfer.state === 'running' && (
                    <button type="button" onClick={() => cancelTransfer(transfer.key)}>
                        取消
                    </button>
                )}
                {transfer.state === 'done' && transfer.kind === 'download' && (
                    <>
                        <button type="button" onClick={() => open(false)}>
                            打开
                        </button>
                        <button type="button" onClick={() => open(true)}>
                            在文件夹中显示
                        </button>
                    </>
                )}
                {(transfer.state === 'failed' || transfer.state === 'cancelled') && onRetry && (
                    <button type="button" onClick={onRetry}>
                        <RotateCcw size={11} aria-hidden />
                        重试
                    </button>
                )}
                {transfer.state !== 'running' && (
                    <button
                        type="button"
                        aria-label="收起"
                        onClick={() => dismissTransfer(transfer.key)}
                    >
                        <X size={11} aria-hidden />
                    </button>
                )}
            </span>
        </span>
    );
}

export interface RowAction {
    key: string;
    label: string;
    icon: LucideIcon;
    onSelect: () => void;
    tone?: 'danger';
    disabled?: boolean;
}

export function RowContextItems({ actions }: { actions: RowAction[] }) {
    return (
        <>
            {actions.map((action) => (
                <ContextMenuItem
                    key={action.key}
                    tone={action.tone ?? 'default'}
                    disabled={action.disabled}
                    onSelect={action.onSelect}
                >
                    <action.icon size={14} />
                    {action.label}
                </ContextMenuItem>
            ))}
        </>
    );
}

/** 悬停出现的「更多」，和右键菜单是同一份操作 */
export function RowMoreMenu({ actions, label }: { actions: RowAction[]; label: string }) {
    const [open, setOpen] = useState(false);
    if (!actions.length) return null;
    return (
        <Popover open={open} onOpenChange={setOpen}>
            <PopoverTrigger asChild>
                <button
                    type="button"
                    className="native-group-file-icon-button"
                    aria-label={`${label}的更多操作`}
                >
                    <MoreHorizontal size={15} />
                </button>
            </PopoverTrigger>
            <PopoverContent
                align="end"
                className="native-group-file-menu z-[80]"
                onCloseAutoFocus={(event) => event.preventDefault()}
            >
                {actions.map((action) => (
                    <button
                        key={action.key}
                        type="button"
                        className={cn(action.tone === 'danger' && 'is-danger')}
                        disabled={action.disabled}
                        onClick={() => {
                            setOpen(false);
                            action.onSelect();
                        }}
                    >
                        <action.icon size={14} />
                        {action.label}
                    </button>
                ))}
            </PopoverContent>
        </Popover>
    );
}

export function NameDialog({
    open,
    title,
    label,
    initial = '',
    busy,
    onClose,
    onSubmit,
}: {
    open: boolean;
    title: string;
    label: string;
    initial?: string;
    busy?: boolean;
    onClose: () => void;
    onSubmit: (name: string) => void;
}) {
    const [name, setName] = useState(initial);
    useEffect(() => {
        if (open) setName(initial);
    }, [initial, open]);
    const value = name.trim();
    const invalid = /[\\/:*?"<>|]/.test(value) ? '名称不能包含 \\ / : * ? " < > |' : '';
    const submit = () => {
        if (value && !invalid && value !== initial) onSubmit(value);
    };
    return (
        <Dialog open={open} onOpenChange={(next) => !next && !busy && onClose()}>
            <DialogContent size="sm" layer={1}>
                <DialogHeader>
                    <DialogTitle>{title}</DialogTitle>
                </DialogHeader>
                <TextField
                    label={label}
                    value={name}
                    autoFocus
                    error={invalid || undefined}
                    onValueChange={setName}
                    onKeyDown={(event) => {
                        if (event.key === 'Enter' && !event.nativeEvent.isComposing) submit();
                    }}
                    onFocus={(event) => {
                        // 改文件名时默认只选主名，不选扩展名
                        const dot = event.currentTarget.value.lastIndexOf('.');
                        if (dot > 0) event.currentTarget.setSelectionRange(0, dot);
                        else event.currentTarget.select();
                    }}
                />
                <DialogFooter>
                    <Button variant="ghost" size="sm" disabled={busy} onClick={onClose}>
                        取消
                    </Button>
                    <Button
                        variant="primary"
                        size="sm"
                        disabled={busy || !value || !!invalid || value === initial}
                        onClick={submit}
                    >
                        确定
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}

export function MoveDialog({
    open,
    name,
    from,
    folders,
    busy,
    onClose,
    onSubmit,
}: {
    open: boolean;
    name: string;
    from: string;
    folders: GroupFolder[];
    busy?: boolean;
    onClose: () => void;
    onSubmit: (to: string) => void;
}) {
    const [to, setTo] = useState('');
    useEffect(() => {
        if (open) setTo('');
    }, [open]);
    const choices = [
        { id: ROOT_FOLDER, name: '群文件（根目录）' },
        ...folders.map((folder) => ({ id: folder.folderId, name: folder.name })),
    ].filter((choice) => choice.id !== from);
    return (
        <Dialog open={open} onOpenChange={(next) => !next && !busy && onClose()}>
            <DialogContent size="sm" layer={1}>
                <DialogHeader>
                    <DialogTitle>移动「{name}」</DialogTitle>
                </DialogHeader>
                <div className="native-group-file-move" role="listbox" aria-label="目标文件夹">
                    {choices.map((choice) => (
                        <button
                            key={choice.id}
                            type="button"
                            role="option"
                            aria-selected={to === choice.id}
                            onClick={() => setTo(choice.id)}
                            onDoubleClick={() => onSubmit(choice.id)}
                        >
                            {choice.id === ROOT_FOLDER ? (
                                <FolderOpen size={16} aria-hidden />
                            ) : (
                                <Folder size={16} aria-hidden />
                            )}
                            <span>{choice.name}</span>
                            {to === choice.id && <Check size={14} aria-hidden />}
                        </button>
                    ))}
                    {!choices.length && <p>没有别的文件夹</p>}
                </div>
                <DialogFooter>
                    <Button variant="ghost" size="sm" disabled={busy} onClick={onClose}>
                        取消
                    </Button>
                    <Button
                        variant="primary"
                        size="sm"
                        disabled={busy || !to}
                        onClick={() => onSubmit(to)}
                    >
                        移动
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}

export function ConfirmDialog({
    open,
    title,
    description,
    confirm,
    busy,
    onClose,
    onConfirm,
}: {
    open: boolean;
    title: string;
    description: ReactNode;
    confirm: string;
    busy?: boolean;
    onClose: () => void;
    onConfirm: () => void;
}) {
    return (
        <Dialog open={open} onOpenChange={(next) => !next && !busy && onClose()}>
            <DialogContent size="sm" layer={1} hideClose dismissOnOutsideClick={!busy}>
                <DialogHeader>
                    <DialogTitle>{title}</DialogTitle>
                    <p className="text-xs leading-relaxed text-text-secondary">{description}</p>
                </DialogHeader>
                <DialogFooter>
                    <Button variant="ghost" size="sm" disabled={busy} onClick={onClose}>
                        取消
                    </Button>
                    <Button variant="danger" size="sm" disabled={busy} onClick={onConfirm}>
                        {confirm}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
