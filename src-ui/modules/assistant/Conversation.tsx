// 诊断会话的时间线与消息输入。

import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import {
    ArrowDown,
    ArrowUp,
    CheckCircle2,
    ChevronRight,
    CircleDashed,
    Loader2,
    Plus,
    Sparkles,
    Square,
    Wrench,
    XCircle,
} from 'lucide-react';
import type { AssistantSession, AssistantToolCall } from '../../core/domain/assistant/types';
import {
    assistantStarterPrompts,
    assistantTimeline,
    groupAssistantTimeline,
    isAssistantBusy,
    type AssistantTimelineBlock,
} from '../../core/domain/assistant/session';
import { useOpenExternal } from '../../hooks/useOpenExternal';
import { assistantStore } from '../../hooks/assistant/assistantStore';
import { Button, SimpleMarkdown } from '../../shared/ui';
import { cn } from '../../shared/utils/cn';
import { Composer } from './Composer';
import { PermissionCard, QuestionCard } from './InteractionCards';

interface Props {
    session: AssistantSession;
    drafts: Record<string, string>;
    pending: string[];
    onNew: () => void;
}

const toolStatusLabel: Record<string, string> = {
    pending: '准备中',
    in_progress: '执行中',
    inProgress: '执行中',
    completed: '已完成',
    failed: '失败',
};

function toolRunning(status: string): boolean {
    return status === 'pending' || status === 'in_progress' || status === 'inProgress';
}

// 会话已停下时残留的「执行中」不再转圈，避免历史记录看着像还在跑。
function ToolStatusIcon({ status, live }: { status: string; live: boolean }) {
    if (status === 'completed') return <CheckCircle2 size={13} className="shrink-0 text-success" />;
    if (status === 'failed') return <XCircle size={13} className="shrink-0 text-danger" />;
    if (toolRunning(status))
        return live ? (
            <Loader2 size={13} className="shrink-0 animate-spin text-brand" />
        ) : (
            <CircleDashed size={13} className="shrink-0 text-text-tertiary" />
        );
    return <Wrench size={13} className="shrink-0 text-text-tertiary" />;
}

function ToolRow({ tool, live }: { tool: AssistantToolCall; live: boolean }) {
    return (
        <details className="group/tool">
            <summary className="flex cursor-pointer list-none items-center gap-2 px-3 py-2 text-xs transition-colors hover:bg-inset/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-brand">
                <ToolStatusIcon status={tool.status} live={live} />
                <span className="min-w-0 flex-1 truncate font-medium text-text-secondary">
                    {tool.title}
                </span>
                <span
                    className={cn(
                        'max-w-[40%] shrink-0 truncate text-2xs',
                        tool.status === 'failed' ? 'text-danger' : 'text-text-tertiary',
                    )}
                >
                    {!live && toolRunning(tool.status)
                        ? '已中断'
                        : (toolStatusLabel[tool.status] ?? tool.status)}
                </span>
                <ChevronRight
                    size={12}
                    className="shrink-0 text-text-tertiary transition-transform group-open/tool:rotate-90"
                />
            </summary>
            <pre className="max-h-72 overflow-auto whitespace-pre-wrap break-words border-t border-border-subtle bg-inset/50 px-3 py-2.5 font-mono text-2xs leading-5 text-text-secondary">
                {tool.details || '暂无详情'}
            </pre>
        </details>
    );
}

function ToolGroup({ tools, live }: { tools: AssistantToolCall[]; live: boolean }) {
    if (tools.length === 1)
        return (
            <div className="overflow-hidden rounded-lg border border-border-subtle bg-surface">
                <ToolRow tool={tools[0]!} live={live} />
            </div>
        );
    const failed = tools.filter((tool) => tool.status === 'failed').length;
    const running = tools.some((tool) => toolRunning(tool.status));
    return (
        <details className="group/tools overflow-hidden rounded-lg border border-border-subtle bg-surface">
            <summary className="flex cursor-pointer list-none items-center gap-2 px-3 py-2 text-xs text-text-secondary transition-colors hover:bg-inset/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-brand">
                <ToolStatusIcon
                    status={failed ? 'failed' : running ? 'in_progress' : 'completed'}
                    live={live}
                />
                <span className="min-w-0 flex-1 truncate font-medium">
                    {live && running ? '正在调用' : '调用了'} {tools.length} 个工具
                    {!live && running && (
                        <span className="ml-1.5 text-text-tertiary">· 已中断</span>
                    )}
                    {failed > 0 && <span className="ml-1.5 text-danger">· {failed} 个失败</span>}
                </span>
                <ChevronRight
                    size={12}
                    className="shrink-0 text-text-tertiary transition-transform group-open/tools:rotate-90"
                />
            </summary>
            <div className="divide-y divide-border-subtle border-t border-border-subtle">
                {tools.map((tool) => (
                    <ToolRow key={tool.id} tool={tool} live={live} />
                ))}
            </div>
        </details>
    );
}

