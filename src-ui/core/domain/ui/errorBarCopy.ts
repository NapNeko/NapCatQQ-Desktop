// InfoBar danger：短句 + 指引看日志；原文打 console / 桌面日志。

export const SEE_LOGS_HINT = '详情见日志';

const PREFIXES = [
    /^应用端运行失败:\s*/g,
    /^写入应用端配置失败:\s*/g,
    /^拉取(?:适配器|插件)?目录失败:\s*/g,
    /^读取(?:适配器|插件)?目录失败:\s*/g,
];

export function briefError(raw: string, maxLen = 72): string {
    let text = raw.trim();
    for (const re of PREFIXES) {
        text = text.replace(re, '');
    }
    text = text.trim();
    const lines = text.split(/\r?\n/).map((l) => l.trim()).filter(Boolean);
    let summary = lines[0] ?? text;
    for (let i = lines.length - 1; i >= 0; i--) {
        if (/^[A-Z]\w*(Error|Exception|Failure):/i.test(lines[i]) || lines[i].startsWith('OSError:')) {
            summary = lines[i];
            break;
        }
    }
    if (summary.length <= maxLen) return summary;
    return `${summary.slice(0, maxLen - 1)}…`;
}

export function errorBarContent(raw?: string | null): string {
    const t = raw?.trim();
    if (!t) return SEE_LOGS_HINT;
    const brief = briefError(t);
    if (!brief) return SEE_LOGS_HINT;
    if (brief.includes('详情见')) return brief;
    return `${brief}。${SEE_LOGS_HINT}`;
}

/** @deprecated 用 briefError */
export const briefAppError = briefError;
/** @deprecated 用 errorBarContent */
export const appErrorBarContent = errorBarContent;
