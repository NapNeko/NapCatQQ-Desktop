// 试聊：在桌面端直接和麦麦说话。走麦麦 WebUI 的聊天室，记成一段 WebUI 私聊，不经过 QQ；
// 每次连上上游补最近 50 条。昵称就是麦麦称呼你的名字。

import { useLayoutEffect, useRef, useState } from 'react';
import { ArrowDown, Eraser, Pencil } from 'lucide-react';
import { Button, Spinner } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import type { AppInstance, MaiBotRuntimeStatus } from '../../../../core/ipc/types';
import { timelineMarks } from '../../../../core/domain/apps/maibotChat';
import { useMaiBotChat, type MaiBotChatState } from '../../../../hooks/apps/useMaiBotChat';
import { ConfirmDelete } from '../entityParts';
import { MaiBotLiveGate, maibotLive } from './MaiBotLiveGate';
import { ChatAvatar, ImagePreview, MessageRow, NoticeRow, TimeRow, TypingRow } from './maibotChatParts';
import { ChatComposer } from './maibotChatComposer';

// 离底部这么近算「在看最新的」：来新消息跟着滚，否则只亮「回到最新」
const STICK_PX = 120;

const StatusLine: React.FC<{ state: MaiBotChatState }> = ({ state }) => {
    const { status, reason } = state;
    if (status === 'ready') {
        return (
            <span className="flex items-center gap-1.5 text-2xs text-text-tertiary">
                <span className="h-1.5 w-1.5 rounded-full bg-success" />
                已连上
            </span>
        );
    }
    const text = status === 'reconnecting' ? '断了，正在重连' : status === 'closed' ? '已断开' : '正在连';
    return (
        <span className="flex min-w-0 items-center gap-1.5 text-2xs text-text-tertiary" title={reason}>
            <span className={cn('h-1.5 w-1.5 shrink-0 rounded-full', status === 'reconnecting' ? 'bg-warning' : 'bg-text-disabled')} />
            <span className="truncate">
                {text}
                {reason ? `：${reason}` : '…'}
            </span>
        </span>
    );
};

const NameEditor: React.FC<{ value: string; onChange: (next: string) => void }> = ({ value, onChange }) => {
    const [draft, setDraft] = useState<string | null>(null);
    if (draft === null) {
        return (
            <button
                type="button"
                title="麦麦会这么称呼你"
                onClick={() => setDraft(value)}
                className="inline-flex max-w-[12rem] items-center gap-1.5 rounded-sm px-2 py-1 text-xs text-text-secondary transition-colors hover:bg-inset focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40"
            >
                <span className="shrink-0">我叫</span>
                <span className="truncate font-medium text-text">{value}</span>
                <Pencil size={11} className="shrink-0 text-text-tertiary" />
            </button>
        );
    }
    const commit = () => {
        onChange(draft);
        setDraft(null);
    };
    return (
        <input
            autoFocus
            aria-label="你的昵称"
            value={draft}
            maxLength={32}
            onChange={(e) => setDraft(e.target.value)}
            onBlur={commit}
            onKeyDown={(e) => {
                if (e.key === 'Enter' && !e.nativeEvent.isComposing) commit();
                if (e.key === 'Escape') setDraft(null);
            }}
            className="h-7 w-36 rounded-sm border border-brand/50 bg-surface px-2 text-xs text-text outline-none"
        />
    );
};

