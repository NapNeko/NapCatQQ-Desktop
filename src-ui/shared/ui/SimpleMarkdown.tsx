// 安全展示 Markdown 正文，链接与图片交由宿主打开。

import type { ReactNode } from 'react';
import {
    parseMarkdownBlocks,
    tokenizeInlineMarkdown,
    type InlineToken,
    type MarkdownBlock,
} from '../../core/domain/release/release-notes-markdown';
import { cn } from '../utils/cn';

export interface SimpleMarkdownProps {
    text: string;
    className?: string;
    /** 空正文时的占位文案 */
    emptyFallback?: string;
    /**
     * 行内链接点击回调。Tauri webview 不能靠 target=_blank 开外链，
     * 调用方应走 openExternalUrl / useOpenExternal。
     */
    onOpenLink?: (url: string) => void;
    onOpenImage?: (url: string) => void;
}

export function SimpleMarkdown({
    text,
    className,
    emptyFallback = '暂无内容',
    onOpenLink,
    onOpenImage,
}: SimpleMarkdownProps) {
    const blocks = parseDisplayBlocks(text);
    if (blocks.length === 0) {
        return <p className={cn('text-sm text-text-tertiary', className)}>{emptyFallback}</p>;
    }

    return (
        <div className={cn('space-y-2 text-sm leading-6 text-text-secondary', className)}>
            {blocks.map((block, index) => {
                if (block.kind === 'code_block') {
                    return (
                        <pre
                            key={index}
                            className="max-w-full overflow-x-auto rounded-sm bg-inset p-3 font-mono text-xs leading-5 text-text"
                        >
                            <code data-language={block.language || undefined}>{block.text}</code>
                        </pre>
                    );
                }
                if (block.kind === 'heading') {
                    const classNameByLevel =
                        block.level === 1
                            ? 'mt-4 text-lg font-semibold text-text first:mt-0'
                            : block.level === 2
                              ? 'mt-4 text-base font-semibold text-text first:mt-0'
                              : block.level === 3
                                ? 'mt-3 text-sm font-semibold text-text first:mt-0'
                                : 'mt-2 text-sm font-medium text-text first:mt-0';
                    return (
                        <h4 key={index} className={classNameByLevel}>
                            {renderInlineMarkdown(block.text, onOpenLink, onOpenImage)}
                        </h4>
                    );
                }
                if (block.kind === 'list_item') {
                    const marker = block.ordered ? `${block.index ?? 1}.` : null;
                    return (
                        <div key={index} className="flex gap-2">
                            {marker ? (
                                <span className="mt-0.5 w-5 shrink-0 text-right font-mono text-2xs tabular-nums text-text-tertiary">
                                    {marker}
                                </span>
                            ) : (
                                <span className="mt-[0.7em] h-1 w-1 shrink-0 rounded-full bg-text-tertiary" />
                            )}
                            <p className="min-w-0 whitespace-pre-wrap break-words">
                                {renderInlineMarkdown(block.text, onOpenLink, onOpenImage)}
                            </p>
                        </div>
                    );
                }
                if (block.kind === 'quote') {
                    return (
                        <blockquote
                            key={index}
                            className="border-l-2 border-border pl-3 text-text-tertiary"
                        >
                            {renderInlineMarkdown(block.text, onOpenLink, onOpenImage)}
                        </blockquote>
                    );
                }
                if (block.kind === 'table') {
                    return (
                        <div
                            key={index}
                            className="mt-2 overflow-x-auto rounded-md border border-border-subtle first:mt-0"
                        >
                            <table className="w-full min-w-[16rem] border-collapse text-left text-xs">
                                <thead className="bg-inset/60">
                                    <tr>
                                        {block.headers.map((cell, ci) => (
                                            <th
                                                key={ci}
                                                className="border-b border-border-subtle px-2.5 py-1.5 font-medium text-text"
                                            >
                                                {renderInlineMarkdown(
                                                    cell,
                                                    onOpenLink,
                                                    onOpenImage,
                                                )}
                                            </th>
                                        ))}
                                    </tr>
                                </thead>
                                <tbody>
                                    {block.rows.map((row, ri) => (
                                        <tr
                                            key={ri}
                                            className="border-b border-border-subtle last:border-b-0"
                                        >
                                            {row.map((cell, ci) => (
                                                <td
                                                    key={ci}
                                                    className="px-2.5 py-1.5 align-top font-mono text-[0.8rem] text-text-secondary"
                                                >
                                                    {renderInlineMarkdown(
                                                        cell,
                                                        onOpenLink,
                                                        onOpenImage,
                                                    )}
                                                </td>
                                            ))}
                                        </tr>
                                    ))}
                                </tbody>
                            </table>
                        </div>
                    );
                }
                return (
                    <p key={index} className="whitespace-pre-wrap break-words">
                        {renderInlineMarkdown(block.text, onOpenLink, onOpenImage)}
                    </p>
                );
            })}
        </div>
    );
}

