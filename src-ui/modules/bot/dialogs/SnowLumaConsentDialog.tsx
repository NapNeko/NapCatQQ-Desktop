import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from '../../../shared/ui';
import type { SnowLumaAgreementsPayload } from '../../../core/services/bot.service';

type MarkdownBlock =
    | { kind: 'heading'; level: 1 | 2 | 3 | 4; text: string }
    | { kind: 'paragraph'; text: string }
    | { kind: 'list_item'; text: string }
    | { kind: 'quote'; text: string };

interface SnowLumaConsentDialogProps {
    open: boolean;
    botId: string | null;
    payload: SnowLumaAgreementsPayload | null;
    submitting: boolean;
    onConfirm: () => void;
    onCancel: () => void;
}

export function SnowLumaConsentDialog({
    open,
    botId,
    payload,
    submitting,
    onConfirm,
    onCancel,
}: SnowLumaConsentDialogProps) {
    const [readComplete, setReadComplete] = useState(false);
    const scrollRef = useRef<HTMLDivElement | null>(null);
    const payloadKey = payload
        ? `${payload.version}:${payload.documents.map((doc) => doc.id).join('|')}`
        : 'empty';

    const updateReadComplete = useCallback(() => {
        const el = scrollRef.current;
        if (!el || !payload) {
            setReadComplete(false);
            return;
        }
        const threshold = 8;
        setReadComplete(el.scrollTop + el.clientHeight >= el.scrollHeight - threshold);
    }, [payload]);

    useEffect(() => {
        setReadComplete(false);
        const el = scrollRef.current;
        if (el) el.scrollTop = 0;
        if (!open || !payload) return undefined;
        const raf = window.requestAnimationFrame(updateReadComplete);
        return () => window.cancelAnimationFrame(raf);
    }, [open, payloadKey, updateReadComplete]);

    return (
        <Dialog
            open={open}
            onOpenChange={(nextOpen) => {
                if (!nextOpen && !submitting) {
                    setReadComplete(false);
                    onCancel();
                }
            }}
        >
            <DialogContent size="xl" dismissOnOutsideClick={false}>
                <DialogHeader>
                    <DialogTitle>SnowLuma 用户协议与隐私政策</DialogTitle>
                    <DialogDescription>
                        {botId ? `Bot ${botId} 启动前需要先确认 SnowLuma 协议。` : '启动前需要先确认 SnowLuma 协议。'}
                    </DialogDescription>
                </DialogHeader>

                <div
                    ref={scrollRef}
                    onScroll={updateReadComplete}
                    className="max-h-[58vh] space-y-4 overflow-y-auto rounded-md border border-border bg-surface/60 p-4"
                >
                    {payload?.documents.map((doc) => (
                        <section key={doc.id} className="space-y-3">
                            <div className="flex items-start justify-between gap-3 border-b border-border-subtle pb-2">
                                <h3 className="text-sm font-semibold text-text">{doc.title}</h3>
                                {doc.declared_version && (
                                    <span className="shrink-0 rounded-xs bg-muted px-2 py-0.5 text-2xs text-text-tertiary">
                                        {doc.declared_version}
                                    </span>
                                )}
                            </div>
                            <MarkdownText text={doc.text} />
                        </section>
                    ))}
                    {!payload && (
                        <p className="text-sm text-text-secondary">正在读取 SnowLuma 协议内容…</p>
                    )}
                </div>

                <div className="flex h-9 items-center justify-between rounded-sm border border-border-subtle bg-inset px-3 text-xs">
                    <span className="text-text-secondary">阅读状态</span>
                    <span className={readComplete ? 'font-medium text-success' : 'text-text-tertiary'}>
                        {readComplete ? '已读完协议内容' : '请阅读至文末'}
                    </span>
                </div>

                <DialogFooter>
                    <Button variant="ghost" size="sm" disabled={submitting} onClick={onCancel}>
                        取消
                    </Button>
                    <Button
                        variant="primary"
                        size="sm"
                        className="min-w-32"
                        disabled={!readComplete || submitting || !payload}
                        onClick={() => {
                            if (!readComplete || submitting || !payload) return;
                            onConfirm();
                        }}
                    >
                        {submitting
                            ? '提交中…'
                            : readComplete
                                ? '同意并继续启动'
                                : '阅读完整内容后继续'}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}

function MarkdownText({ text }: { text: string }) {
    const blocks = parseMarkdownBlocks(text);
    return (
        <div className="space-y-2 text-sm leading-6 text-text-secondary">
            {blocks.map((block, index) => {
                if (block.kind === 'heading') {
                    const className =
                        block.level === 1
                            ? 'mt-4 text-lg font-semibold text-text'
                            : block.level === 2
                            ? 'mt-4 text-base font-semibold text-text'
                            : block.level === 3
                                ? 'mt-3 text-sm font-semibold text-text'
                                : 'mt-2 text-sm font-medium text-text';
                    return (
                        <h4 key={index} className={className}>
                            {renderInlineMarkdown(block.text)}
                        </h4>
                    );
                }
                if (block.kind === 'list_item') {
                    return (
                        <div key={index} className="flex gap-2">
                            <span className="mt-[0.7em] h-1 w-1 shrink-0 rounded-full bg-text-tertiary" />
                            <p className="min-w-0 whitespace-pre-wrap break-words">
                                {renderInlineMarkdown(block.text)}
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
                            {renderInlineMarkdown(block.text)}
                        </blockquote>
                    );
                }
                return (
                    <p key={index} className="whitespace-pre-wrap break-words">
                        {renderInlineMarkdown(block.text)}
                    </p>
                );
            })}
        </div>
    );
}

function parseMarkdownBlocks(text: string): MarkdownBlock[] {
    const blocks: MarkdownBlock[] = [];
    const paragraphs: string[] = [];
    const flushParagraph = () => {
        if (paragraphs.length === 0) return;
        const joined = paragraphs.join(' ').trim();
        if (joined) blocks.push({ kind: 'paragraph', text: joined });
        paragraphs.length = 0;
    };

    for (const rawLine of text.split(/\r?\n/)) {
        const line = rawLine.trim();
        if (!line || line === '---') {
            flushParagraph();
            continue;
        }
        const heading = /^(#{1,4})\s+(.+)$/.exec(line);
        if (heading) {
            flushParagraph();
            blocks.push({
                kind: 'heading',
                level: heading[1].length as 1 | 2 | 3 | 4,
                text: heading[2].trim(),
            });
            continue;
        }
        const quote = /^>\s*(.+)$/.exec(line);
        if (quote) {
            flushParagraph();
            blocks.push({ kind: 'quote', text: quote[1].trim() });
            continue;
        }
        const listItem = /^[-*]\s+(.+)$/.exec(line);
        if (listItem) {
            flushParagraph();
            blocks.push({ kind: 'list_item', text: listItem[1].trim() });
            continue;
        }
        if (/^\d+(?:\.\d+)+\s+/.test(line)) {
            flushParagraph();
            blocks.push({ kind: 'paragraph', text: line });
            continue;
        }
        paragraphs.push(line);
    }
    flushParagraph();
    return blocks;
}

function renderInlineMarkdown(text: string) {
    const parts: ReactNode[] = [];
    const pattern = /(\*\*[^*]+\*\*|`[^`]+`)/g;
    let lastIndex = 0;
    let match: RegExpExecArray | null;
    while ((match = pattern.exec(text)) !== null) {
        if (match.index > lastIndex) {
            parts.push(text.slice(lastIndex, match.index));
        }
        const token = match[0];
        if (token.startsWith('**')) {
            parts.push(
                <strong key={parts.length} className="font-semibold text-text">
                    {token.slice(2, -2)}
                </strong>,
            );
        } else {
            parts.push(
                <code key={parts.length} className="rounded-xs bg-muted px-1 py-0.5 font-mono text-[0.85em] text-text">
                    {token.slice(1, -1)}
                </code>,
            );
        }
        lastIndex = pattern.lastIndex;
    }
    if (lastIndex < text.length) {
        parts.push(text.slice(lastIndex));
    }
    return parts;
}
