// 文件：实例目录的文件管理（上游 explorer 插件那套：树是控制台推送，读写删改名走控制台请求）。
// 文本进 CodeMirror 编辑器，图片直接预览，二进制不给改。新建文件 = 写一份空内容。

import { useEffect, useState } from 'react';
import {
    ChevronRight,
    FilePlus2,
    FileText,
    Folder,
    FolderOpen,
    FolderPlus,
    Pencil,
    RotateCw,
    Trash2,
} from 'lucide-react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Spinner,
    SyntaxTextEditor,
    TextField,
} from '../../../../shared/ui';
import type { SyntaxMode } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import { koishiService } from '../../../../core/services/koishi.service';
import { useKoishiExplorerTree, useKoishiFileOps } from '../../../../hooks/apps/useKoishiConsole';
import { PaneLoading } from '../PaneStatus';
import type { AppInstance, KoishiFileEntry } from '../../../../core/ipc/types';

/** 按扩展名挑编辑器的着色模式 */
function modeOf(name: string): SyntaxMode {
    const ext = name.split('.').pop()?.toLowerCase() ?? '';
    switch (ext) {
        case 'json':
            return 'json';
        case 'yml':
        case 'yaml':
            return 'yaml';
        case 'toml':
            return 'toml';
        case 'env':
            return 'dot_env';
        case 'md':
        case 'txt':
        case 'log':
            return 'plain';
        default:
            return 'plain';
    }
}

function decodeFile(base64: string, encoding: string | null): string {
    const bin = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
    // 上游 chardet 对纯 ASCII 内容爱报 ISO-8859-1；这种文件按 UTF-8 解才不吃中文
    const label = encoding?.toLowerCase() ?? '';
    const charset = [
        'iso-8859-1',
        'iso8859-1',
        'latin1',
        'windows-1252',
        'ascii',
        'us-ascii',
    ].includes(label)
        ? 'utf-8'
        : encoding || 'utf-8';
    try {
        return new TextDecoder(charset).decode(bin);
    } catch {
        return new TextDecoder('utf-8').decode(bin);
    }
}

const isImage = (mime: string | null) => !!mime && mime.startsWith('image/');

