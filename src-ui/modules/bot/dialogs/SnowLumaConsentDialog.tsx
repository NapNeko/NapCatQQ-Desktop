import { useState } from 'react';
import {
    Button,
    Checkbox,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from '../../../shared/ui';
import type { SnowLumaAgreementsPayload } from '../../../core/services/bot.service';

type MarkdownBlock =
    | { kind: 'heading'; level: 2 | 3 | 4; text: string }
    | { kind: 'paragraph'; text: string };

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
    const [agreed, setAgreed] = useState(false);

    return (
        <Dialog
            open={open}
            onOpenChange={(nextOpen) => {
                if (!nextOpen && !submitting) {
                    setAgreed(false);
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

                <div className="max-h-[58vh] space-y-3 overflow-y-auto rounded-md border border-border bg-surface/60 p-3">
                    {payload?.documents.map((doc) => (
                        <section key={doc.id} className="space-y-2">
                            <div className="flex items-center justify-between gap-3">
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

                <Checkbox
                    checked={agreed}
                    onCheckedChange={setAgreed}
                    disabled={submitting || !payload}
                    label="我已阅读并同意 SnowLuma 用户协议与隐私政策"
                />

                <DialogFooter>
                    <Button variant="ghost" size="sm" disabled={submitting} onClick={onCancel}>
                        取消
                    </Button>
                    <Button
                        variant="primary"
                        size="sm"
                        disabled={!agreed || submitting || !payload}
                        onClick={() => {
                            if (!agreed || submitting || !payload) return;
                            onConfirm();
                        }}
                    >
                        {submitting ? '提交中…' : '同意并继续启动'}
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
                        block.level === 2
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
        const heading = /^(#{2,4})\s+(.+)$/.exec(line);
        if (heading) {
            flushParagraph();
            blocks.push({
                kind: 'heading',
                level: heading[1].length as 2 | 3 | 4,
                text: heading[2].trim(),
            });
            continue;
        }
        paragraphs.push(line);
    }
    flushParagraph();
    return blocks;
}

function renderInlineMarkdown(text: string) {
    const parts: React.ReactNode[] = [];
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
