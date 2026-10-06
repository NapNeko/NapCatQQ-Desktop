// 往终端里填路径 / cd 命令时按 shell 的规矩加引号；文件栏拼路径按主机的分隔符。

import type { LocalShellKind } from '../../ipc/generated/domain/LocalShellKind';
import type { TerminalHostOs } from '../../ipc/generated/domain/TerminalHostOs';

export type ShellSyntax = 'posix' | 'powershell' | 'cmd' | 'wsl';

export function shellSyntaxOf(os: TerminalHostOs, shell: LocalShellKind | undefined): ShellSyntax {
    if (os === 'linux') return 'posix';
    switch (shell) {
        case 'cmd':
            return 'cmd';
        case 'git_bash':
            return 'posix';
        case 'wsl':
            return 'wsl';
        default:
            return 'powershell';
    }
}

const SAFE_POSIX = /^[A-Za-z0-9_@%+=:,./-]+$/;
const SAFE_WIN = /^[A-Za-z0-9_@%+=:,./\\-]+$/;

/** `C:\a\b` → `/mnt/c/a/b`（WSL 里看本机路径） */
export function toWslPath(path: string): string {
    const m = /^([A-Za-z]):[\\/](.*)$/.exec(path);
    if (!m) return path.replace(/\\/g, '/');
    return `/mnt/${(m[1] as string).toLowerCase()}/${(m[2] as string).replace(/\\/g, '/')}`;
}

export function quotePath(path: string, syntax: ShellSyntax): string {
    switch (syntax) {
        case 'posix':
        case 'wsl': {
            const p = syntax === 'wsl' ? toWslPath(path) : path;
            return SAFE_POSIX.test(p) ? p : `'${p.replace(/'/g, `'\\''`)}'`;
        }
        case 'powershell':
            return SAFE_WIN.test(path) ? path : `'${path.replace(/'/g, "''")}'`;
        case 'cmd':
            return SAFE_WIN.test(path) ? path : `"${path.replace(/"/g, '')}"`;
    }
}

export function cdCommand(path: string, syntax: ShellSyntax): string {
    switch (syntax) {
        case 'powershell':
            return `Set-Location -LiteralPath ${quotePath(path, syntax)}`;
        case 'cmd':
            return `cd /d ${quotePath(path, syntax)}`;
        default:
            return `cd -- ${quotePath(path, syntax)}`;
    }
}

export function joinHostPath(os: TerminalHostOs, dir: string, name: string): string {
    const sep = os === 'windows' ? '\\' : '/';
    return dir.endsWith(sep) ? `${dir}${name}` : `${dir}${sep}${name}`;
}

export function baseName(path: string): string {
    const trimmed = path.replace(/[\\/]+$/, '');
    const cut = Math.max(trimmed.lastIndexOf('/'), trimmed.lastIndexOf('\\'));
    return cut === -1 ? trimmed : trimmed.slice(cut + 1);
}

/** 文件栏里 Windows「此电脑」（列各个盘）用空路径表示 */
export const DRIVES_PATH = '';

export function isDrivesView(os: TerminalHostOs, path: string | null | undefined): boolean {
    return os === 'windows' && path === DRIVES_PATH;
}

export interface Crumb {
    label: string;
    path: string;
}

/// 路径栏的面包屑，每一段都能点。Windows 最前面是「此电脑」，然后是盘；Linux 从 `/` 开始
export function breadcrumbs(os: TerminalHostOs, path: string): Crumb[] {
    if (os === 'windows') {
        const crumbs: Crumb[] = [{ label: '此电脑', path: DRIVES_PATH }];
        const m = /^([A-Za-z]:)\\?(.*)$/.exec(path);
        if (!m) return path ? [...crumbs, { label: path, path }] : crumbs;
        const drive = m[1] as string;
        let current = `${drive}\\`;
        crumbs.push({ label: drive, path: current });
        for (const part of (m[2] as string).split('\\').filter(Boolean)) {
            current = current.endsWith('\\') ? `${current}${part}` : `${current}\\${part}`;
            crumbs.push({ label: part, path: current });
        }
        return crumbs;
    }
    const crumbs: Crumb[] = [{ label: '/', path: '/' }];
    let current = '';
    for (const part of path.split('/').filter(Boolean)) {
        current = `${current}/${part}`;
        crumbs.push({ label: part, path: current });
    }
    return crumbs;
}

/// 从 from 到 to 是往里走（进子目录、从此电脑进盘）还是往外 / 跳到别处；决定列表从哪边滑进来
export function navDirection(from: string, to: string): 'in' | 'out' {
    if (from === DRIVES_PATH) return 'in';
    if (to === DRIVES_PATH) return 'out';
    const base = from.replace(/[\\/]+$/, '');
    return to.length > base.length && to.startsWith(base) && /[\\/]/.test(to.charAt(base.length))
        ? 'in'
        : 'out';
}

/** 文件名里不能出现的字符（两边取并集，远端 Linux 其实只禁 `/`，但留着 `\` 会让人看不懂） */
export function invalidFileName(name: string): boolean {
    const trimmed = name.trim();
    return !trimmed || trimmed === '.' || trimmed === '..' || /[\\/:*?"<>|\x00-\x1f]/.test(trimmed);
}

const TEXT_EXTENSIONS = new Set([
    'txt',
    'md',
    'log',
    'json',
    'jsonc',
    'json5',
    'toml',
    'yaml',
    'yml',
    'ini',
    'cfg',
    'conf',
    'env',
    'py',
    'js',
    'mjs',
    'cjs',
    'ts',
    'tsx',
    'jsx',
    'sh',
    'bash',
    'zsh',
    'ps1',
    'bat',
    'cmd',
    'xml',
    'html',
    'css',
    'service',
    'properties',
    'lock',
    'csv',
    'sql',
    'gitignore',
    'dockerignore',
]);

/** 看着像文本：按扩展名，外加 `.env` / `Dockerfile` 这类没扩展名的常见文件 */
export function looksLikeText(name: string): boolean {
    const lower = name.toLowerCase();
    if (
        ['dockerfile', 'makefile', 'license', 'readme', '.env', '.bashrc', '.profile'].includes(
            lower,
        )
    )
        return true;
    if (lower.startsWith('.env')) return true;
    const dot = lower.lastIndexOf('.');
    if (dot === -1) return false;
    return TEXT_EXTENSIONS.has(lower.slice(dot + 1));
}

export type EditorSyntax = 'json' | 'toml' | 'dot_env' | 'plain';

export function editorSyntaxOf(name: string): EditorSyntax {
    const lower = name.toLowerCase();
    if (lower.endsWith('.json') || lower.endsWith('.jsonc') || lower.endsWith('.json5'))
        return 'json';
    if (lower.endsWith('.toml')) return 'toml';
    if (lower.startsWith('.env') || lower.endsWith('.env')) return 'dot_env';
    return 'plain';
}
