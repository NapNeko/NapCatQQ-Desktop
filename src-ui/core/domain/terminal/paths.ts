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

/** 文件名里不能出现的字符（两边取并集，远端 Linux 其实只禁 `/`，但留着 `\` 会让人看不懂） */
export function invalidFileName(name: string): boolean {
    const trimmed = name.trim();
    return !trimmed || trimmed === '.' || trimmed === '..' || /[\\/:*?"<>|\x00-\x1f]/.test(trimmed);
}

const TEXT_EXTENSIONS = new Set([
    'txt', 'md', 'log', 'json', 'jsonc', 'json5', 'toml', 'yaml', 'yml', 'ini', 'cfg', 'conf', 'env',
    'py', 'js', 'mjs', 'cjs', 'ts', 'tsx', 'jsx', 'sh', 'bash', 'zsh', 'ps1', 'bat', 'cmd', 'xml',
    'html', 'css', 'service', 'properties', 'lock', 'csv', 'sql', 'gitignore', 'dockerignore',
]);

/** 看着像文本：按扩展名，外加 `.env` / `Dockerfile` 这类没扩展名的常见文件 */
export function looksLikeText(name: string): boolean {
    const lower = name.toLowerCase();
    if (['dockerfile', 'makefile', 'license', 'readme', '.env', '.bashrc', '.profile'].includes(lower)) return true;
    if (lower.startsWith('.env')) return true;
    const dot = lower.lastIndexOf('.');
    if (dot === -1) return false;
    return TEXT_EXTENSIONS.has(lower.slice(dot + 1));
}

export type EditorSyntax = 'json' | 'toml' | 'dot_env' | 'plain';

export function editorSyntaxOf(name: string): EditorSyntax {
    const lower = name.toLowerCase();
    if (lower.endsWith('.json') || lower.endsWith('.jsonc') || lower.endsWith('.json5')) return 'json';
    if (lower.endsWith('.toml')) return 'toml';
    if (lower.startsWith('.env') || lower.endsWith('.env')) return 'dot_env';
    return 'plain';
}
