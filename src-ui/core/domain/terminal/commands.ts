// 最近命令：从命令标记里拿到的原文，按开终端的目标各记一份。
//
// 看起来带密码 / 密钥的不记（照 PSReadLine 不写历史文件的那套判断），空格开头的不记
// （bash 的 HISTCONTROL=ignorespace 也是这个约定）。

import type { TerminalTarget } from '../../ipc/generated/domain/TerminalTarget';

export const RECENT_LIMIT = 50;
const MAX_COMMAND_LENGTH = 500;

const SENSITIVE =
    /passw(?:or)?d|passwd|secret|token|api[-_]?key|access[-_]?key|private[-_]?key|credential|authorization|bearer\s|\bmysql\b.*\s-p\S|sshpass|--password/i;

export function shouldRemember(command: string): boolean {
    if (!command || /^\s/.test(command)) return false;
    const trimmed = command.trim();
    if (!trimmed || trimmed.length > MAX_COMMAND_LENGTH) return false;
    if (trimmed.includes('\n')) return false;
    return !SENSITIVE.test(trimmed);
}

/** 放到最前面；同一条只留一份 */
export function pushRecent(
    list: readonly string[],
    command: string,
    limit = RECENT_LIMIT,
): string[] {
    const trimmed = command.trim();
    const next = [trimmed, ...list.filter((c) => c !== trimmed)];
    return next.length > limit ? next.slice(0, limit) : next;
}

/** 同一个目标开出来的终端共用一份最近命令、一个标签 */
export function targetKey(target: TerminalTarget): string {
    switch (target.kind) {
        case 'local':
            return 'local';
        case 'server':
            return `server:${target.server_id}`;
        case 'bot':
            return `bot:${target.bot_id}${target.host_dir ? ':host' : ''}`;
        case 'app_instance':
            return `app:${target.instance_id}`;
    }
}
