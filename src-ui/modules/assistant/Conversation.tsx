// 诊断会话的时间线与消息输入。

import { useEffect, useMemo, useRef, useState } from 'react';
import { ArrowDown, ArrowUp, Bot, ChevronRight, Loader2, Square, Wrench } from 'lucide-react';
import type { AssistantSession } from '../../core/domain/assistant/types';
import { assistantTimeline, isAssistantBusy } from '../../core/domain/assistant/session';
import { useOpenExternal } from '../../hooks/useOpenExternal';
import { assistantStore } from '../../hooks/assistant/assistantStore';
import { Button, SimpleMarkdown, TextAreaField } from '../../shared/ui';
import { PermissionCard, QuestionCard } from './InteractionCards';

interface Props {
    session: AssistantSession;
    drafts: Record<string, string>;
    pending: string[];
    onNew: () => void;
}

function toolStatus(status: string): string {
    return (
        (
            {
                pending: '准备中',
                in_progress: '执行中',
                inProgress: '执行中',
                completed: '已完成',
                failed: '失败',
            } as Record<string, string>
        )[status] ?? status
    );
}

export function Conversation({ session, drafts, pending, onNew }: Props) {
    const { info } = session;
    const timeline = useMemo(() => assistantTimeline(session), [session]);
    const scrollRef = useRef<HTMLDivElement>(null);
    const follow = useRef(true);
    const [scrolledAwaySession, setScrolledAwaySession] = useState<string | null>(null);
    const openExternal = useOpenExternal();
    const draft = drafts[info.id] ?? '';
    const busy = isAssistantBusy(info.state);
    const closed = info.state === 'closed';
    const sending = pending.includes(`send:${info.id}`);
    const stopping = pending.includes(`cancel:${info.id}`);
    const recovering = info.state === 'failed' || info.state === 'disconnected';
    useEffect(() => {
        follow.current = true;
    }, [info.id]);
    useEffect(() => {
        if (follow.current && scrollRef.current)
            scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }, [timeline, info.id]);
    const send = () => {
        if (!draft.trim() || busy || closed || sending) return;
        follow.current = true;
        setScrolledAwaySession(null);
        if (scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
        void assistantStore.send(info.id, draft.trim());
    };
    return (
        <div className="flex min-h-0 flex-1 flex-col">
            <div className="relative min-h-0 flex-1">
                <div
                    ref={scrollRef}
                    className="h-full overflow-y-auto overscroll-contain px-4 py-5 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-brand sm:px-6"
                    role="region"
                    aria-label="诊断对话"
                    tabIndex={0}
                    onScroll={() => {
                        const el = scrollRef.current;
                        if (el) {
                            follow.current = el.scrollHeight - el.scrollTop - el.clientHeight < 100;
                            setScrolledAwaySession(follow.current ? null : info.id);
                        }
                    }}
                >
                    <div className="space-y-4">
                        {timeline.length === 0 && (
                            <div className="py-12 text-center">
                                <h2 className="text-sm font-medium text-text-secondary">
                                    一起看看 {info.botName} 的问题
                                </h2>
                                <p className="mt-2 text-xs leading-5 text-text-tertiary">
                                    描述你看到了什么、从什么时候开始，也可以直接让助手检查连接。
                                </p>
                                <div className="mt-4 flex flex-wrap justify-center gap-2">
                                    {['帮我检查当前运行状态', '为什么收不到消息？'].map((text) => (
                                        <Button
                                            key={text}
                                            size="sm"
                                            disabled={busy || closed}
                                            onClick={() => assistantStore.draft(info.id, text)}
                                        >
                                            {text}
                                        </Button>
                                    ))}
                                </div>
                            </div>
                        )}
                        {timeline.map((item) => {
                            if (item.kind === 'permission')
                                return (
                                    <PermissionCard
                                        key={`permission:${item.value.id}`}
                                        request={item.value}
                                        pending={pending.includes(item.value.id) || closed}
                                        onRespond={(option) => {
                                            void assistantStore.permission(
                                                info.id,
                                                item.value.id,
                                                option,
                                            );
                                        }}
                                    />
                                );
                            if (item.kind === 'question')
                                return (
                                    <QuestionCard
                                        key={`question:${item.value.id}`}
                                        question={item.value}
                                        pending={pending.includes(item.value.id) || closed}
                                        draft={drafts[`question:${info.id}:${item.value.id}`] ?? ''}
                                        onDraft={(value) =>
                                            assistantStore.draft(
                                                `question:${info.id}:${item.value.id}`,
                                                value,
                                            )
                                        }
                                        onRespond={(answer) => {
                                            void assistantStore.question(
                                                info.id,
                                                item.value.id,
                                                answer,
                                            );
                                        }}
                                    />
                                );
                            if (item.kind === 'tool')
                                return (
                                    <details
                                        key={`tool:${item.value.id}`}
                                        className="group max-w-2xl overflow-hidden rounded-lg bg-inset/60 text-xs"
                                    >
                                        <summary className="flex cursor-pointer list-none items-center gap-2.5 px-4 py-3 text-text-secondary transition-colors hover:bg-inset/60 focus-visible:outline-brand">
                                            <ChevronRight
                                                size={12}
                                                className="shrink-0 transition-transform group-open:rotate-90"
                                            />
                                            <Wrench
                                                size={13}
                                                className="shrink-0 text-text-tertiary"
                                            />
                                            <span className="min-w-0 flex-1 break-words font-medium">
                                                {item.value.title}
                                            </span>
                                            <span
                                                className={
                                                    item.value.status === 'failed'
                                                        ? 'max-w-[40%] shrink-0 truncate text-danger'
                                                        : 'max-w-[40%] shrink-0 truncate text-text-tertiary'
                                                }
                                            >
                                                {toolStatus(item.value.status)}
                                            </span>
                                        </summary>
                                        <pre className="max-h-80 overflow-auto whitespace-pre-wrap break-words border-t border-border-subtle bg-inset/40 px-4 py-3 font-mono text-xs leading-5 text-text-secondary">
                                            {item.value.details || '暂无详情'}
                                        </pre>
                                    </details>
                                );
                            const message = item.value;
                            if (message.role === 'system')
                                return (
                                    <p
                                        key={`message:${message.id}`}
                                        className="whitespace-pre-wrap break-words text-xs leading-5 text-text-tertiary"
                                    >
                                        {message.text}
                                    </p>
                                );
                            return (
                                <article
                                    key={`message:${message.id}`}
                                    aria-label={message.role === 'user' ? '你的消息' : '助手回复'}
                                    className={`flex items-start gap-2.5 ${message.role === 'user' ? 'flex-row-reverse' : ''}`}
                                >
                                    <span className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-inset text-text-secondary">
                                        {message.role === 'user' ? '你' : <Bot size={17} />}
                                    </span>
                                    <div className="min-w-0 max-w-[min(85%,620px,calc(100%-42px))] sm:max-w-[min(76%,620px,calc(100%-42px))]">
                                        <div
                                            className={`mb-1 text-2xs text-text-tertiary ${message.role === 'user' ? 'text-right' : ''}`}
                                        >
                                            {message.role === 'user' ? '你' : '诊断助手'}
                                        </div>
                                        <div
                                            className={
                                                message.role === 'user'
                                                    ? 'rounded-[14px] rounded-tr-sm bg-[color-mix(in_srgb,var(--action-primary)_17%,var(--surface-card))] px-3 py-2'
                                                    : 'rounded-[14px] rounded-tl-sm bg-[color-mix(in_srgb,var(--text-primary)_6%,var(--surface-card))] px-3 py-2'
                                            }
                                        >
                                            {message.role === 'user' ? (
                                                <p className="whitespace-pre-wrap break-words text-sm leading-6 text-text">
                                                    {message.text}
                                                </p>
                                            ) : (
                                                <SimpleMarkdown
                                                    text={message.text}
                                                    emptyFallback="正在整理…"
                                                    onOpenLink={openExternal}
                                                    className="break-words"
                                                />
                                            )}
                                        </div>
                                    </div>
                                </article>
                            );
                        })}
                        {info.state === 'running' && (
                            <div
                                role="status"
                                className="flex items-center gap-2 text-xs text-text-tertiary"
                            >
                                <Loader2 size={13} className="animate-spin text-brand" />
                                助手正在处理…
                            </div>
                        )}
                    </div>
                </div>
                {scrolledAwaySession === info.id && (
                    <Button
                        size="icon"
                        variant="secondary"
                        className="absolute bottom-3 right-4 h-8 w-8 rounded-full shadow-card"
                        aria-label="滚动到最新消息"
                        onClick={() => {
                            follow.current = true;
                            setScrolledAwaySession(null);
                            if (scrollRef.current)
                                scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
                        }}
                    >
                        <ArrowDown size={15} />
                    </Button>
                )}
            </div>
            <div className="shrink-0 px-4 pb-4 pt-2 sm:px-5">
                <div className="space-y-2">
                    {closed ? (
                        <div className="flex flex-wrap items-center justify-between gap-3 py-2 text-sm text-text-secondary">
                            <span>此会话已关闭，记录仍可查看。</span>
                            <Button size="sm" onClick={onNew}>
                                新建会话
                            </Button>
                        </div>
                    ) : (
                        <>
                            <div className="rounded-[14px] border border-border bg-surface px-2.5 pb-1.5 pt-1.5 focus-within:border-brand/55 focus-within:ring-[3px] focus-within:ring-brand/10">
                                <TextAreaField
                                    aria-label="输入诊断问题"
                                    className="[&_textarea]:border-0 [&_textarea]:bg-transparent [&_textarea]:focus:ring-0"
                                    placeholder={
                                        recovering
                                            ? '补充问题，恢复连接后继续…'
                                            : '描述问题，Enter 发送，Shift + Enter 换行'
                                    }
                                    minRows={2}
                                    maxRows={7}
                                    maxLength={16_000}
                                    value={draft}
                                    disabled={info.state === 'starting'}
                                    onValueChange={(value) => assistantStore.draft(info.id, value)}
                                    onKeyDown={(event) => {
                                        if (
                                            event.key === 'Enter' &&
                                            !event.shiftKey &&
                                            !event.nativeEvent.isComposing
                                        ) {
                                            event.preventDefault();
                                            send();
                                        }
                                    }}
                                />
                                <div className="mt-1 flex items-center justify-between gap-2 px-1 pb-0.5">
                                    <div
                                        className="min-w-0 text-2xs text-text-tertiary"
                                        aria-live="polite"
                                    >
                                        {info.state === 'waitingForInput'
                                            ? '请先回应上方的审批或追问'
                                            : recovering
                                              ? '发送新消息将尝试恢复此会话'
                                              : draft.length > 12_000
                                                ? `${draft.length} / 16000`
                                                : 'Enter 发送 · Shift + Enter 换行'}
                                    </div>
                                    <div className="flex shrink-0 gap-2">
                                        {busy ? (
                                            <Button
                                                size="icon"
                                                className="h-8 w-8 rounded-full"
                                                aria-label="停止"
                                                disabled={stopping}
                                                onClick={() => {
                                                    void assistantStore.cancel(info.id);
                                                }}
                                            >
                                                {stopping ? (
                                                    <Loader2 size={13} className="animate-spin" />
                                                ) : (
                                                    <Square size={12} />
                                                )}
                                            </Button>
                                        ) : (
                                            <Button
                                                size="icon"
                                                className="h-8 w-8 rounded-full disabled:bg-inset disabled:text-text-disabled disabled:opacity-100"
                                                variant="primary"
                                                aria-label={recovering ? '恢复并发送' : '发送'}
                                                disabled={!draft.trim() || sending}
                                                onClick={send}
                                            >
                                                {sending ? (
                                                    <Loader2 size={14} className="animate-spin" />
                                                ) : (
                                                    <ArrowUp size={14} />
                                                )}
                                            </Button>
                                        )}
                                    </div>
                                </div>
                            </div>
                        </>
                    )}
                </div>
            </div>
        </div>
    );
}
