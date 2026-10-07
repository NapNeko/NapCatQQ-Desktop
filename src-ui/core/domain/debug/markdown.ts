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
    return read(data, 0);
}

export function markdownPlainText(content: string): string {
    const inline = (text: string) =>
        tokenizeInlineMarkdown(text)
            .map((token) => (token.kind === 'link' ? token.label : token.text))
            .join('');
    return parseMarkdownBlocks(content)
        .map((block) =>
            block.kind === 'table'
                ? [...block.headers, ...block.rows.flat()].map(inline).join(' ')
                : inline(block.text),
        )
        .join(' ')
        .replace(/\s+/g, ' ')
        .trim();
}