export const KoishiFilesTab: React.FC<{ instance: AppInstance }> = ({ instance }) => {
    const running = instance.state === 'running';
    const tree = useKoishiExplorerTree(instance.id, running);
    const ops = useKoishiFileOps(instance.id);
    const [openPath, setOpenPath] = useState<string | null>(null);
    const [creating, setCreating] = useState<{ kind: 'file' | 'dir'; parent: string } | null>(null);
    const [renaming, setRenaming] = useState<{ path: string; name: string } | null>(null);
    const [removing, setRemoving] = useState<string | null>(null);

    return (
        <div className="flex min-h-0 flex-1 gap-4 pb-3">
            <aside className="flex w-[260px] shrink-0 flex-col overflow-hidden rounded-lg border border-border-subtle bg-surface shadow-card">
                <div className="flex items-center gap-0.5 border-b border-border-subtle/70 px-2 py-1.5">
                    <span className="px-1 text-2xs text-text-tertiary">实例目录</span>
                    <div className="ml-auto flex items-center">
                        <TreeAction
                            label="新建文件"
                            disabled={!running}
                            onClick={() => setCreating({ kind: 'file', parent: '' })}
                        >
                            <FilePlus2 size={13} />
                        </TreeAction>
                        <TreeAction
                            label="新建文件夹"
                            disabled={!running}
                            onClick={() => setCreating({ kind: 'dir', parent: '' })}
                        >
                            <FolderPlus size={13} />
                        </TreeAction>
                        <TreeAction
                            label="刷新"
                            disabled={!running || tree.isFetching}
                            onClick={() => void tree.refetch()}
                        >
                            <RotateCw size={13} className={cn(tree.isFetching && 'animate-spin')} />
                        </TreeAction>
                    </div>
                </div>
                <div className="min-h-0 flex-1 overflow-y-auto p-1.5">
                    {!running ? (
                        <p className="px-3 py-6 text-center text-xs text-text-tertiary">
                            启动实例后可读写
                        </p>
                    ) : tree.isLoading ? (
                        <PaneLoading text="正在读取文件树…" />
                    ) : tree.error ? (
                        <p className="px-3 py-6 text-center text-xs text-danger">
                            {tree.error.message}
                        </p>
                    ) : (
                        <Tree
                            entries={tree.data ?? []}
                            base=""
                            openPath={openPath}
                            onOpen={setOpenPath}
                            onRename={(path, name) => setRenaming({ path, name })}
                            onRemove={setRemoving}
                        />
                    )}
                </div>
            </aside>

            <section className="flex min-w-0 flex-1 flex-col overflow-hidden rounded-lg border border-border-subtle bg-surface shadow-card">
                {openPath ? (
                    <FileView
                        key={openPath}
                        instance={instance}
                        path={openPath}
                        running={running}
                        ops={ops}
                    />
                ) : (
                    <div className="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-center">
                        <span className="flex h-11 w-11 items-center justify-center rounded-lg bg-inset text-text-tertiary">
                            <FileText size={20} />
                        </span>
                        <p className="max-w-md text-xs leading-relaxed text-text-tertiary">
                            左边挑一个文件查看或编辑；koishi.yml 这类配置更推荐去「原始文件」页改
                        </p>
                    </div>
                )}
            </section>

            <NameDialog
                open={creating !== null}
                title={creating?.kind === 'dir' ? '新建文件夹' : '新建文件'}
                label="名字（可带子路径，如 data/notes/a.txt）"
                onClose={() => setCreating(null)}
                onSubmit={(name) => {
                    const c = creating;
                    if (!c) return;
                    const path = c.parent ? `${c.parent}/${name}` : name;
                    if (c.kind === 'dir') ops.mkdir.mutate(path);
                    else ops.write.mutate({ path, content: '' });
                    setCreating(null);
                    if (c.kind === 'file') setOpenPath(path);
                }}
            />
            <NameDialog
                open={renaming !== null}
                title="改名"
                label="新名字"
                initial={renaming?.name ?? ''}
                onClose={() => setRenaming(null)}
                onSubmit={(name) => {
                    if (!renaming) return;
                    const parent = renaming.path.split('/').slice(0, -1).join('/');
                    const to = parent ? `${parent}/${name}` : name;
                    ops.rename.mutate({ from: renaming.path, to });
                    if (openPath === renaming.path) setOpenPath(to);
                    setRenaming(null);
                }}
            />
            <Dialog open={removing !== null} onOpenChange={(o) => !o && setRemoving(null)}>
                <DialogContent size="sm">
                    <DialogHeader>
                        <DialogTitle>删掉 {removing?.split('/').pop()}？</DialogTitle>
                        <DialogDescription>
                            文件夹会连里面的一起删；实例目录里没有回收站
                        </DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                        <Button variant="ghost" size="sm" onClick={() => setRemoving(null)}>
                            取消
                        </Button>
                        <Button
                            variant="danger"
                            size="sm"
                            disabled={ops.remove.isPending}
                            onClick={() => {
                                if (!removing) return;
                                ops.remove.mutate(removing);
                                if (openPath === removing || openPath?.startsWith(`${removing}/`))
                                    setOpenPath(null);
                                setRemoving(null);
                            }}
                        >
                            删除
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </div>
    );
};

function TreeAction({
    label,
    disabled,
    onClick,
    children,
}: {
    label: string;
    disabled?: boolean;
    onClick: () => void;
    children: React.ReactNode;
}) {
    return (
        <button
            type="button"
            title={label}
            aria-label={label}
            disabled={disabled}
            onClick={onClick}
            className="inline-flex h-7 w-7 items-center justify-center rounded-md text-text-tertiary transition-colors hover:bg-inset hover:text-text disabled:opacity-40"
        >
            {children}
        </button>
    );
}

