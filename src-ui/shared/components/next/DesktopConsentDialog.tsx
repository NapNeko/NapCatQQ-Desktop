import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
    Button,
    Checkbox,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    SimpleMarkdown,
} from '../../ui';
import type { DesktopAgreementsPayload } from '../../../core/services/desktop-consent.service';
import type { DesktopConsentMode } from '../../../hooks/desktop/useDesktopConsentGate';
import { cn } from '../../utils/cn';

interface DesktopConsentDialogProps {
    open: boolean;
    mode: DesktopConsentMode;
    payload: DesktopAgreementsPayload | null;
    submitting: boolean;
    onAccept: () => void;
    /** 只读关闭，或门禁下「不同意」 */
    onClose: () => void;
}

const SCROLL_THRESHOLD_PX = 8;

export function DesktopConsentDialog({
    open,
    mode,
    payload,
    submitting,
    onAccept,
    onClose,
}: DesktopConsentDialogProps) {
    const [activeId, setActiveId] = useState('eula');
    const [agreed, setAgreed] = useState(false);
    /** 各文档是否已滚到文末；门禁下须全部 true 才能勾选/继续 */
    const [readById, setReadById] = useState<Record<string, boolean>>({});
    const scrollRef = useRef<HTMLDivElement | null>(null);
    const docs = payload?.documents ?? [];
    const active = docs.find((d) => d.id === activeId) ?? docs[0];
    const isGate = mode === 'gate';
    const payloadKey = payload
        ? `${payload.version}:${docs.map((d) => d.id).join('|')}`
        : 'empty';

    const allRead = useMemo(() => {
        if (!isGate) return true;
        if (docs.length === 0) return false;
        return docs.every((d) => readById[d.id] === true);
    }, [docs, isGate, readById]);

    const markActiveReadIfNeeded = useCallback(() => {
        const el = scrollRef.current;
        if (!el || !active) return;
        const atBottom =
            el.scrollTop + el.clientHeight >= el.scrollHeight - SCROLL_THRESHOLD_PX;
        // 内容不足一屏时视为已读完
        const fitsWithoutScroll =
            el.scrollHeight <= el.clientHeight + SCROLL_THRESHOLD_PX;
        if (atBottom || fitsWithoutScroll) {
            setReadById((prev) =>
                prev[active.id] ? prev : { ...prev, [active.id]: true },
            );
        }
    }, [active]);

    useEffect(() => {
        if (!open) return;
        setAgreed(false);
        setReadById({});
        setActiveId(docs[0]?.id ?? 'eula');
    }, [open, payloadKey]);

    // 切换文档或打开时：滚回顶部并检测是否一屏内已读完
    useEffect(() => {
        if (!open || !active) return undefined;
        const el = scrollRef.current;
        if (el) el.scrollTop = 0;
        const raf = window.requestAnimationFrame(markActiveReadIfNeeded);
        return () => window.cancelAnimationFrame(raf);
    }, [open, active?.id, payloadKey, markActiveReadIfNeeded]);

    // 未全部读完时不允许保持勾选
    useEffect(() => {
        if (!allRead && agreed) setAgreed(false);
    }, [allRead, agreed]);

    const canAccept = isGate && allRead && agreed && !submitting && !!payload;

    const readStatusLabel = (() => {
        if (!isGate) return null;
        if (docs.length === 0) return '正在加载协议…';
        if (allRead) return '已读完全部协议';
        const pending = docs
            .filter((d) => !readById[d.id])
            .map((d) => shortTitle(d.title, d.id));
        if (pending.length === docs.length) {
            return `请将各协议阅读至文末（${pending.join('、')}）`;
        }
        return `还需阅读：${pending.join('、')}`;
    })();

    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                // 门禁不允许点遮罩/Esc 悄悄关掉；走 onClose（启动场景会退出应用）
                if (!next && !submitting) onClose();
            }}
        >
            <DialogContent size="xl" dismissOnOutsideClick={!isGate}>
                <DialogHeader>
                    <DialogTitle>
                        {isGate ? '请阅读并同意用户协议' : '用户协议与隐私说明'}
                    </DialogTitle>
                    <DialogDescription>
                        {isGate
                            ? '使用本软件前须完整阅读《用户协议》与《隐私说明》至文末后再确认。同意一次后，仅当协议正文更新时才会再次提示。不同意将退出应用。'
                            : payload?.accepted_at
                                ? `当前协议已同意（${formatAcceptedAt(payload.accepted_at)}）。以下为现行正文。`
                                : '以下为现行用户协议与隐私说明正文。'}
                    </DialogDescription>
                </DialogHeader>

                <div className="flex gap-1 rounded-md bg-inset p-1">
                    {docs.map((doc) => {
                        const done = readById[doc.id] === true;
                        const selected = (active?.id ?? activeId) === doc.id;
                        return (
                            <button
                                key={doc.id}
                                type="button"
                                onClick={() => setActiveId(doc.id)}
                                className={cn(
                                    'flex flex-1 items-center justify-center gap-1.5 rounded-sm px-3 py-1.5 text-xs font-medium transition-colors',
                                    selected
                                        ? 'bg-surface text-text shadow-sm'
                                        : 'text-text-tertiary hover:text-text',
                                )}
                            >
                                {shortTitle(doc.title, doc.id)}
                                {isGate ? (
                                    <span
                                        className={cn(
                                            'h-1.5 w-1.5 shrink-0 rounded-full',
                                            done ? 'bg-success' : 'bg-text-tertiary/40',
                                        )}
                                        title={done ? '已读完' : '未读完'}
                                        aria-hidden
                                    />
                                ) : null}
                            </button>
                        );
                    })}
                </div>

                <div
                    ref={scrollRef}
                    onScroll={markActiveReadIfNeeded}
                    className="max-h-[52vh] overflow-y-auto rounded-md border border-border bg-surface/60 p-4"
                >
                    {active ? (
                        <SimpleMarkdown text={active.text} />
                    ) : (
                        <p className="text-sm text-text-secondary">正在加载协议内容…</p>
                    )}
                </div>

                {isGate ? (
                    <>
                        <div className="flex h-9 items-center justify-between rounded-sm border border-border-subtle bg-inset px-3 text-xs">
                            <span className="text-text-secondary">阅读状态</span>
                            <span
                                className={
                                    allRead
                                        ? 'font-medium text-success'
                                        : 'text-text-tertiary'
                                }
                            >
                                {readStatusLabel}
                            </span>
                        </div>
                        <div className="rounded-md border border-border-subtle bg-inset px-3 py-3">
                            <Checkbox
                                checked={agreed}
                                onCheckedChange={(v) => {
                                    if (!allRead) return;
                                    setAgreed(v);
                                }}
                                disabled={!allRead}
                                label="我已阅读并同意《用户协议》与《隐私说明》"
                                hint={
                                    allRead
                                        ? '同意后将写入本机配置；仅当协议正文变更时才会再次提示。'
                                        : '请先切换各标签并将正文滚动至文末。'
                                }
                            />
                        </div>
                    </>
                ) : null}

                <DialogFooter>
                    {active?.declared_version || payload?.version ? (
                        <span
                            className="mr-auto select-all font-mono text-[10px] tracking-wider text-text-tertiary/60"
                            title="文档版本 · 协议正文内容指纹（变更后需重新确认）"
                        >
                            {[
                                active?.declared_version
                                    ? `v${active.declared_version}`
                                    : null,
                                payload?.version
                                    ? payload.version.slice(0, 8)
                                    : null,
                            ]
                                .filter(Boolean)
                                .join(' · ')}
                        </span>
                    ) : null}
                    <Button variant="ghost" size="sm" disabled={submitting} onClick={onClose}>
                        {isGate ? '不同意并退出' : '关闭'}
                    </Button>
                    {isGate ? (
                        <Button
                            variant="primary"
                            size="sm"
                            className="min-w-28"
                            disabled={!canAccept}
                            onClick={() => {
                                if (!canAccept) return;
                                onAccept();
                            }}
                        >
                            {submitting
                                ? '提交中…'
                                : allRead
                                    ? agreed
                                        ? '同意并继续'
                                        : '请勾选同意'
                                    : '请先读完全部协议'}
                        </Button>
                    ) : null}
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}

function shortTitle(title: string, id: string): string {
    if (id === 'eula') return '用户协议';
    if (id === 'privacy') return '隐私说明';
    const head = title.split(/[（(/]/)[0]?.trim();
    return head || title || id;
}

function formatAcceptedAt(iso: string): string {
    const t = Date.parse(iso);
    if (Number.isNaN(t)) return iso;
    try {
        return new Date(t).toLocaleString();
    } catch {
        return iso;
    }
}