type DisplayBlock = MarkdownBlock | { kind: 'code_block'; text: string; language: string };

function parseDisplayBlocks(text: string): DisplayBlock[] {
    const lines = text.replace(/\r\n?/g, '\n').split('\n');
    const blocks: DisplayBlock[] = [];
    let markdown: string[] = [];
    const flush = () => {
        blocks.push(...parseMarkdownBlocks(markdown.join('\n')));
        markdown = [];
    };
    for (let index = 0; index < lines.length; index++) {
        const fence = /^ {0,3}(`{3,}|~{3,})([^`~]*)$/.exec(lines[index]);
        if (!fence) {
            markdown.push(lines[index]);
            continue;
        }
        flush();
        const body: string[] = [];
        const closing = new RegExp(`^ {0,3}${fence[1][0]}{${fence[1].length},}\\s*$`);
        while (++index < lines.length && !closing.test(lines[index])) body.push(lines[index]);
        blocks.push({ kind: 'code_block', text: body.join('\n'), language: fence[2].trim() });
    }
    flush();
    return blocks;
}

function renderInlineMarkdown(
    text: string,
    onOpenLink?: (url: string) => void,
    onOpenImage?: (url: string) => void,
): ReactNode[] {
    // 行内代码先隔开，里面的图片语法必须原样保留。
    const runs = text.split(/(`[^`]*`)/g);
    return runs.flatMap((run, runIndex) => {
        if (run.startsWith('`') && run.endsWith('`')) {
            return [
                <code
                    key={`code-${runIndex}`}
                    className="rounded-xs bg-muted px-1 py-0.5 font-mono text-[0.85em] text-text"
                >
                    {run.slice(1, -1)}
                </code>,
            ];
        }
        const parts: ReactNode[] = [];
        const images = /!\[([^\]\n]*)\]\((https?:\/\/[^\s)]+)(?:\s+"[^"]*")?\)/gi;
        let offset = 0;
        for (let match = images.exec(run); match; match = images.exec(run)) {
            parts.push(
                <span key={`text-${runIndex}-${offset}`}>
                    {renderInlineTokens(
                        tokenizeInlineMarkdown(run.slice(offset, match.index)),
                        onOpenLink,
                    )}
                </span>,
            );
            const source = match[2];
            const label = match[1].replace(/\s+#\d+px/g, '').trim() || '图片';
            const image = (
                <img
                    src={source}
                    alt={label}
                    loading="lazy"
                    decoding="async"
                    className="inline-block max-h-80 max-w-full rounded-sm object-contain align-middle"
                />
            );
            parts.push(
                onOpenImage ? (
                    <button
                        key={`image-${runIndex}-${match.index}`}
                        type="button"
                        onClick={() => onOpenImage(source)}
                        aria-label={`查看图片：${label}`}
                        className="inline-block max-w-full rounded-sm align-middle focus-visible:outline-2 focus-visible:outline-brand"
                    >
                        {image}
                    </button>
                ) : (
                    <span key={`image-${runIndex}-${match.index}`}>{image}</span>
                ),
            );
            offset = match.index + match[0].length;
        }
        parts.push(
            <span key={`text-${runIndex}-${offset}`}>
                {renderInlineTokens(tokenizeInlineMarkdown(run.slice(offset)), onOpenLink)}
            </span>,
        );
        return parts;
    });
}

function renderInlineTokens(
    tokens: InlineToken[],
    onOpenLink?: (url: string) => void,
): ReactNode[] {
    return tokens.map((token, index) => {
        switch (token.kind) {
            case 'text':
                return <span key={index}>{token.text}</span>;
            case 'bold':
                return (
                    <strong key={index} className="font-semibold text-text">
                        {token.text}
                    </strong>
                );
            case 'code':
                return (
                    <code
                        key={index}
                        className="rounded-xs bg-muted px-1 py-0.5 font-mono text-[0.85em] text-text"
                    >
                        {token.text}
                    </code>
                );
            case 'link':
                if (onOpenLink) {
                    return (
                        <button
                            key={index}
                            type="button"
                            onClick={() => onOpenLink(token.href)}
                            className="inline text-left text-brand underline-offset-2 hover:underline"
                            title={token.href}
                        >
                            {token.label}
                        </button>
                    );
                }
                return (
                    <span key={index} className="text-brand" title={token.href}>
                        {token.label}
                    </span>
                );
            default:
                return null;
        }
    });
}
