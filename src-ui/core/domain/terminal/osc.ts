// 终端里 shell 发出的 OSC 标记：提示符、命令起止、退出码、当前目录、进度。
//
// 633 是 VS Code 的协议，桌面端给 bash / PowerShell 挂的脚本发这个；133 是 FinalTerm 的老协议，
// starship、oh-my-posh 这类自己会发；7 和 9;9 只报目录（zsh / fish 常见 7，cmd 和
// Windows Terminal 集成的是 9;9）；9;4 是 ConEmu 的进度（winget 之类会发）。

export type ShellMark =
    | { kind: 'prompt_start' }
    | { kind: 'prompt_end' }
    | { kind: 'command_line'; command: string }
    | { kind: 'command_start' }
    | { kind: 'command_end'; exitCode: number | null }
    | { kind: 'cwd'; path: string }
    | { kind: 'progress'; state: ProgressState; value: number };

/** 0 清掉；1 普通；2 出错；3 不确定；4 暂停 / 警告 */
export type ProgressState = 0 | 1 | 2 | 3 | 4;

/** 633 里的值：`\\` 是反斜杠，`\xNN` 是一个字节（桌面端只转义 ASCII） */
export function deserializeOscValue(raw: string): string {
    return raw.replace(/\\(\\|x([0-9a-fA-F]{2}))/g, (_m, op: string, hex?: string) =>
        hex ? String.fromCharCode(Number.parseInt(hex, 16)) : op,
    );
}

function exitCodeOf(raw: string): number | null {
    const trimmed = raw.split(';')[0]?.trim() ?? '';
    if (trimmed === '') return null;
    const n = Number.parseInt(trimmed, 10);
    return Number.isFinite(n) ? n : null;
}

/** OSC 633 的正文（不含 `633;`） */
export function parseOsc633(data: string): ShellMark | null {
    const semi = data.indexOf(';');
    const code = semi === -1 ? data : data.slice(0, semi);
    const rest = semi === -1 ? '' : data.slice(semi + 1);
    switch (code) {
        case 'A':
            return { kind: 'prompt_start' };
        case 'B':
            return { kind: 'prompt_end' };
        case 'C':
            return { kind: 'command_start' };
        case 'D':
            return { kind: 'command_end', exitCode: exitCodeOf(rest) };
        case 'E': {
            // E;<命令>[;<nonce>]：命令里的分号已转义成 \x3b，第一个分号前就是整条命令
            const command = rest.split(';')[0] ?? '';
            return { kind: 'command_line', command: deserializeOscValue(command) };
        }
        case 'P': {
            const eq = rest.indexOf('=');
            if (eq === -1) return null;
            if (rest.slice(0, eq) !== 'Cwd') return null;
            const path = deserializeOscValue(rest.slice(eq + 1));
            return path ? { kind: 'cwd', path } : null;
        }
        default:
            return null;
    }
}

/** OSC 133 的正文：A 提示符、B 开始输入、C 开始执行、D;<退出码> 结束 */
export function parseOsc133(data: string): ShellMark | null {
    const code = data[0];
    switch (code) {
        case 'A':
            return { kind: 'prompt_start' };
        case 'B':
            return { kind: 'prompt_end' };
        case 'C':
            return { kind: 'command_start' };
        case 'D':
            return { kind: 'command_end', exitCode: exitCodeOf(data.slice(2)) };
        default:
            return null;
    }
}

/** OSC 7：`file://主机/路径`，路径按 URL 编码 */
export function parseOsc7(data: string): ShellMark | null {
    if (!data.startsWith('file://')) return null;
    const afterScheme = data.slice('file://'.length);
    const slash = afterScheme.indexOf('/');
    if (slash === -1) return null;
    let path = afterScheme.slice(slash);
    try {
        path = decodeURIComponent(path);
    } catch {
        // 编码坏了就原样用
    }
    // Windows 上的 file:///C:/x 去掉前导斜杠
    if (/^\/[A-Za-z]:\//.test(path)) path = path.slice(1).replace(/\//g, '\\');
    return path ? { kind: 'cwd', path } : null;
}

/** OSC 9 的正文：`9;<目录>`（Windows Terminal），`4;<状态>;<进度>`（ConEmu 进度） */
export function parseOsc9(data: string): ShellMark | null {
    if (data.startsWith('9;')) {
        let path = data.slice(2).trim();
        if (path.length >= 2 && path.startsWith('"') && path.endsWith('"')) path = path.slice(1, -1);
        return path ? { kind: 'cwd', path } : null;
    }
    if (data.startsWith('4;')) {
        const [stateRaw, valueRaw] = data.slice(2).split(';');
        const state = Number.parseInt(stateRaw ?? '', 10);
        if (!Number.isInteger(state) || state < 0 || state > 4) return null;
        const value = Math.max(0, Math.min(100, Number.parseInt(valueRaw ?? '0', 10) || 0));
        return { kind: 'progress', state: state as ProgressState, value };
    }
    return null;
}