export const MaiBotTryChatTab: React.FC<{
    instance: AppInstance;
    status: MaiBotRuntimeStatus | undefined;
    onStart: () => void;
    starting: boolean;
}> = ({ instance, status, onStart, starting }) => {
    const live = maibotLive(status);
    const { state, userName, send, rename, clear, clearing } = useMaiBotChat(instance.id, live);
    const [preview, setPreview] = useState<string | null>(null);
    const [confirmClear, setConfirmClear] = useState(false);
    const [behind, setBehind] = useState(false);
    const list = useRef<HTMLDivElement>(null);
    const stick = useRef(true);
    const everScrolled = useRef(false);

    const { items, typing, botName, botQq } = state;
    useLayoutEffect(() => {
        const el = list.current;
        if (!el) return;
        if (stick.current) {
            // 头一次铺历史直接到底，之后的新消息平滑滚过去
            el.scrollTo({ top: el.scrollHeight, behavior: everScrolled.current ? 'smooth' : 'auto' });
            everScrolled.current = items.length > 0;
        } else {
            setBehind(true);
        }
    }, [items, typing]);

    if (!live) return <MaiBotLiveGate status={status} what="试聊" onStart={onStart} starting={starting} />;

    const ready = state.status === 'ready';
    const messages = items.flatMap((i) => (i.kind === 'message' ? [i.message] : []));
    const marks = timelineMarks(messages);
    let mi = 0;

    return (
        <div className="flex min-h-0 flex-1 flex-col">
            <div className="flex items-center gap-3 pb-3">
                <ChatAvatar bot qq={botQq} className="h-9 w-9" />
                <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                    <span className="truncate text-sm font-medium text-text">{botName}</span>
                    <StatusLine state={state} />
                </div>
                <NameEditor value={userName} onChange={rename} />
                <Button
                    size="sm"
                    variant="ghost"
                    disabled={messages.length === 0 || clearing}
                    onClick={() => setConfirmClear(true)}
                >
                    <Eraser size={13} />
                    清空记录
                </Button>
            </div>

            <div className="relative min-h-0 flex-1 overflow-hidden rounded-lg border border-border-subtle bg-inset/40">
                <div
                    ref={list}
                    className="h-full overflow-y-auto px-4 pb-4"
                    onScroll={(e) => {
                        const el = e.currentTarget;
                        stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < STICK_PX;
                        if (stick.current) setBehind(false);
                    }}
                >
                    {items.length === 0 ? (
                        <div className="flex h-full flex-col items-center justify-center gap-2 text-center">
                            {ready ? (
                                <>
                                    <ChatAvatar bot qq={botQq} className="h-12 w-12" />
                                    <p className="text-sm text-text">和{botName}说句话试试</p>
                                    <p className="max-w-sm text-xs leading-relaxed text-text-tertiary">
                                        这里的对话记在麦麦的 WebUI 私聊里，不会发到 QQ 上。
                                    </p>
                                </>
                            ) : (
                                <Spinner size="md" tone="brand" label="正在连麦麦" />
                            )}
                        </div>
                    ) : (
                        <div className="pt-1">
                            {items.map((item) => {
                                if (item.kind === 'notice') return <NoticeRow key={item.key} text={item.text} error={item.error} />;
                                const mark = marks[mi++];
                                return (
                                    <div key={item.key}>
                                        {mark.showTime && <TimeRow at={item.message.at} />}
                                        <MessageRow
                                            message={item.message}
                                            showSender={mark.showSender}
                                            botQq={botQq}
                                            onPreview={setPreview}
                                        />
                                    </div>
                                );
                            })}
                            {typing && <TypingRow botName={botName} botQq={botQq} />}
                        </div>
                    )}
                </div>
                {behind && (
                    <button
                        type="button"
                        onClick={() => {
                            stick.current = true;
                            setBehind(false);
                            list.current?.scrollTo({ top: list.current.scrollHeight, behavior: 'smooth' });
                        }}
                        className="absolute bottom-3 left-1/2 inline-flex -translate-x-1/2 items-center gap-1 rounded-pill bg-elevated px-3 py-1 text-2xs text-text-secondary shadow-popover ring-1 ring-border-subtle hover:text-text"
                    >
                        <ArrowDown size={12} />
                        回到最新
                    </button>
                )}
            </div>

            <div className="pt-3">
                <ChatComposer
                    ready={ready}
                    placeholder={ready ? `和${botName}说点什么，Enter 发送，Shift + Enter 换行` : '还没连上麦麦…'}
                    onSend={(text, images) => {
                        stick.current = true;
                        return send(text, images);
                    }}
                />
            </div>

            <ImagePreview src={preview} onClose={() => setPreview(null)} />
            <ConfirmDelete
                open={confirmClear}
                title={`清空和${botName}的聊天记录？`}
                description="只清这里的对话。麦麦对你的印象、从对话里学到的东西不会跟着清。"
                confirmLabel="清空"
                busy={clearing}
                onCancel={() => setConfirmClear(false)}
                onConfirm={() => void clear().then(() => setConfirmClear(false))}
            />
        </div>
    );
};
