// Release notes / 协议正文用的轻量 Markdown 解析。
// 不引入 marked：只覆盖 GitHub release body 常见子集。
//
// 针对 NC / NCD / ncd-watch 实测踩坑：
// 1. NCD 会夹 `<!-- BEGIN AUTO RELEASE NOTES -->` 一类 HTML 注释，必须剥掉
// 2. NC 大量 `**[label](url)**`：若先匹配粗体，链接会被整段吞成纯文本
// 3. NC 用 `1. item` 有序列表；不能和版本号行 `1.2.3 foo` 混淆
// 4. 连续链接行不要被拼成一段后丢换行语义（每行独立块更稳）
// 5. ncd-watch Assets 用 GFM 表格（| col | + |---| 分隔行）

export type MarkdownBlock =
    | { kind: 'heading'; level: 1 | 2 | 3 | 4; text: string }
    | { kind: 'paragraph'; text: string }
    | { kind: 'list_item'; text: string; ordered?: boolean; index?: number }
    | { kind: 'quote'; text: string }
    | { kind: 'table'; headers: string[]; rows: string[][] };

export type InlineToken =
    | { kind: 'text'; text: string }
    | { kind: 'bold'; text: string }
    | { kind: 'code'; text: string }
    | { kind: 'link'; label: string; href: string };

/** 剥 HTML 注释、统一换行，供后续块解析。 */
export function preprocessReleaseNotesMarkdown(raw: string): string {
    return raw
        .replace(/\r\n/g, '\n')
        .replace(/\r/g, '\n')
        // 多行 HTML 注释（NCD AUTO RELEASE NOTES 标记）
        .replace(/<!--[\s\S]*?-->/g, '')
        // 残留的空注释壳
        .replace(/<!---->/g, '')
        .trim();
}

export function parseMarkdownBlocks(raw: string): MarkdownBlock[] {
    const text = preprocessReleaseNotesMarkdown(raw);
    if (!text) return [];

    const lines = text.split('\n');
    const blocks: MarkdownBlock[] = [];
    const paragraphs: string[] = [];

    const flushParagraph = () => {
        if (paragraphs.length === 0) return;
        // 段内保留软换行：链接行 / 短句并排时比硬拼空格更接近 GitHub 渲染
        const joined = paragraphs.join('\n').trim();
        if (joined) blocks.push({ kind: 'paragraph', text: joined });
        paragraphs.length = 0;
    };

    let i = 0;
    while (i < lines.length) {
        const line = lines[i].trim();

        if (!line || line === '---' || /^(-{3,}|\*{3,}|_{3,})$/.test(line)) {
            flushParagraph();
            i += 1;
            continue;
        }

        // GFM 表格：header + |---| 分隔 + 0..n 数据行（ncd-watch Assets）
        if (isTableRowLine(line) && i + 1 < lines.length) {
            const next = lines[i + 1].trim();
            if (isTableSeparatorLine(next)) {
                flushParagraph();
                const headers = parseTableCells(line);
                const rows: string[][] = [];
                i += 2;
                while (i < lines.length) {
                    const body = lines[i].trim();
                    if (!body || !isTableRowLine(body) || isTableSeparatorLine(body)) break;
                    rows.push(parseTableCells(body));
                    i += 1;
                }
                blocks.push({ kind: 'table', headers, rows });
                continue;
            }
        }

        const heading = /^(#{1,4})\s+(.+)$/.exec(line);
        if (heading) {
            flushParagraph();
            blocks.push({
                kind: 'heading',
                level: heading[1].length as 1 | 2 | 3 | 4,
                text: heading[2].trim(),
            });
            i += 1;
            continue;
        }

        const quote = /^>\s?(.*)$/.exec(line);
        if (quote) {
            flushParagraph();
            blocks.push({ kind: 'quote', text: quote[1].trim() });
            i += 1;
            continue;
        }

        // 无序：- item / * item（避免 **bold** 被当成列表）
        const unordered = /^[-*+]\s+(.+)$/.exec(line);
        if (unordered && !line.startsWith('**')) {
            flushParagraph();
            blocks.push({ kind: 'list_item', text: unordered[1].trim() });
            i += 1;
            continue;
        }

        // 有序：1. item（排除 1.2.3 版本号起头）
        const ordered = /^(\d+)\.\s+(.+)$/.exec(line);
        if (ordered && !/^\d+\.\d+/.test(line)) {
            flushParagraph();
            blocks.push({
                kind: 'list_item',
                text: ordered[2].trim(),
                ordered: true,
                index: Number.parseInt(ordered[1], 10),
            });
            i += 1;
            continue;
        }

        // 单独一行的链接（含 **[label](url)**）直接成段，避免和前后句糊在一起
        if (isStandaloneLinkLine(line)) {
            flushParagraph();
            blocks.push({ kind: 'paragraph', text: line });
            i += 1;
            continue;
        }

        paragraphs.push(line);
        i += 1;
    }
    flushParagraph();
    return blocks;
}

/** 粗判表格行：含 |，且不是纯分隔线（分隔线另判）。 */
export function isTableRowLine(line: string): boolean {
    const t = line.trim();
    if (!t.includes('|')) return false;
    // 至少两列：`a|b` 或 `|a|b|`
    const cells = parseTableCells(t);
    return cells.length >= 2;
}

/** GFM 分隔行：| --- | :---: | ---: | */
export function isTableSeparatorLine(line: string): boolean {
    const t = line.trim();
    if (!t.includes('|') && !t.includes('-')) return false;
    const cells = parseTableCells(t);
    if (cells.length === 0) return false;
    return cells.every((cell) => /^:?-{3,}:?$/.test(cell.trim()));
}

/** 拆 `| a | b |` → `['a','b']`；也接受 `a | b`。 */
export function parseTableCells(line: string): string[] {
    let t = line.trim();
    if (t.startsWith('|')) t = t.slice(1);
    if (t.endsWith('|')) t = t.slice(0, -1);
    return t.split('|').map((c) => c.trim());
}

function isStandaloneLinkLine(line: string): boolean {
    // **[label](url)** 或 [label](url) 或纯 URL
    if (/^\*{0,2}\[[^\]]+\]\([^)]+\)\*{0,2}$/.test(line)) return true;
    if (/^https?:\/\/\S+$/i.test(line)) return true;
    return false;
}

