import { useCallback, useEffect, useRef, useState } from 'react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    SimpleMarkdown,
} from '../../../shared/ui';
import type { SnowLumaAgreementsPayload } from '../../../core/services/bot.service';

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
                            <SimpleMarkdown text={doc.text} />
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