function Tree({
    entries,
    base,
    openPath,
    onOpen,
    onRename,
    onRemove,
}: {
    entries: KoishiFileEntry[];
    base: string;
    openPath: string | null;
    onOpen: (path: string) => void;
    onRename: (path: string, name: string) => void;
    onRemove: (path: string) => void;
}) {
    const [folded, setFolded] = useState<Record<string, boolean>>({});
    return (
        <ul className="flex flex-col">
            {entries.map((e) => {
                const path = base ? `${base}/${e.name}` : e.name;
                const dir = e.type === 'directory';
                const open = !folded[path];
                return (
                    <li key={path}>
                        <div
                            className={cn(
                                'group flex h-7 items-center gap-1 rounded-md pr-1 text-[12.5px] transition-colors',
                                openPath === path
                                    ? 'bg-brand-soft text-text'
                                    : 'text-text-secondary hover:bg-inset',
                            )}
                        >
                            {dir ? (
                                <button
                                    type="button"
                                    aria-label={open ? '收起' : '展开'}
                                    className="flex h-5 w-4 shrink-0 items-center justify-center text-text-tertiary"
                                    onClick={() => setFolded((f) => ({ ...f, [path]: open }))}
                                >
                                    <ChevronRight
                                        size={12}
                                        className={cn(
                                            'transition-transform duration-200',
                                            open && 'rotate-90',
                                        )}
                                    />
                                </button>
                            ) : (
                                <span className="w-4 shrink-0" />
                            )}
                            <button
                                type="button"
                                className="flex min-w-0 flex-1 items-center gap-1.5 text-left"
                                onClick={() =>
                                    dir ? setFolded((f) => ({ ...f, [path]: open })) : onOpen(path)
                                }
                            >
                                {dir ? (
                                    open ? (
                                        <FolderOpen size={13} className="shrink-0 text-brand/80" />
                                    ) : (
                                        <Folder size={13} className="shrink-0 text-text-tertiary" />
                                    )
                                ) : (
                                    <FileText size={13} className="shrink-0 text-text-tertiary" />
                                )}
                                <span className="truncate">{e.name}</span>
                                {e.type === 'symlink' && e.target && (
                                    <span className="truncate text-2xs text-text-disabled">
                                        → {e.target}
                                    </span>
                                )}
                            </button>
                            <span className="flex shrink-0 items-center opacity-0 transition-opacity group-hover:opacity-100">
                                <TreeAction label="改名" onClick={() => onRename(path, e.name)}>
                                    <Pencil size={11} />
                                </TreeAction>
                                <TreeAction label="删除" onClick={() => onRemove(path)}>
                                    <Trash2 size={11} />
                                </TreeAction>
                            </span>
                        </div>
                        {dir && open && e.children && e.children.length > 0 && (
                            <div className="pl-4">
                                <Tree
                                    entries={e.children}
                                    base={path}
                                    openPath={openPath}
                                    onOpen={onOpen}
                                    onRename={onRename}
                                    onRemove={onRemove}
                                />
                            </div>
                        )}
                    </li>
                );
            })}
        </ul>
    );
}

