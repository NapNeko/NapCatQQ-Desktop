// 试聊：Koishi 控制台沙盒的原生版。消息走桌面端的控制台长连接（上游只推给「拥有」这个
// platform 的连接），页面上 1.5s 轮询拿缓冲；发完立刻刷一次、1.2s 后再补一次。
// 内容是 Koishi 的元素语法串，parseKoishiMessage 拆成段画（文字 / @ / 图 / 引用）。

import { useEffect, useMemo, useRef, useState } from 'react';
import { Bot, Eraser, Send, User } from 'lucide-react';
import { Button, Spinner, TextField } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import {
    KOISHI_SANDBOX_DEFAULT_USER,
    KOISHI_SANDBOX_PLATFORM,
    parseKoishiMessage,
    sandboxChannel,
    type KoishiSegment,
} from '../../../../core/domain/apps/koishiConsole';
import { useKoishiSandboxMessages, useKoishiSandboxSend } from '../../../../hooks/apps/useKoishiConsole';
import type { AppInstance, KoishiSandboxMessage } from '../../../../core/ipc/types';

type Mode = 'private' | 'guild';

export const KoishiSandboxTab: React.FC<{ instance: AppInstance }> = ({ instance }) => {
    const running = instance.state === 'running';
    const [mode, setMode] = useState<Mode>('private');
    const [user, setUser] = useState(KOISHI_SANDBOX_DEFAULT_USER);
    const channel = sandboxChannel(mode, user);
    const messages = useKoishiSandboxMessages(instance.id, running);
    const send = useKoishiSandboxSend(instance.id);
    const [text, setText] = useState('');
    const areaRef = useRef<HTMLTextAreaElement>(null);
    const listRef = useRef<HTMLDivElement>(null);

    const rows = useMemo(
        () => (messages.data ?? []).filter((m) => m.platform === KOISHI_SANDBOX_PLATFORM && m.channel === channel),
        [messages.data, channel],
    );

    useEffect(() => {
        const el = listRef.current;
        if (el) el.scrollTop = el.scrollHeight;
    }, [rows.length]);

    const submit = async () => {
        const content = text.trim();
        if (!content || send.isPending || !running) return;
        setText('');
        await send.mutateAsync({
            platform: KOISHI_SANDBOX_PLATFORM,
            user,
            channel,
            content,
        });
        areaRef.current?.focus();
    };

    return (
        <div className="flex min-h-0 flex-1 flex-col gap-2 pb-3">
            <div className="flex shrink-0 flex-wrap items-center gap-2">
                <div
                    role="tablist"
                    aria-label="对话模式"
                    className="flex h-7 items-center gap-0.5 rounded-md bg-inset/60 p-0.5"
                >
                    {(
                        [
                            ['private', '私聊'],
                            ['guild', '群聊'],
                        ] as const
                    ).map(([v, label]) => (
                        <button
                            key={v}
                            type="button"
                            role="tab"
                            aria-selected={mode === v}
                            onClick={() => setMode(v)}
                            className={cn(
                                'h-6 rounded-sm px-2.5 text-[11.5px] font-medium leading-6 transition-colors',
                                mode === v
                                    ? 'bg-surface text-text shadow-[0_1px_2px_rgba(0,0,0,0.04)]'
                                    : 'text-text-tertiary hover:text-text',
                            )}
                        >
                            {label}
                        </button>
                    ))}
                </div>
                {mode === 'private' && (
                    <div className="w-36">
                        <TextField
                            aria-label="模拟的用户名"
                            value={user}
                            disabled={!running}
                            onValueChange={(v) => setUser(v.trim() || KOISHI_SANDBOX_DEFAULT_USER)}
                        />
                    </div>
                )}
                <span className="text-2xs text-text-tertiary">不连 QQ，指令和插件走的是实例里的真管线</span>
                <div className="ml-auto">
                    <Button
                        size="sm"
                        variant="ghost"
                        disabled={!running || rows.length === 0 || send.isPending}
                        title="上游沙盒的清屏指令"
                        onClick={() => void send.mutateAsync({ platform: KOISHI_SANDBOX_PLATFORM, user, channel, content: 'clear' })}
                    >
                        <Eraser size={13} />
                        清屏
                    </Button>
                </div>
            </div>

            <div
                ref={listRef}
                className="min-h-0 flex-1 overflow-y-auto rounded-lg border border-border-subtle bg-inset/50 px-4 py-3"
                aria-live="polite"
                aria-label="沙盒消息"
            >
                {!running ? (
                    <Hint text="启动实例后才能试聊" />
                ) : messages.isLoading ? (
                    <Hint text="正在连接沙盒…" spinner />
                ) : messages.error ? (
                    <Hint text={messages.error.message} error />
                ) : rows.length === 0 ? (
                    <Hint text={`说点什么试试，比如 help；这里是${mode === 'guild' ? '群聊' : '私聊'}模式`} />
                ) : (
                    rows.map((m, i) => (
                        <Bubble key={m.id} msg={m} prev={rows[i - 1]} selfName={user} />
                    ))
                )}
            </div>

            <div
                className={cn(
                    'flex shrink-0 items-end gap-2 rounded-lg border bg-surface p-2 transition-colors',
                    'border-border-subtle focus-within:border-brand/40',
                )}
            >
                <textarea
                    ref={areaRef}
                    value={text}
                    disabled={!running || send.isPending}
                    placeholder={running ? '发消息，Enter 发送，Shift+Enter 换行' : '实例没在运行'}
                    aria-label="沙盒输入框"
                    rows={1}
                    className="max-h-36 min-h-[24px] flex-1 resize-none bg-transparent px-1.5 py-1 text-[13px] leading-snug text-text outline-none placeholder:text-text-disabled"
                    onChange={(e) => {
                        setText(e.target.value);
                        const el = e.target;
                        el.style.height = 'auto';
                        el.style.height = `${Math.min(el.scrollHeight, 144)}px`;
                    }}
                    onKeyDown={(e) => {
                        if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
                            e.preventDefault();
                            void submit();
                        }
                    }}
                />
                <Button
                    size="sm"
                    variant="primary"
                    disabled={!running || send.isPending || !text.trim()}
                    onClick={() => void submit()}
                    aria-label="发送"
                >
                    {send.isPending ? <Spinner size="sm" className="text-white" /> : <Send size={13} />}
                    发送
                </Button>
            </div>
        </div>
    );
};

