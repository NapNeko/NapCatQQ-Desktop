// 浏览器预览里的假终端：一个会回显、认几条命令、发命令标记的小 shell，
// 够走查面板、标签、分屏、文件栏、状态条、sudo 代填、关键字高亮。

import type { TerminalAttachHandlers } from '../../services/terminal.service';
import type { LocalShellOption } from '../generated/domain/LocalShellOption';
import type { ServerStats } from '../generated/domain/ServerStats';
import type { TerminalDirListing } from '../generated/domain/TerminalDirListing';
import type { TerminalFileEntry } from '../generated/domain/TerminalFileEntry';
import type { TerminalOpenRequest } from '../generated/domain/TerminalOpenRequest';
import type { TerminalSessionInfo } from '../generated/domain/TerminalSessionInfo';
import type { TerminalTarget } from '../generated/domain/TerminalTarget';
import type { TerminalTextFile } from '../generated/domain/TerminalTextFile';

const encoder = new TextEncoder();
const ESC = '\x1b';

interface MockSession {
    info: TerminalSessionInfo;
    cwd: string;
    line: string;
    history: string;
    handlers: TerminalAttachHandlers | null;
    sudoPending: string | null;
}

const sessions = new Map<string, MockSession>();
let nextId = 1;

const FILES: Record<string, TerminalFileEntry[]> = {};
function dir(path: string, entries: [string, boolean, number][]) {
    FILES[path] = entries.map(([name, isDir, size]) => ({
        name,
        path: path === '/' ? `/${name}` : `${path}/${name}`,
        is_dir: isDir,
        is_symlink: false,
        size,
        modified: Math.floor(Date.now() / 1000) - size,
        mode: isDir ? 'rwxr-xr-x' : 'rw-r--r--',
    }));
}
dir('/home/napcat', [
    ['ncd', true, 0],
    ['Napcat', true, 0],
    ['.bashrc', false, 3771],
    ['notes.txt', false, 120],
]);
dir('/home/napcat/ncd', [['apps', true, 0], ['tools', true, 0]]);
dir('/home/napcat/ncd/apps', [['maibot', true, 0]]);
dir('/home/napcat/ncd/apps/maibot', [['m1', true, 0]]);
dir('/home/napcat/ncd/apps/maibot/m1', [
    ['.venv', true, 0],
    ['plugins', true, 0],
    ['config', true, 0],
    ['bot.py', false, 4096],
    ['pyproject.toml', false, 2310],
    ['maibot.log', false, 88231],
]);

const TEXTS: Record<string, string> = {
    '/home/napcat/notes.txt': '# 备忘\n麦麦的配置在 ncd/apps/maibot/m1/config\n',
    '/home/napcat/ncd/apps/maibot/m1/pyproject.toml': '[project]\nname = "maibot"\nversion = "1.2.5"\n',
};

function titleFor(target: TerminalTarget): { title: string; host: string; linux: boolean; cwd: string } {
    switch (target.kind) {
        case 'local':
            return { title: '本机 · PowerShell 7', host: '本机', linux: false, cwd: 'C:\\Users\\napcat' };
        case 'server':
            return { title: target.server_id, host: target.server_id, linux: true, cwd: '/home/napcat' };
        case 'bot':
            return {
                title: target.host_dir ? `Bot ${target.bot_id} 部署目录 · vps1` : `Bot ${target.bot_id} · vps1`,
                host: 'vps1',
                linux: true,
                cwd: '/home/napcat/Napcat',
            };
        case 'app_instance':
            return { title: '麦麦 · vps1', host: 'vps1', linux: true, cwd: '/home/napcat/ncd/apps/maibot/m1' };
    }
}

function emit(s: MockSession, text: string) {
    s.history = (s.history + text).slice(-200_000);
    s.handlers?.output(encoder.encode(text));
}

function osc(body: string) {
    return `${ESC}]${body}\x07`;
}

function prompt(s: MockSession) {
    if (s.info.host_os === 'windows') {
        emit(s, `${osc(`633;P;Cwd=${s.cwd.replace(/\\/g, '\\\\')}`)}${osc('633;A')}PS ${s.cwd}> ${osc('633;B')}`);
        return;
    }
    const shown = s.cwd.replace(/^\/home\/napcat/, '~');
    emit(s, `${osc(`633;P;Cwd=${s.cwd}`)}${osc('633;A')}${ESC}[32mnapcat@vps1${ESC}[0m:${ESC}[34m${shown}${ESC}[0m$ ${osc('633;B')}`);
}

