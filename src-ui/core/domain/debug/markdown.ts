// 协议 Markdown 的正文读取与纯文本摘要，共用现有安全解析器。
import { parseMarkdownBlocks, tokenizeInlineMarkdown } from '../release/release-notes-markdown';

const isRecord = (value: unknown): value is Record<string, unknown> =>
    !!value && typeof value === 'object' && !Array.isArray(value);
const CONTENT_FIELDS = ['content', 'markdown', 'data', 'text'] as const;

export function markdownContent(data: Record<string, unknown>): string {
    const read = (value: unknown, depth: number): string => {
        if (depth > 4) return '';
        if (typeof value === 'string') {
            const trimmed = value.trim();
            if (!trimmed) return '';
            if (trimmed.startsWith('{') || trimmed.startsWith('"')) {
                try {
                    const parsed: unknown = JSON.parse(trimmed);
                    if (
                        typeof parsed === 'string' ||
                        (isRecord(parsed) && CONTENT_FIELDS.some((key) => key in parsed))
                    )
                        return read(parsed, depth + 1);
                } catch {
                    // 正文也可以以花括号或引号开头，无法解析时按原文展示。
                }
            }
            return value;
        }
        if (!isRecord(value)) return '';
        for (const key of CONTENT_FIELDS) {
            const content = read(value[key], depth + 1);
            if (content) return content;
        }
        return '';
    };
    return stripQQMarkdownMetadata(read(data, 0));
}

export function stripQQMarkdownMetadata(content: string): string {
    const marker = /^\s*\[\]\(([^)\r\n]+)\)[ \t]*(?:\r?\n)?/.exec(content);
    if (!marker) return content;
    try {
        const metadata: unknown = JSON.parse(decodeURIComponent(marker[1]));
        // QQ 在正文开头用空链接携带渲染版本；只识别这个封套，避免误删普通链接或代码。
        if (isRecord(metadata) && metadata.version === 2) return content.slice(marker[0].length);
    } catch {
        // 无效编码或非 JSON 的空链接仍属于原始正文。
    }
    return content;
}

export type MarkdownButtonAction =
    | { kind: 'link'; url: string }
    | { kind: 'command'; text: string; enter: boolean; reply: boolean }
    | { kind: 'callback'; data: string }
    | { kind: 'unsupported'; reason: string };

export interface MarkdownKeyboardButton {
    id: string;
    label: string;
    visitedLabel: string;
    primary: boolean;
    permissionType: number | null;
    permittedUsers: string[];
    action: MarkdownButtonAction;
}

export interface MarkdownKeyboard {
    botAppid?: string;
    rows: MarkdownKeyboardButton[][];
}

const stringOf = (value: unknown): string =>
    typeof value === 'string'
        ? value
        : typeof value === 'number' && Number.isSafeInteger(value)
          ? String(value)
          : '';
const numberOf = (value: unknown): number | null => {
    if (typeof value !== 'number' && typeof value !== 'string') return null;
    if (typeof value === 'string' && !value.trim()) return null;
    const parsed = Number(value);
    return Number.isSafeInteger(parsed) ? parsed : null;
};
const booleanOf = (value: unknown): boolean =>
    value === true || value === 'true' || value === 1 || value === '1';

export function markdownHttpUrl(value: string): string | null {
    try {
        const url = new URL(value);
        return url.protocol === 'http:' || url.protocol === 'https:' ? url.href : null;
    } catch {
        return null;
    }
}

function keyboardAction(data: Record<string, unknown>): MarkdownButtonAction {
    const action = isRecord(data.action) ? data.action : data;
    const text = stringOf(action.data);
    switch (numberOf(action.type)) {
        case 0: {
            const url = markdownHttpUrl(text);
            return url
                ? { kind: 'link', url }
                : { kind: 'unsupported', reason: '此按钮需要 QQ 客户端打开链接' };
        }
        case 1:
            return { kind: 'callback', data: text };
        case 2:
            if (numberOf(action.anchor) === 1)
                return { kind: 'unsupported', reason: '此按钮需要手机 QQ 选择图片' };
            if (booleanOf(action.at_bot_show_channel_list ?? action.atBotShowChannelList))
                return { kind: 'unsupported', reason: '此按钮需要 QQ 子频道选择器' };
            return text.trim()
                ? {
                      kind: 'command',
                      text,
                      enter: booleanOf(action.enter),
                      reply: booleanOf(action.reply ?? action.is_reply ?? action.isReply),
                  }
                : { kind: 'unsupported', reason: '按钮没有提供指令内容' };
        default:
            return { kind: 'unsupported', reason: '此按钮需要 QQ 客户端处理' };
    }
}

export function markdownKeyboard(data: Record<string, unknown>): MarkdownKeyboard | null {
    const find = (value: unknown, depth: number): Record<string, unknown> | null => {
        if (depth > 4) return null;
        if (typeof value === 'string') {
            try {
                return find(JSON.parse(value), depth + 1);
            } catch {
                return null;
            }
        }
        if (!isRecord(value)) return null;
        if (Array.isArray(value.rows)) return value;
        for (const field of ['keyboard', 'content', 'data']) {
            const found = find(value[field], depth + 1);
            if (found) return found;
        }
        return null;
    };
    const keyboard = find(data, 0);
    if (!keyboard || !Array.isArray(keyboard.rows)) return null;
    const rows = keyboard.rows.slice(0, 5).flatMap((row): MarkdownKeyboardButton[][] => {
        if (!isRecord(row) || !Array.isArray(row.buttons)) return [];
        const buttons = row.buttons.slice(0, 5).flatMap((button): MarkdownKeyboardButton[] => {
            if (!isRecord(button)) return [];
            const render = isRecord(button.render_data) ? button.render_data : button;
            const label = stringOf(render.label);
            if (!label.trim()) return [];
            const action = isRecord(button.action) ? button.action : button;
            const permission = isRecord(action.permission) ? action.permission : action;
            const users = permission.specify_user_ids ?? permission.specifyUserIds;
            return [
                {
                    id: stringOf(button.id),
                    label,
                    visitedLabel: stringOf(render.visited_label ?? render.visitedLabel),
                    primary: numberOf(render.style) === 1,
                    permissionType: numberOf(
                        permission.permission_type ??
                            permission.permissionType ??
                            (isRecord(action.permission) ? permission.type : undefined),
                    ),
                    permittedUsers: Array.isArray(users) ? users.map(stringOf).filter(Boolean) : [],
                    action: keyboardAction(button),
                },
            ];
        });
        return buttons.length ? [buttons] : [];
    });
    if (!rows.length) return null;
    return {
        botAppid:
            stringOf(keyboard.bot_appid ?? keyboard.botAppid ?? data.bot_appid ?? data.botAppid) ||
            undefined,
        rows,
    };
}

export function markdownPlainText(content: string): string {
    const inline = (text: string) =>
        tokenizeInlineMarkdown(text)
            .map((token) => (token.kind === 'link' ? token.label : token.text))
            .join('');
    return parseMarkdownBlocks(stripQQMarkdownMetadata(content))
        .map((block) =>
            block.kind === 'table'
                ? [...block.headers, ...block.rows.flat()].map(inline).join(' ')
                : inline(block.text),
        )
        .join(' ')
        .replace(/\s+/g, ' ')
        .trim();
}
