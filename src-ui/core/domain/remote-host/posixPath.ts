// Linux 远端路径：只处理 POSIX，不碰 Windows。

export function normalizePosix(path: string): string {
    const raw = path.trim() || '/';
    const input = raw.startsWith('/') ? raw : `/${raw}`;
    const stack: string[] = [];
    for (const part of input.split('/')) {
        if (!part || part === '.') continue;
        if (part === '..') {
            stack.pop();
            continue;
        }
        stack.push(part);
    }
    return stack.length === 0 ? '/' : `/${stack.join('/')}`;
}

export function joinPosix(parent: string, name: string): string {
    const n = name.replace(/^\/+|\/+$/g, '');
    if (!n || n === '.') return normalizePosix(parent);
    if (n === '..') return parentPosix(parent);
    return normalizePosix(`${normalizePosix(parent)}/${n}`);
}

export function parentPosix(path: string): string {
    const n = normalizePosix(path);
    if (n === '/') return '/';
    const i = n.lastIndexOf('/');
    return i <= 0 ? '/' : n.slice(0, i);
}

export function posixSegments(path: string): string[] {
    const n = normalizePosix(path);
    return n === '/' ? [] : n.slice(1).split('/');
}

export function isPosixAbsolute(path: string): boolean {
    return path.trim().startsWith('/');
}

/** `remote:<serverId>` → serverId；本机或空则 null。 */
export function remoteServerIdFromHostId(hostId: string): string | null {
    if (!hostId.startsWith('remote:')) return null;
    const id = hostId.slice('remote:'.length).trim();
    return id || null;
}
