// 终端右边的文件栏（FinalShell / MobaXterm 那种）：跟着终端的当前目录走，能上传下载、新建改名删除、
// 双击小文本直接改。本机终端的文件栏看的是本机目录。

import { useRef, useState } from 'react';
import {
    ArrowUp,
    Copy,
    Download,
    File,
    FileSymlink,
    Folder,
    FolderPlus,
    Link2,
    Link2Off,
    Pencil,
    RefreshCw,
    SquareTerminal,
    Trash2,
    Upload,
} from 'lucide-react';
import {
    Button,
    ContextMenu,
    ContextMenuContent,
    ContextMenuItem,
    ContextMenuSeparator,
    ContextMenuTrigger,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Spinner,
    TextField,
} from '../../shared/ui';
import { cn } from '../../shared/utils/cn';
import { useTerminalFiles } from '../../hooks/terminal/useTerminalFiles';
import { formatBytes, formatModified } from '../../core/domain/terminal/format';
import { invalidFileName, looksLikeText } from '../../core/domain/terminal/paths';
import type { TerminalFileEntry } from '../../core/ipc/generated/domain/TerminalFileEntry';
import type { TerminalHostOs } from '../../core/ipc/generated/domain/TerminalHostOs';
import type { TerminalTextFile } from '../../core/ipc/generated/domain/TerminalTextFile';
import { TerminalFileEditor } from './TerminalFileEditor';

const TEXT_EDIT_LIMIT = 2 * 1024 * 1024;
const WIDTH_MIN = 200;
const WIDTH_MAX = 560;
/** 拖宽文件栏时给终端留的最小宽度 */
const TERMINAL_KEEP = 240;

interface Props {
    sessionId: string;
    hostOs: TerminalHostOs;
    cwd: string | null;
    width: number;
    dropping: boolean;
    /** 在终端里 cd 过去 / 填上路径 */
    onCd(path: string): void;
    onInsertPath(path: string): void;
    /** 拖完左边缘松手时给新宽度 */
    onWidthChange(width: number): void;
    /** 刚被点开，滑进来 */
    slideIn?: boolean;
}

type NameDialog = { kind: 'mkdir' } | { kind: 'rename'; entry: TerminalFileEntry };