function Hint({ text, error, spinner }: { text: string; error?: boolean; spinner?: boolean }) {
    return (
        <div className="flex h-full min-h-[200px] flex-col items-center justify-center gap-2 text-center">
            {spinner && <Spinner size="sm" />}
            <p className={cn('max-w-md text-xs leading-relaxed', error ? 'text-danger' : 'text-text-tertiary')}>{text}</p>
        </div>
    );
}

function Bubble({ msg, prev, selfName }: { msg: KoishiSandboxMessage; prev?: KoishiSandboxMessage; selfName: string }) {
    const mine = msg.user !== 'koishi';
    const segments = useMemo(() => parseKoishiMessage(msg.content), [msg.content]);
    const showSender = !prev || prev.user !== msg.user;
    return (
        <div className={cn('flex items-start gap-2.5', mine && 'flex-row-reverse', showSender ? 'mt-3' : 'mt-1')}>
            {showSender ? (
                <span
                    className={cn(
                        'inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-full ring-1 ring-border-subtle',
                        mine ? 'bg-inset text-text-secondary' : 'bg-brand-soft text-brand',
                    )}
                >
                    {mine ? <User size={15} /> : <Bot size={15} />}
                </span>
            ) : (
                <span className="w-8 shrink-0" />
            )}
            <div className={cn('flex min-w-0 max-w-[min(34rem,78%)] flex-col', mine ? 'items-end' : 'items-start')}>
                {showSender && (
                    <span className="mb-1 px-1 text-2xs text-text-tertiary">{mine ? selfName : 'Koishi'}</span>
                )}
                <div
                    className={cn(
                        'rounded-lg px-3 py-2 text-[13.5px] leading-relaxed text-text',
                        mine ? 'rounded-tr-sm bg-brand-soft' : 'rounded-tl-sm border border-border-subtle bg-surface',
                    )}
                >
                    {segments.length === 0 ? (
                        <span className="text-text-tertiary">（空消息）</span>
                    ) : (
                        segments.map((s, i) => <Seg key={i} seg={s} mine={mine} />)
                    )}
                </div>
            </div>
        </div>
    );
}

function Seg({ seg, mine }: { seg: KoishiSegment; mine: boolean }) {
    switch (seg.kind) {
        case 'text':
            return <span className="whitespace-pre-wrap break-words">{seg.text}</span>;
        case 'at':
            return (
                <span className={cn('font-medium', mine ? 'text-brand' : 'text-info')}>@{seg.name || seg.id} </span>
            );
        case 'img':
            return (
                <img
                    src={seg.src}
                    alt="图片"
                    draggable={false}
                    className="my-0.5 block max-h-60 min-h-16 min-w-16 max-w-[15rem] rounded-md object-contain"
                />
            );
        case 'quote':
            return <span className="text-text-tertiary">[引用] </span>;
        default:
            return <span className="text-text-tertiary">[{seg.name}]</span>;
    }
}