/**
 * 行内 token。顺序：code → link → bold → text。
 * link 必须先于 bold，否则 `**[x](url)**` 会被粗体整段吞掉。
 */
export function tokenizeInlineMarkdown(text: string): InlineToken[] {
    const tokens: InlineToken[] = [];
    // code | link | bold-with-optional-inner | bare-url
    const pattern =
        /(`[^`]+`)|(\[[^\]]+\]\([^)\s]+\))|(\*\*[^*]+?\*\*)|(https?:\/\/[^\s)<]+)/g;
    let lastIndex = 0;
    let match: RegExpExecArray | null;

    while ((match = pattern.exec(text)) !== null) {
        if (match.index > lastIndex) {
            tokens.push({ kind: 'text', text: text.slice(lastIndex, match.index) });
        }
        const full = match[0];
        if (match[1]) {
            tokens.push({ kind: 'code', text: full.slice(1, -1) });
        } else if (match[2]) {
            const link = /^\[([^\]]+)\]\(([^)\s]+)\)$/.exec(full);
            if (link) {
                tokens.push({ kind: 'link', label: link[1], href: link[2] });
            } else {
                tokens.push({ kind: 'text', text: full });
            }
        } else if (match[3]) {
            const inner = full.slice(2, -2);
            // 粗体内若整段是链接，拆成 bold 包一层 link 语义：这里直接吐 link 更清晰
            const onlyLink = /^\[([^\]]+)\]\(([^)\s]+)\)$/.exec(inner);
            if (onlyLink) {
                tokens.push({ kind: 'link', label: onlyLink[1], href: onlyLink[2] });
            } else {
                tokens.push({ kind: 'bold', text: inner });
            }
        } else if (match[4]) {
            const href = full.replace(/[.,;:!?]+$/, '');
            tokens.push({ kind: 'link', label: href, href });
            // 被剥掉的尾标点回吐
            if (href.length < full.length) {
                tokens.push({ kind: 'text', text: full.slice(href.length) });
            }
        } else {
            tokens.push({ kind: 'text', text: full });
        }
        lastIndex = pattern.lastIndex;
    }

    if (lastIndex < text.length) {
        tokens.push({ kind: 'text', text: text.slice(lastIndex) });
    }
    return tokens;
}