export function TerminalFilesPanel({ sessionId, hostOs, cwd, width, dropping, onCd, onInsertPath, onWidthChange, slideIn }: Props) {
    const files = useTerminalFiles(sessionId, hostOs, cwd);
    const asideRef = useRef<HTMLElement>(null);
    const [liveWidth, setLiveWidth] = useState<number | null>(null);
    const [editing, setEditing] = useState<TerminalTextFile | null>(null);
    const [naming, setNaming] = useState<NameDialog | null>(null);
    const [name, setName] = useState('');
    const [deleting, setDeleting] = useState<TerminalFileEntry | null>(null);
    const [selected, setSelected] = useState<string | null>(null);

    const open = async (entry: TerminalFileEntry) => {
        if (entry.is_dir) {
            files.navigate(entry.path);
            return;
        }
        if (looksLikeText(entry.name) && entry.size <= TEXT_EDIT_LIMIT) {
            const file = await files.readText(entry);
            if (file) setEditing(file);
            return;
        }
        await files.download(entry);
    };

    const startNaming = (dialog: NameDialog) => {
        setNaming(dialog);
        setName(dialog.kind === 'rename' ? dialog.entry.name : '');
    };

    const submitName = async () => {
        if (!naming || invalidFileName(name)) return;
        const ok =
            naming.kind === 'mkdir' ? await files.makeDir(name.trim()) : await files.rename(naming.entry, name.trim());
        if (ok) setNaming(null);
    };

    const listingDir = files.listing?.path ?? files.path ?? '';

    // 拖的时候只改本地宽度，松手才落盘
    const startResize = (e: React.PointerEvent) => {
        const aside = asideRef.current;
        const row = aside?.parentElement;
        if (!aside || !row) return;
        e.preventDefault();
        const right = aside.getBoundingClientRect().right;
        const max = Math.max(WIDTH_MIN, Math.min(WIDTH_MAX, row.getBoundingClientRect().width - TERMINAL_KEEP));
        let next = width;
        const move = (ev: PointerEvent) => {
            next = Math.round(Math.min(max, Math.max(WIDTH_MIN, right - ev.clientX)));
            setLiveWidth(next);
        };
        const up = () => {
            window.removeEventListener('pointermove', move);
            window.removeEventListener('pointerup', up);
            setLiveWidth(null);
            if (next !== width) onWidthChange(next);
        };
        window.addEventListener('pointermove', move);
        window.addEventListener('pointerup', up);
        setLiveWidth(width);
    };

    return (
        <aside
            ref={asideRef}
            className={cn(
                'relative flex min-h-0 shrink-0 flex-col border-l border-border-subtle bg-surface',
                slideIn && 'ncd-term-slide-in',
            )}
            style={{ width: liveWidth ?? width }}
            data-terminal-drop={sessionId}
            data-terminal-drop-dir={listingDir}
        >
            <div
                className="ncd-term-files-resize"
                data-dragging={liveWidth !== null}
                onPointerDown={startResize}
                title="拖动改宽度"
            />
            <div className="flex h-8 shrink-0 items-center gap-0.5 border-b border-border-subtle px-1.5">
                <IconButton title="上一级" disabled={!files.listing?.parent} onClick={files.up}>
                    <ArrowUp size={13} />
                </IconButton>
                <IconButton title="刷新" onClick={files.refresh}>
                    <RefreshCw size={13} className={cn(files.loading && 'animate-spin')} />
                </IconButton>
                <IconButton
                    title={files.follow ? '跟着终端的目录走（点一下停）' : '回到终端当前目录，并跟着走'}
                    active={files.follow}
                    onClick={() => files.setFollow(!files.follow)}
                >
                    {files.follow ? <Link2 size={13} /> : <Link2Off size={13} />}
                </IconButton>
                <span className="flex-1" />
                <IconButton title="新建文件夹" onClick={() => startNaming({ kind: 'mkdir' })}>
                    <FolderPlus size={13} />
                </IconButton>
                <IconButton title="上传文件（也可以直接拖进来）" onClick={() => void files.pickAndUpload()}>
                    <Upload size={13} />
                </IconButton>
            </div>
            <div
                className="shrink-0 truncate border-b border-border-subtle px-2.5 py-1 font-mono text-[11px] text-text-tertiary"
                title={listingDir}
            >
                {listingDir || '…'}
            </div>

            <div className="min-h-0 flex-1 overflow-y-auto py-0.5">
                {files.error && (
                    <p className="px-3 py-2 text-[12px] leading-relaxed text-danger">{files.error}</p>
                )}
                {!files.error && files.listing?.entries.length === 0 && (
                    <p className="px-3 py-2 text-[12px] text-text-tertiary">空目录</p>
                )}
                {files.listing?.entries.map((entry) => (
                    <ContextMenu key={entry.path}>
                        <ContextMenuTrigger asChild>
                            <button
                                type="button"
                                className={cn(
                                    'group flex w-full items-center gap-2 px-2.5 py-[3px] text-left text-[12px] text-text',
                                    selected === entry.path ? 'bg-accent-soft' : 'hover:bg-inset',
                                )}
                                onClick={() => setSelected(entry.path)}
                                onDoubleClick={() => void open(entry)}
                                title={entry.mode ? `${entry.name}  ${entry.mode}` : entry.name}
                            >
                                {entry.is_dir ? (
                                    <Folder size={13} className="shrink-0 text-accent" />
                                ) : entry.is_symlink ? (
                                    <FileSymlink size={13} className="shrink-0 text-text-tertiary" />
                                ) : (
                                    <File size={13} className="shrink-0 text-text-tertiary" />
                                )}
                                <span className="min-w-0 flex-1 truncate">{entry.name}</span>
                                <span className="shrink-0 text-[10px] tabular-nums text-text-tertiary">
                                    {entry.is_dir ? formatModified(entry.modified) : formatBytes(entry.size)}
                                </span>
                            </button>
                        </ContextMenuTrigger>
                        <ContextMenuContent className="min-w-[170px]">
                            {entry.is_dir ? (
                                <ContextMenuItem onClick={() => onCd(entry.path)}>
                                    <SquareTerminal size={13} />
                                    <span>在终端里进这个目录</span>
                                </ContextMenuItem>
                            ) : (
                                <>
                                    <ContextMenuItem onClick={() => void open(entry)}>
                                        <Pencil size={13} />
                                        <span>{looksLikeText(entry.name) ? '打开编辑' : '下载'}</span>
                                    </ContextMenuItem>
                                    <ContextMenuItem onClick={() => void files.download(entry)}>
                                        <Download size={13} />
                                        <span>下载到本机</span>
                                    </ContextMenuItem>
                                </>
                            )}
                            <ContextMenuItem onClick={() => onInsertPath(entry.path)}>
                                <SquareTerminal size={13} />
                                <span>把路径填进终端</span>
                            </ContextMenuItem>
                            <ContextMenuItem onClick={() => void navigator.clipboard.writeText(entry.path)}>
                                <Copy size={13} />
                                <span>复制路径</span>
                            </ContextMenuItem>
                            <ContextMenuSeparator />
                            <ContextMenuItem onClick={() => startNaming({ kind: 'rename', entry })}>
                                <Pencil size={13} />
                                <span>改名</span>
                            </ContextMenuItem>
                            <ContextMenuItem tone="danger" onClick={() => setDeleting(entry)}>
                                <Trash2 size={13} />
                                <span>删除</span>
                            </ContextMenuItem>
                        </ContextMenuContent>
                    </ContextMenu>
                ))}
            </div>

            {files.busy && (
                <div className="flex h-7 shrink-0 items-center gap-2 border-t border-border-subtle px-2.5 text-[11px] text-text-secondary">
                    <Spinner size="sm" />
                    {files.busy}…
                </div>
            )}
            {dropping && <div className="ncd-term-drop">松手传到 {listingDir}</div>}

            <TerminalFileEditor file={editing} onSave={files.writeText} onClose={() => setEditing(null)} />

            <Dialog open={naming !== null} onOpenChange={(o) => !o && setNaming(null)}>
                <DialogContent size="sm">
                    <DialogHeader>
                        <DialogTitle>{naming?.kind === 'rename' ? '改名' : '新建文件夹'}</DialogTitle>
                    </DialogHeader>
                    <form
                        onSubmit={(e) => {
                            e.preventDefault();
                            void submitName();
                        }}
                    >
                        <TextField
                            autoFocus
                            value={name}
                            onValueChange={setName}
                            placeholder="名字"
                            aria-label="名字"
                        />
                        <DialogFooter className="mt-4">
                            <Button variant="ghost" onClick={() => setNaming(null)}>
                                取消
                            </Button>
                            <Button type="submit" variant="primary" disabled={invalidFileName(name)}>
                                确定
                            </Button>
                        </DialogFooter>
                    </form>
                </DialogContent>
            </Dialog>

            <Dialog open={deleting !== null} onOpenChange={(o) => !o && setDeleting(null)}>
                <DialogContent size="sm">
                    <DialogHeader>
                        <DialogTitle>删除 {deleting?.name}？</DialogTitle>
                        <DialogDescription>
                            {deleting?.is_dir ? '整个文件夹连里面的东西一起删掉，' : ''}删了找不回来。
                        </DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                        <Button variant="ghost" onClick={() => setDeleting(null)}>
                            取消
                        </Button>
                        <Button
                            variant="danger"
                            onClick={async () => {
                                if (deleting && (await files.remove(deleting))) setDeleting(null);
                            }}
                        >
                            删除
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </aside>
    );
}

function IconButton({
    title,
    onClick,
    disabled,
    active,
    children,
}: {
    title: string;
    onClick(): void;
    disabled?: boolean;
    active?: boolean;
    children: React.ReactNode;
}) {
    return (
        <button
            type="button"
            title={title}
            aria-label={title}
            disabled={disabled}
            onClick={onClick}
            className={cn(
                'flex h-6 w-6 items-center justify-center rounded-xs transition-colors disabled:opacity-40',
                active ? 'bg-accent-soft text-text' : 'text-text-tertiary hover:bg-inset hover:text-text',
            )}
        >
            {children}
        </button>
    );
}