function FileView({
    instance,
    path,
    running,
    ops,
}: {
    instance: AppInstance;
    path: string;
    running: boolean;
    ops: ReturnType<typeof useKoishiFileOps>;
}) {
    const [state, setState] = useState<
        | { phase: 'loading' }
        | { phase: 'error'; message: string }
        | { phase: 'text'; original: string }
        | { phase: 'image'; dataUrl: string; mime: string }
        | { phase: 'binary'; mime: string | null; size: number }
    >({ phase: 'loading' });
    const [draft, setDraft] = useState('');
    const dirty = state.phase === 'text' && draft !== state.original;

    useEffect(() => {
        let dead = false;
        setState({ phase: 'loading' });
        koishiService
            .explorerRead(instance.id, path)
            .then((f) => {
                if (dead) return;
                if (isImage(f.mime)) {
                    setState({
                        phase: 'image',
                        dataUrl: `data:${f.mime};base64,${f.base64}`,
                        mime: f.mime!,
                    });
                    return;
                }
                if (f.mime && !f.mime.startsWith('text/') && f.mime !== 'application/json') {
                    setState({
                        phase: 'binary',
                        mime: f.mime,
                        size: Math.round((f.base64.length * 3) / 4),
                    });
                    return;
                }
                const text = decodeFile(f.base64, f.encoding);
                setDraft(text);
                setState({ phase: 'text', original: text });
            })
            .catch((e: unknown) => {
                if (!dead)
                    setState({
                        phase: 'error',
                        message: e instanceof Error ? e.message : String(e),
                    });
            });
        return () => {
            dead = true;
        };
    }, [instance.id, path]);

    const name = path.split('/').pop() ?? path;
    return (
        <>
            <div className="flex shrink-0 items-center gap-2 border-b border-border-subtle/70 px-3.5 py-2">
                <span className="min-w-0 truncate font-mono text-[12.5px] text-text">{path}</span>
                <div className="ml-auto flex items-center gap-2">
                    {dirty && <span className="text-2xs text-warning">未保存</span>}
                    {state.phase === 'text' && (
                        <Button
                            size="sm"
                            variant="secondary"
                            disabled={!dirty || !running || ops.write.isPending}
                            onClick={() => {
                                ops.write.mutate({ path, content: draft });
                                if (state.phase === 'text')
                                    setState({ phase: 'text', original: draft });
                            }}
                        >
                            {ops.write.isPending ? <Spinner size="sm" /> : null}
                            保存
                        </Button>
                    )}
                </div>
            </div>
            <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
                {state.phase === 'loading' && <PaneLoading text="正在读取…" />}
                {state.phase === 'error' && (
                    <p className="px-4 py-6 text-center text-xs text-danger">{state.message}</p>
                )}
                {state.phase === 'image' && (
                    <div className="flex flex-1 items-center justify-center overflow-auto p-4">
                        <img
                            src={state.dataUrl}
                            alt={name}
                            className="max-h-full max-w-full rounded-md object-contain"
                        />
                    </div>
                )}
                {state.phase === 'binary' && (
                    <div className="flex flex-1 flex-col items-center justify-center gap-2 p-8 text-center">
                        <p className="text-xs text-text-tertiary">
                            二进制文件（{state.mime ?? '未知类型'}，{state.size}{' '}
                            字节），只能改名或删除
                        </p>
                    </div>
                )}
                {state.phase === 'text' && (
                    <SyntaxTextEditor
                        className="min-h-0 flex-1 rounded-none border-0"
                        value={draft}
                        onChange={setDraft}
                        mode={modeOf(name)}
                        disabled={!running || ops.write.isPending}
                        aria-label={`编辑 ${name}`}
                    />
                )}
            </div>
        </>
    );
}

function NameDialog({
    open,
    title,
    label,
    initial = '',
    onClose,
    onSubmit,
}: {
    open: boolean;
    title: string;
    label: string;
    initial?: string;
    onClose: () => void;
    onSubmit: (name: string) => void;
}) {
    const [name, setName] = useState(initial);
    useEffect(() => setName(initial), [initial, open]);
    return (
        <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
            <DialogContent size="sm">
                <DialogHeader>
                    <DialogTitle>{title}</DialogTitle>
                </DialogHeader>
                <TextField
                    label={label}
                    value={name}
                    autoFocus
                    onValueChange={setName}
                    onKeyDown={(e) => {
                        if (e.key === 'Enter' && name.trim()) onSubmit(name.trim());
                    }}
                />
                <DialogFooter>
                    <Button variant="ghost" size="sm" onClick={onClose}>
                        取消
                    </Button>
                    <Button
                        variant="primary"
                        size="sm"
                        disabled={!name.trim()}
                        onClick={() => onSubmit(name.trim())}
                    >
                        确定
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
