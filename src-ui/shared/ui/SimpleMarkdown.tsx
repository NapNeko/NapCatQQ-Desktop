// 轻量 Markdown 展示：标题 / 列表 / 引用 / 表格 / 粗体 / 行内代码 / 链接。
// 解析逻辑在 core/domain/release/release-notes-markdown.ts（可单测）。
// 给协议正文、Release notes 等「人写的短 Markdown」用，不引入 marked/rehype。

import type { ReactNode } from 'react';
import {
    parseMarkdownBlocks,
    tokenizeInlineMarkdown,
    type InlineToken,
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
}

export function SimpleMarkdown({
    text,
    className,
    emptyFallback = '暂无内容',
    onOpenLink,
}: SimpleMarkdownProps) {
    const blocks = parseMarkdownBlocks(text);
    if (blocks.length === 0) {
        return (
            <p className={cn('text-sm text-text-tertiary', className)}>{emptyFallback}</p>
        );
    }

    return (
        <div className={cn('space-y-2 text-sm leading-6 text-text-secondary', className)}>
            {blocks.map((block, index) => {
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
                            {renderInlineTokens(
                                tokenizeInlineMarkdown(block.text),
                                onOpenLink,
                            )}
                        </h4>
                    );
                }
                if (block.kind === 'list_item') {
                    const marker = block.ordered
                        ? `${block.index ?? 1}.`
                        : null;
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
                                {renderInlineTokens(
                                    tokenizeInlineMarkdown(block.text),
                                    onOpenLink,
                                )}
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
                            {renderInlineTokens(
                                tokenizeInlineMarkdown(block.text),
                                onOpenLink,
                            )}
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
                                                {renderInlineTokens(
                                                    tokenizeInlineMarkdown(cell),
                                                    onOpenLink,
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
                                                    {renderInlineTokens(
                                                        tokenizeInlineMarkdown(cell),
                                                        onOpenLink,
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
                        {renderInlineTokens(
                            tokenizeInlineMarkdown(block.text),
                            onOpenLink,
                        )}
                    </p>
                );
            })}
        </div>
    );
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