// 同一轮里连着的助手内容只在第一块显示头像，后面的块对齐到同一列。
function AssistantRow({ lead, children }: { lead: boolean; children: ReactNode }) {
    return (
        <div className="flex items-start gap-3">
            {lead ? (
                <span
                    aria-hidden
                    className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-brand-soft text-brand"
                >
                    <Sparkles size={14} />
                </span>
            ) : (
                <span aria-hidden className="w-7 shrink-0" />
            )}
            <div className="min-w-0 flex-1">{children}</div>
        </div>
    );
}

function startsTurn(previous: AssistantTimelineBlock | undefined): boolean {
    return !previous || (previous.kind === 'message' && previous.value.role !== 'assistant');
}

export function Conversation({ session, drafts, pending, onNew }: Props) {
    const { info } = session;
    const blocks = useMemo(() => groupAssistantTimeline(assistantTimeline(session)), [session]);
    const turnStart = blocks.reduce(
        (latest, block, index) =>
            block.kind === 'message' && block.value.role === 'user' ? index : latest,
        -1,
    );
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
    const last = blocks[blocks.length - 1];
    const thinking =
        info.state === 'running' && !(last?.kind === 'message' && last.value.role === 'assistant');
    useEffect(() => {
        follow.current = true;
    }, [info.id]);
    useEffect(() => {
        if (follow.current && scrollRef.current)
            scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }, [blocks, thinking, info.id]);
    const toBottom = () => {
        follow.current = true;
        setScrolledAwaySession(null);
        if (scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    };
    const send = (text = draft) => {
        if (!text.trim() || busy || closed || sending) return;
        toBottom();
        void assistantStore.send(info.id, text.trim());
    };
    return (
        <div className="flex min-h-0 flex-1 flex-col">
            <div className="relative min-h-0 flex-1">
                <div
                    ref={scrollRef}
                    className="h-full overflow-y-auto overscroll-contain focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-brand"
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
                    <div className="mx-auto w-full max-w-3xl space-y-5 px-4 py-6 sm:px-6">
                        {blocks.length === 0 && (
                            <div className="py-14 text-center">
                                <h2 className="text-sm font-medium text-text">
                                    一起看看 {info.botName} 的问题
                                </h2>
                                <p className="mt-1.5 text-xs leading-5 text-text-tertiary">
                                    描述你看到了什么、从什么时候开始。
                                </p>
                                <div className="mt-5 flex flex-wrap justify-center gap-2">
                                    {assistantStarterPrompts.map((text) => (
                                        <button
                                            key={text}
                                            type="button"
                                            disabled={busy || closed || sending}
                                            onClick={() => send(text)}
                                            className="rounded-pill border border-border-subtle bg-surface px-3 py-1.5 text-xs text-text-secondary transition-colors hover:border-border hover:bg-inset hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand disabled:pointer-events-none disabled:opacity-50"
                                        >
                                            {text}
                                        </button>
                                    ))}
                                </div>
                            </div>
                        )}
                        {blocks.map((block, index) => {
                            const lead = startsTurn(blocks[index - 1]);
                            if (block.kind === 'tools')
                                return (
                                    <AssistantRow key={`tools:${block.value[0]!.id}`} lead={lead}>
                                        <ToolGroup
                                            tools={block.value}
                                            live={busy && index > turnStart}
                                        />
                                    </AssistantRow>
                                );
                            if (block.kind === 'permission')
                                return (
                                    <AssistantRow key={`permission:${block.value.id}`} lead={lead}>
                                        <PermissionCard
                                            request={block.value}
                                            pending={pending.includes(block.value.id) || closed}
                                            onRespond={(option) => {
                                                void assistantStore.permission(
                                                    info.id,
                                                    block.value.id,
                                                    option,
                                                );
                                            }}
                                        />
                                    </AssistantRow>
                                );
                            if (block.kind === 'question')
                                return (
                                    <AssistantRow key={`question:${block.value.id}`} lead={lead}>
                                        <QuestionCard
                                            question={block.value}
                                            pending={pending.includes(block.value.id) || closed}
                                            draft={
                                                drafts[`question:${info.id}:${block.value.id}`] ??
                                                ''
                                            }
                                            onDraft={(value) =>
                                                assistantStore.draft(
                                                    `question:${info.id}:${block.value.id}`,
                                                    value,
                                                )
                                            }
                                            onRespond={(answer) => {
                                                void assistantStore.question(
                                                    info.id,
                                                    block.value.id,
                                                    answer,
                                                );
                                            }}
                                        />
                                    </AssistantRow>
                                );
                            const message = block.value;
                            if (message.role === 'system')
                                return (
                                    <p
                                        key={`message:${message.id}`}
                                        className="mx-auto max-w-xl whitespace-pre-wrap break-words text-center text-2xs leading-5 text-text-tertiary"
                                    >
                                        {message.text}
                                    </p>
                                );
                            if (message.role === 'user')
                                return (
                                    <article
                                        key={`message:${message.id}`}
                                        aria-label="你的消息"
                                        className="flex justify-end"
                                    >
                                        <p className="max-w-[min(85%,560px)] whitespace-pre-wrap break-words rounded-2xl rounded-br-md bg-brand-soft px-3.5 py-2 text-sm leading-6 text-text">
                                            {message.text}
                                        </p>
                                    </article>
                                );
                            return (
                                <article key={`message:${message.id}`} aria-label="助手回复">
                                    <AssistantRow lead={lead}>
                                        <SimpleMarkdown
                                            text={message.text}
                                            emptyFallback="正在整理…"
                                            onOpenLink={openExternal}
                                            className="break-words pt-0.5"
                                        />
                                    </AssistantRow>
                                </article>
                            );
                        })}
                        {thinking && (
                            <AssistantRow lead={startsTurn(last)}>
                                <p
                                    role="status"
                                    className="flex h-7 items-center gap-2 text-xs text-text-tertiary"
                                >
                                    <span className="flex gap-1" aria-hidden>
                                        {[0, 150, 300].map((delay) => (
                                            <span
                                                key={delay}
                                                className="h-1.5 w-1.5 rounded-full bg-brand/60 motion-safe:animate-pulse"
                                                style={{ animationDelay: `${delay}ms` }}
                                            />
                                        ))}
                                    </span>
                                    正在分析…
                                </p>
                            </AssistantRow>
                        )}
                    </div>
                </div>
                {scrolledAwaySession === info.id && (
                    <Button
                        size="icon"
                        variant="secondary"
                        className="absolute bottom-3 left-1/2 h-8 w-8 -translate-x-1/2 rounded-full shadow-card"
                        aria-label="滚动到最新消息"
                        onClick={toBottom}
                    >
                        <ArrowDown size={15} />
                    </Button>
                )}
            </div>
            <div className="shrink-0 px-4 pb-4 pt-1 sm:px-6">
                <div className="mx-auto w-full max-w-3xl">
                    {closed ? (
                        <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-dashed border-border px-4 py-3 text-sm text-text-secondary">
                            <span>此会话已关闭，记录仍可查看。</span>
                            <Button size="sm" onClick={onNew}>
                                <Plus size={14} />
                                新建会话
                            </Button>
                        </div>
                    ) : (
                        <Composer
                            label="输入诊断问题"
                            value={draft}
                            placeholder={
                                recovering
                                    ? '补充问题，恢复连接后继续…'
                                    : '继续描述，Enter 发送，Shift + Enter 换行'
                            }
                            disabled={info.state === 'starting'}
                            status={
                                stopping
                                    ? '正在停止…'
                                    : info.state === 'starting'
                                      ? '正在连接助手…'
                                      : info.state === 'waitingForInput'
                                        ? '请先回应上方的审批或追问'
                                        : recovering
                                          ? '发送新消息将尝试恢复此会话'
                                          : draft.length > 12_000
                                            ? `${draft.length} / 16000`
                                            : busy
                                              ? '正在分析，可先写下一条问题'
                                              : sending
                                                ? '正在发送…'
                                                : 'Enter 发送，Shift + Enter 换行'
                            }
                            onChange={(value) => assistantStore.draft(info.id, value)}
                            onSubmit={() => send()}
                            action={
                                busy ? (
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
                                            <Square size={11} className="fill-current" />
                                        )}
                                    </Button>
                                ) : (
                                    <Button
                                        size="icon"
                                        className="h-8 w-8 rounded-full disabled:bg-inset disabled:text-text-disabled disabled:opacity-100 disabled:shadow-none"
                                        variant="primary"
                                        aria-label={recovering ? '恢复并发送' : '发送'}
                                        disabled={!draft.trim() || sending}
                                        onClick={() => send()}
                                    >
                                        {sending ? (
                                            <Loader2 size={14} className="animate-spin" />
                                        ) : (
                                            <ArrowUp size={15} />
                                        )}
                                    </Button>
                                )
                            }
                        />
                    )}
                </div>
            </div>
        </div>
    );
}
