import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
    ArrowDown,
    Check,
    CheckCircle2,
    ChevronRight,
    FileText,
} from 'lucide-react';
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
    const [scrollProgress, setScrollProgress] = useState(0);
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

    const isCurrentRead = active ? readById[active.id] === true : false;

    const handleScroll = useCallback(() => {
        const el = scrollRef.current;
        if (!el || !active) return;
        const maxScroll = el.scrollHeight - el.clientHeight;
        if (maxScroll <= SCROLL_THRESHOLD_PX) {
            setScrollProgress(100);
            setReadById((prev) =>
                prev[active.id] ? prev : { ...prev, [active.id]: true },
            );
            return;
        }
        const current = Math.min(
            100,
            Math.max(0, Math.round((el.scrollTop / maxScroll) * 100)),
        );
        setScrollProgress(current);

        const atBottom =
            el.scrollTop + el.clientHeight >= el.scrollHeight - SCROLL_THRESHOLD_PX;
        if (atBottom) {
            setReadById((prev) =>
                prev[active.id] ? prev : { ...prev, [active.id]: true },
            );
        }
    }, [active]);

    const scrollToBottom = useCallback(() => {
        const el = scrollRef.current;
        if (!el) return;
        el.scrollTo({
            top: el.scrollHeight,
            behavior: 'smooth',
        });
    }, []);

    useEffect(() => {
        if (!open) return;
        setAgreed(false);
        setReadById({});
        setScrollProgress(0);
        setActiveId(docs[0]?.id ?? 'eula');
    }, [open, payloadKey]);

    // 切换文档或打开时：滚回顶部并检测是否一屏内已读完
    useEffect(() => {
        if (!open || !active) return undefined;
        const el = scrollRef.current;
        if (el) {
            el.scrollTop = 0;
            setScrollProgress(0);
        }
        const raf = window.requestAnimationFrame(handleScroll);
        return () => window.cancelAnimationFrame(raf);
    }, [open, active?.id, payloadKey, handleScroll]);

    // 未全部读完时不允许保持勾选
    useEffect(() => {
        if (!allRead && agreed) setAgreed(false);
    }, [allRead, agreed]);

    const canAccept = isGate && allRead && agreed && !submitting && !!payload;
    const pendingDocs = docs.filter((d) => !readById[d.id]);

    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                // 门禁不允许点遮罩/Esc 悄悄关掉；走 onClose（启动场景会退出应用）
                if (!next && !submitting) onClose();
            }}
        >
            <DialogContent size="xl" dismissOnOutsideClick={!isGate} className="gap-4 p-6">
                {/* 头部：纯净文字排版与留白 */}
                <DialogHeader className="space-y-1 pb-1">
                    <DialogTitle className="text-base font-semibold tracking-tight text-text">
                        {isGate ? '用户协议与隐私说明' : '用户协议与隐私条款'}
                    </DialogTitle>
                    <DialogDescription className="text-xs text-text-secondary leading-relaxed">
                        {isGate
                            ? '首次启动或条款更新时须阅读并确认。请完整阅读后继续。'
                            : payload?.accepted_at
                                ? `当前已同意（${formatAcceptedAt(payload.accepted_at)}）。以下为现行正文。`
                                : '以下为现行用户协议与隐私说明正文。'}
                    </DialogDescription>
                </DialogHeader>

                {/* 融合一体的主体卡片容器：使用 rounded-lg 与 theme border/bg tokens */}
                <div className="relative flex flex-col overflow-hidden rounded-lg border border-border bg-surface shadow-card">
                    {/* 顶栏：Tab 切换与阅读进度 */}
                    <div className="flex items-center justify-between border-b border-border-subtle bg-inset px-3 py-2">
                        <div className="flex items-center gap-1.5">
                            {docs.map((doc) => {
                                const done = readById[doc.id] === true;
                                const selected = (active?.id ?? activeId) === doc.id;
                                return (
                                    <button
                                        key={doc.id}
                                        type="button"
                                        onClick={() => setActiveId(doc.id)}
                                        className={cn(
                                            'group relative flex items-center gap-2 rounded-md px-3 py-1.5 text-xs font-medium transition-all duration-150 active:scale-[0.98]',
                                            selected
                                                ? 'border border-border bg-surface text-text shadow-card font-medium'
                                                : 'border border-transparent text-text-secondary hover:border-border-subtle hover:bg-elevated hover:text-text',
                                        )}
                                    >
                                        <FileText
                                            className={cn(
                                                'h-3.5 w-3.5 shrink-0 transition-colors duration-150',
                                                selected
                                                    ? 'text-brand'
                                                    : 'text-text-tertiary group-hover:text-brand',
                                            )}
                                        />
                                        <span>{shortTitle(doc.title, doc.id)}</span>
                                        {doc.declared_version && (
                                            <span className="font-mono text-2xs text-text-tertiary transition-colors group-hover:text-text-secondary">
                                                v{doc.declared_version}
                                            </span>
                                        )}
                                        {isGate && (
                                            <span
                                                className={cn(
                                                    'flex items-center gap-0.5 rounded-pill px-1.5 py-0.5 text-2xs leading-none transition-colors duration-150',
                                                    done
                                                        ? 'bg-success-soft font-medium text-success'
                                                        : 'bg-muted text-text-tertiary group-hover:text-text-secondary',
                                                )}
                                            >
                                                {done ? (
                                                    <>
                                                        <Check className="h-2.5 w-2.5 stroke-[2.5]" />
                                                        <span>已读</span>
                                                    </>
                                                ) : (
                                                    <span>待读</span>
                                                )}
                                            </span>
                                        )}
                                    </button>
                                );
                            })}
                        </div>

                        {isGate && (
                            <div className="flex items-center gap-1.5 pr-2 font-mono text-xs text-text-tertiary">
                                <span>阅读进度</span>
                                <span
                                    className={cn(
                                        'font-semibold tabular-nums',
                                        scrollProgress === 100 ? 'text-success' : 'text-brand',
                                    )}
                                >
                                    {scrollProgress}%
                                </span>
                            </div>
                        )}
                    </div>

                    {/* 进度条细线（位于顶栏与正文之间，主题色延展） */}
                    {isGate && (
                        <div className="h-[2px] w-full overflow-hidden bg-border-subtle">
                            <div
                                className="h-full bg-brand transition-all duration-150"
                                style={{ width: `${scrollProgress}%` }}
                            />
                        </div>
                    )}

                    {/* 中部：Markdown 文本视口 */}
                    <div
                        ref={scrollRef}
                        onScroll={handleScroll}
                        className="max-h-[46vh] overflow-y-auto bg-canvas px-6 py-5 scroll-smooth focus:outline-none"
                    >
                        {active ? (
                            <SimpleMarkdown
                                text={active.text}
                                className="space-y-3.5 text-sm leading-relaxed text-text-secondary"
                            />
                        ) : (
                            <p className="text-sm text-text-secondary">正在加载协议内容…</p>
                        )}
                    </div>

                    {/* 直达文末浮动按钮 */}
                    {isGate && !isCurrentRead && (
                        <div className="pointer-events-none absolute bottom-16 right-5">
                            <button
                                type="button"
                                onClick={scrollToBottom}
                                className="pointer-events-auto flex items-center gap-1.5 rounded-pill border border-border bg-surface px-3.5 py-1.5 text-xs font-medium text-text-secondary shadow-popover transition-all hover:bg-elevated hover:text-text active:scale-95"
                            >
                                <span>直达文末</span>
                                <ArrowDown className="h-3.5 w-3.5 animate-bounce" />
                            </button>
                        </div>
                    )}

                    {/* 底栏：完全融合在卡片底部的确认栏 */}
                    {isGate ? (
                        <div className="flex items-center justify-between gap-3 border-t border-border-subtle bg-inset px-5 py-3">
                            <Checkbox
                                checked={agreed}
                                onCheckedChange={(v) => {
                                    if (!allRead) return;
                                    setAgreed(v);
                                }}
                                disabled={!allRead}
                                label={
                                    <span className="text-xs font-medium text-text select-none">
                                        我已阅读并同意《用户协议》与《隐私说明》
                                    </span>
                                }
                            />
                            <div className="shrink-0">
                                {allRead ? (
                                    <span className="inline-flex items-center gap-1.5 text-xs font-medium text-success">
                                        <CheckCircle2 className="h-3.5 w-3.5" />
                                        <span>已读完全部条款</span>
                                    </span>
                                ) : (
                                    <div className="flex items-center gap-1.5 text-xs text-text-tertiary">
                                        <span>还需阅读：</span>
                                        {pendingDocs.map((d) => (
                                            <button
                                                key={d.id}
                                                type="button"
                                                onClick={() => setActiveId(d.id)}
                                                className="inline-flex items-center gap-0.5 rounded-xs px-2 py-0.5 text-xs font-medium text-brand transition-colors hover:bg-brand-soft"
                                            >
                                                <span>{shortTitle(d.title, d.id)}</span>
                                                <ChevronRight className="h-3 w-3" />
                                            </button>
                                        ))}
                                    </div>
                                )}
                            </div>
                        </div>
                    ) : null}
                </div>

                {/* 底部操作区 */}
                <DialogFooter className="mt-1 pt-1">
                    {active?.declared_version ? (
                        <span
                            className="mr-auto select-all font-mono text-2xs tracking-wider text-text-tertiary"
                            title="协议版本号；升版时才会要求重新确认"
                        >
                            v{active.declared_version}
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
                                        ? '同意并进入'
                                        : '请勾选同意'
                                    : '请读完全部条款'}
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