function resolvePath(cwd: string, arg: string): string {
    if (!arg || arg === '~') return '/home/napcat';
    const base = arg.startsWith('/') ? [] : cwd.split('/').filter(Boolean);
    for (const part of arg.replace(/^~\//, '/home/napcat/').split('/')) {
        if (!part || part === '.') continue;
        if (part === '..') base.pop();
        else base.push(part);
    }
    return `/${base.join('/')}`;
}

const DEMO_LOG = [
    '2026-09-27 18:02:11 [INFO] 麦麦启动中，WebUI 监听 127.0.0.1:8001',
    '2026-09-27 18:02:12 [INFO] 连接 ws://127.0.0.1:3001 成功',
    '2026-09-27 18:02:15 [WARNING] 模型 deepseek-chat 响应偏慢 (3.2s)',
    '2026-09-27 18:02:18 [ERROR] 插件 hello_world 加载失败: No module named "requests"',
    '2026-09-27 18:02:18 [DEBUG] 详情见 https://docs.mai-mai.org/plugins',
    '2026-09-27 18:02:20 [INFO] 插件加载完成，Done.',
];

function run(s: MockSession, raw: string) {
    const command = raw.trim();
    const [name = '', ...args] = command.split(/\s+/);
    let code = 0;
    const out: string[] = [];
    if (!command) {
        prompt(s);
        return;
    }
    emit(s, `${osc(`633;E;${command.replace(/;/g, '\\x3b')}`)}${osc('633;C')}`);
    switch (name) {
        case 'help':
            out.push('能用的命令：ls cd pwd echo clear sudo false demo uv exit');
            break;
        case 'pwd':
            out.push(s.cwd);
            break;
        case 'ls':
            out.push((FILES[s.cwd] ?? []).map((e) => (e.is_dir ? `${ESC}[34m${e.name}${ESC}[0m` : e.name)).join('  '));
            break;
        case 'cd': {
            const next = resolvePath(s.cwd, args[0] ?? '~');
            if (FILES[next]) s.cwd = next;
            else {
                out.push(`bash: cd: ${args[0]}: No such file or directory`);
                code = 1;
            }
            break;
        }
        case 'echo':
            out.push(args.join(' '));
            break;
        case 'clear':
            emit(s, `${ESC}[2J${ESC}[H`);
            break;
        case 'false':
            code = 1;
            break;
        case 'demo':
            out.push(...DEMO_LOG);
            break;
        case 'uv':
            out.push('Package      Version', '------------ -------', 'aiohttp      3.12.4', 'maim-message 0.3.9', 'openai       1.93.0');
            break;
        case 'sudo':
            s.sudoPending = args.join(' ');
            emit(s, `[sudo] password for napcat: `);
            return;
        case 'exit':
            emit(s, `${osc('633;D;0')}\r\n`);
            finish(s, 0);
            return;
        default:
            out.push(`bash: ${name}: command not found`);
            code = 127;
    }
    if (out.length) emit(s, `${out.join('\r\n')}\r\n`);
    emit(s, osc(`633;D;${code}`));
    prompt(s);
}

function finish(s: MockSession, code: number) {
    s.info = { ...s.info, status: { kind: 'exited', code } };
    emit(s, `\r\n${ESC}[2m[已退出，退出码 ${code}，按回车重新打开]${ESC}[0m\r\n`);
    s.handlers?.event({ v: 1, kind: 'status', status: s.info.status });
}

function input(s: MockSession, data: string) {
    let text = data;
    if (text.includes(`${ESC}[200~`)) text = text.replace(`${ESC}[200~`, '').replace(`${ESC}[201~`, '');
    for (const ch of text) {
        if (s.sudoPending !== null) {
            if (ch === '\r') {
                const cmd = s.sudoPending;
                s.sudoPending = null;
                emit(s, `\r\n（模拟）已用 root 执行：${cmd}\r\n${osc('633;D;0')}`);
                prompt(s);
            }
            continue;
        }
        if (ch === '\r') {
            emit(s, '\r\n');
            const line = s.line;
            s.line = '';
            run(s, line);
        } else if (ch === '\x7f' || ch === '\b') {
            if (s.line) {
                s.line = s.line.slice(0, -1);
                emit(s, '\b \b');
            }
        } else if (ch === '\x03') {
            s.line = '';
            emit(s, `^C\r\n${osc('633;D;130')}`);
            prompt(s);
        } else if (ch >= ' ' && ch !== '\x7f') {
            s.line += ch;
            emit(s, ch);
        }
    }
}

function get(id: string): MockSession {
    const s = sessions.get(id);
    if (!s) throw new Error('这个终端已经关了');
    return s;
}

export const terminalMock = {
    async localShells(): Promise<LocalShellOption[]> {
        return [
            { kind: 'pwsh', label: 'PowerShell 7', path: 'C:\\Program Files\\PowerShell\\7\\pwsh.exe' },
            { kind: 'windows_powershell', label: 'Windows PowerShell', path: 'powershell.exe' },
            { kind: 'cmd', label: '命令提示符', path: 'cmd.exe' },
            { kind: 'git_bash', label: 'Git Bash', path: 'C:\\Program Files\\Git\\bin\\bash.exe' },
        ];
    },

    async list(): Promise<TerminalSessionInfo[]> {
        return [...sessions.values()].map((s) => s.info);
    },

    async open(request: TerminalOpenRequest): Promise<TerminalSessionInfo> {
        const id = `t${nextId++}`;
        const meta = titleFor(request.target);
        const isApp = request.target.kind === 'app_instance';
        const info: TerminalSessionInfo = {
            id,
            target: request.target,
            title: meta.title,
            host_id: meta.linux ? 'remote:vps1' : 'local',
            host_label: meta.host,
            host_os: meta.linux ? 'linux' : 'windows',
            cwd: meta.cwd,
            shell: meta.linux ? undefined : (request.shell ?? 'pwsh'),
            status: { kind: 'running' },
            created_at_ms: Date.now(),
            features: {
                files: true,
                stats: meta.linux,
                sudo_fill: meta.linux,
                shell_integration: true,
                external: !meta.linux,
            },
            snippets: isApp
                ? [
                      { label: '装了哪些包', command: 'uv pip list' },
                      { label: '装一个包', command: 'uv pip install ' },
                  ]
                : [{ label: '磁盘', command: 'df -h' }],
        };
        const s: MockSession = { info, cwd: meta.cwd, line: '', history: '', handlers: null, sudoPending: null };
        sessions.set(id, s);
        emit(s, `${ESC}[2m${meta.linux ? `目录 · ${meta.cwd}` : 'PowerShell 7.5.2'}${ESC}[0m\r\n`);
        if (isApp) emit(s, `${ESC}[2mpython 和 uv 已指向这个实例的 .venv；装包用 uv pip install${ESC}[0m\r\n`);
        emit(s, `${ESC}[2m（浏览器预览里的假终端，输入 help 看能用的命令）${ESC}[0m\r\n`);
        prompt(s);
        return info;
    },

    async attach(id: string, handlers: TerminalAttachHandlers): Promise<TerminalSessionInfo> {
        const s = get(id);
        s.handlers = handlers;
        if (s.history) handlers.output(encoder.encode(s.history));
        return s.info;
    },

    async write(id: string, data: string): Promise<void> {
        input(get(id), data);
    },

    async resize(_id: string, _cols: number, _rows: number): Promise<void> {},

    async restart(id: string): Promise<TerminalSessionInfo> {
        const s = get(id);
        s.info = { ...s.info, status: { kind: 'running' } };
        emit(s, `\r\n${ESC}[2m──────── 重新打开 ────────${ESC}[0m\r\n`);
        s.handlers?.event({ v: 1, kind: 'info', info: s.info });
        prompt(s);
        return s.info;
    },

    async close(id: string): Promise<void> {
        sessions.delete(id);
    },

    async fillSudo(id: string): Promise<void> {
        input(get(id), '\r');
    },

    async stats(_id: string): Promise<ServerStats> {
        const jitter = (base: number, spread: number) => base + Math.round((Math.random() - 0.5) * spread);
        return {
            cpu_percent: jitter(23, 20),
            cores: 2,
            mem_used: jitter(1_600, 200) * 1024 * 1024,
            mem_total: 3_800 * 1024 * 1024,
            swap_used: 120 * 1024 * 1024,
            swap_total: 1024 * 1024 * 1024,
            disk_used: 14 * 1024 ** 3,
            disk_total: 40 * 1024 ** 3,
            net_rx_per_sec: jitter(18_000, 12_000),
            net_tx_per_sec: jitter(6_000, 4_000),
            load1: 0.42,
            load5: 0.37,
            load15: 0.31,
            uptime_secs: 3 * 86400 + 5 * 3600,
        };
    },

    async listDir(_id: string, path: string): Promise<TerminalDirListing> {
        const clean = path.replace(/\/+$/, '') || '/';
        const entries = FILES[clean];
        if (!entries) throw new Error(`找不到 ${clean}`);
        const cut = clean.lastIndexOf('/');
        return { path: clean, parent: clean === '/' ? undefined : clean.slice(0, cut) || '/', entries };
    },

    async readText(_id: string, path: string): Promise<TerminalTextFile> {
        return { path, content: TEXTS[path] ?? '', crlf: false };
    },

    async writeText(_id: string, path: string, content: string): Promise<void> {
        TEXTS[path] = content;
    },

    async makeDir(_id: string, path: string): Promise<void> {
        const cut = path.lastIndexOf('/');
        const parent = path.slice(0, cut) || '/';
        if (!FILES[path]) dir(path, []);
        const name = path.slice(cut + 1);
        if (!(FILES[parent] ?? []).some((e) => e.name === name)) {
            FILES[parent] = [
                ...(FILES[parent] ?? []),
                { name, path, is_dir: true, is_symlink: false, size: 0, modified: Math.floor(Date.now() / 1000), mode: 'rwxr-xr-x' },
            ];
        }
    },

    async rename(_id: string, from: string, to: string): Promise<void> {
        const cut = from.lastIndexOf('/');
        const parent = from.slice(0, cut) || '/';
        FILES[parent] = (FILES[parent] ?? []).map((e) =>
            e.path === from ? { ...e, path: to, name: to.slice(to.lastIndexOf('/') + 1) } : e,
        );
    },

    async remove(_id: string, path: string): Promise<void> {
        const cut = path.lastIndexOf('/');
        const parent = path.slice(0, cut) || '/';
        FILES[parent] = (FILES[parent] ?? []).filter((e) => e.path !== path);
    },
};
