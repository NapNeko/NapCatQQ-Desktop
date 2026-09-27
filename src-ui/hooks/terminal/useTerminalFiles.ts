// 终端旁边的文件栏：跟着 shell 报上来的当前目录走；手动点到别的目录就先不跟了，点「跟随」再回来。

import { useCallback, useEffect, useRef, useState } from 'react';
import { terminalService } from '../../core/services/terminal.service';
import { errorText } from '../../core/domain/errors';
import { baseName, joinHostPath } from '../../core/domain/terminal/paths';
import { pushErrorBar } from '../ui/pushErrorBar';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import type { TerminalDirListing } from '../../core/ipc/generated/domain/TerminalDirListing';
import type { TerminalFileEntry } from '../../core/ipc/generated/domain/TerminalFileEntry';
import type { TerminalHostOs } from '../../core/ipc/generated/domain/TerminalHostOs';
import type { TerminalTextFile } from '../../core/ipc/generated/domain/TerminalTextFile';

export interface TerminalFilesApi {
    path: string | null;
    listing: TerminalDirListing | null;
    loading: boolean;
    error: string | null;
    busy: string | null;
    follow: boolean;
    setFollow(follow: boolean): void;
    navigate(path: string): void;
    up(): void;
    refresh(): void;
    makeDir(name: string): Promise<boolean>;
    rename(entry: TerminalFileEntry, name: string): Promise<boolean>;
    remove(entry: TerminalFileEntry): Promise<boolean>;
    upload(localPaths: string[], destDir?: string): Promise<void>;
    pickAndUpload(): Promise<void>;
    download(entry: TerminalFileEntry): Promise<void>;
    readText(entry: TerminalFileEntry): Promise<TerminalTextFile | null>;
    writeText(file: TerminalTextFile, content: string): Promise<boolean>;
}

// 拖进面板的文件由面板统一处理，传完要通知这个会话的文件栏重新列一遍
const refreshers = new Map<string, Set<() => void>>();

function notifyRefresh(sessionId: string) {
    refreshers.get(sessionId)?.forEach((fn) => fn());
}

/** 往某个会话所在的主机传文件（拖进终端 / 文件栏时用），传完刷新它的文件栏 */
export async function uploadToSession(sessionId: string, localPaths: string[], destDir: string): Promise<void> {
    if (localPaths.length === 0) return;
    // 同一个 key 后推的顶掉先推的：「正在上传」换成结果
    const key = `terminal-upload:${sessionId}:${Date.now()}`;
    pushInfoBar({ key, tone: 'info', title: `正在上传 ${localPaths.length} 项…`, content: destDir, autoDismissMs: 0 });
    try {
        const count = await terminalService.upload(sessionId, localPaths, destDir);
        pushInfoBar({ key, tone: 'success', title: `已上传 ${count} 个文件`, content: destDir, autoDismissMs: 4000 });
    } catch (err) {
        pushErrorBar({ key, title: '上传失败', raw: errorText(err) });
    } finally {
        notifyRefresh(sessionId);
    }
}

export function useTerminalFiles(sessionId: string, hostOs: TerminalHostOs, cwd: string | null): TerminalFilesApi {
    const [path, setPath] = useState<string | null>(cwd);
    const [follow, setFollow] = useState(true);
    const [listing, setListing] = useState<TerminalDirListing | null>(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [busy, setBusy] = useState<string | null>(null);
    const seq = useRef(0);

    useEffect(() => {
        if (follow && cwd && cwd !== path) setPath(cwd);
    }, [cwd, follow, path]);

    const load = useCallback(
        async (target: string) => {
            const ticket = ++seq.current;
            setLoading(true);
            try {
                const next = await terminalService.listDir(sessionId, target);
                if (ticket !== seq.current) return;
                setListing(next);
                setError(null);
                if (next.path !== target) setPath(next.path);
            } catch (err) {
                if (ticket !== seq.current) return;
                setError(errorText(err));
            } finally {
                if (ticket === seq.current) setLoading(false);
            }
        },
        [sessionId],
    );

    useEffect(() => {
        if (path) void load(path);
    }, [path, load]);

    useEffect(() => {
        const refresh = () => {
            if (path) void load(path);
        };
        const set = refreshers.get(sessionId) ?? new Set<() => void>();
        set.add(refresh);
        refreshers.set(sessionId, set);
        return () => {
            set.delete(refresh);
            if (set.size === 0) refreshers.delete(sessionId);
        };
    }, [sessionId, path, load]);

    const navigate = useCallback(
        (next: string) => {
            setPath(next);
            if (next !== cwd) setFollow(false);
        },
        [cwd],
    );

    const run = useCallback(
        async (label: string, action: () => Promise<unknown>): Promise<boolean> => {
            setBusy(label);
            try {
                await action();
                return true;
            } catch (err) {
                pushErrorBar({ title: `${label}失败`, raw: errorText(err) });
                return false;
            } finally {
                setBusy(null);
                if (path) void load(path);
            }
        },
        [load, path],
    );

    const upload = useCallback(
        async (localPaths: string[], destDir?: string) => {
            const dest = destDir ?? listing?.path ?? path;
            if (!dest || localPaths.length === 0) return;
            await run(`上传 ${localPaths.length} 项`, async () => {
                const count = await terminalService.upload(sessionId, localPaths, dest);
                pushInfoBar({ tone: 'success', title: `已上传 ${count} 个文件`, content: dest, autoDismissMs: 4000 });
            });
        },
        [listing?.path, path, run, sessionId],
    );

    return {
        path,
        listing,
        loading,
        error,
        busy,
        follow,
        setFollow(next) {
            setFollow(next);
            if (next && cwd) setPath(cwd);
        },
        navigate,
        up() {
            if (listing?.parent) navigate(listing.parent);
        },
        refresh() {
            if (path) void load(path);
        },
        makeDir: (name) =>
            run('新建文件夹', () => terminalService.makeDir(sessionId, joinHostPath(hostOs, listing?.path ?? path ?? '', name))),
        rename: (entry, name) => {
            const dir = entry.path.slice(0, entry.path.length - baseName(entry.path).length).replace(/[\\/]$/, '');
            return run('改名', () => terminalService.rename(sessionId, entry.path, joinHostPath(hostOs, dir || '/', name)));
        },
        remove: (entry) => run('删除', () => terminalService.remove(sessionId, entry.path, entry.is_dir)),
        upload,
        async pickAndUpload() {
            const picked = await terminalService.pickUploadFiles();
            if (picked.length) await upload(picked);
        },
        async download(entry) {
            const dest = await terminalService.pickSaveTarget(`下载 ${entry.name}`, entry.name);
            if (!dest) return;
            await run('下载', async () => {
                await terminalService.download(sessionId, entry.path, dest);
                pushInfoBar({ tone: 'success', title: '已下载', content: dest, autoDismissMs: 4000 });
            });
        },
        async readText(entry) {
            try {
                return await terminalService.readText(sessionId, entry.path);
            } catch (err) {
                pushErrorBar({ title: `打不开 ${entry.name}`, raw: errorText(err) });
                return null;
            }
        },
        writeText: (file, content) =>
            run('保存', () => terminalService.writeText(sessionId, file.path, content, file.crlf)),
    };
}
