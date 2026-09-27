// 试聊页的零件：头像、消息气泡（文字 / 图 / 表情 / 语音 / @ / 引用 / 转发）、时间行、提示行、打字中、看大图。

import { useState } from 'react';
import { Bot, Paperclip, User } from 'lucide-react';
import { Dialog, DialogContent, DialogTitle } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import { chatTimeLabel, type MaiBotChatMessage, type MaiBotChatSegment } from '../../../../core/domain/apps/maibotChat';

/** 麦麦配了 QQ 号就拉 qlogo 头像（和人物页同一个源），拉不到或没配用图标 */
export const ChatAvatar: React.FC<{ bot: boolean; qq?: string; className?: string }> = ({ bot, qq, className }) => {
    const [failed, setFailed] = useState(false);
    const useQq = bot && !!qq && /^[1-9]\d{4,11}$/.test(qq) && !failed;
    return (
        <span
            className={cn(
                'inline-flex h-8 w-8 shrink-0 items-center justify-center overflow-hidden rounded-full ring-1 ring-border-subtle',
                bot ? 'bg-brand-soft text-brand' : 'bg-inset text-text-secondary',
                className,
            )}
        >
            {useQq ? (
                <img
                    src={`https://q.qlogo.cn/headimg_dl?dst_uin=${qq}&spec=100`}
                    alt=""
                    className="h-full w-full object-cover"
                    referrerPolicy="no-referrer"
                    draggable={false}
                    onError={() => setFailed(true)}
                />
            ) : bot ? (
                <Bot size={15} />
            ) : (
                <User size={15} />
            )}
        </span>
    );
};

const onlyPictures = (segs: readonly MaiBotChatSegment[]) =>
    segs.length > 0 && segs.every((s) => s.type === 'image' || s.type === 'emoji');

const Segment: React.FC<{ seg: MaiBotChatSegment; mine: boolean; onPreview: (src: string) => void }> = ({ seg, mine, onPreview }) => {
    switch (seg.type) {
        case 'text':
            return <span className="whitespace-pre-wrap break-words">{seg.text}</span>;
        case 'at':
            return <span className={cn('font-medium', mine ? 'text-brand' : 'text-info')}>@{seg.name} </span>;
        case 'reply':
            return (
                <span className="mb-1 block border-l-2 border-border pl-2 text-2xs leading-relaxed text-text-tertiary">
                    <span className="line-clamp-2">
                        {seg.sender ? `${seg.sender}：` : ''}
                        {seg.text || '一条消息'}
                    </span>
                </span>
            );
        case 'image':
        case 'emoji':
            return (
                <button
                    type="button"
                    onClick={() => onPreview(seg.src)}
                    className="my-0.5 block cursor-zoom-in overflow-hidden rounded-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40"
                >
                    {/* 气泡按内容收缩宽度，图要是没有自带尺寸（只有 viewBox 的 SVG）会被挤成 0：表情给定框，图给个下限 */}
                    <img
                        src={seg.src}
                        alt={seg.type === 'emoji' ? '表情' : '图片'}
                        draggable={false}
                        className={cn(
                            'block object-contain',
                            seg.type === 'emoji' ? 'h-24 w-24' : 'max-h-60 min-h-16 min-w-16 max-w-[15rem]',
                        )}
                    />
                </button>
            );
        case 'voice':
            return <audio controls src={seg.src} className="my-0.5 h-9 max-w-[15rem]" />;
        case 'file':
            return (
                <span className="inline-flex items-center gap-1 text-text-secondary">
                    <Paperclip size={12} />
                    {seg.name}
                </span>
            );
        case 'forward':
            return <span className="text-text-tertiary">[合并转发{seg.count > 0 ? ` · ${seg.count} 条` : ''}]</span>;
        case 'other':
            return <span className="text-text-tertiary">{seg.text}</span>;
    }
};

export const MessageRow: React.FC<{
    message: MaiBotChatMessage;
    showSender: boolean;
    botQq?: string;
    onPreview: (src: string) => void;
}> = ({ message: m, showSender, botQq, onPreview }) => {
    const mine = !m.fromBot;
    const bare = onlyPictures(m.segments);
    return (
        <div className={cn('flex items-start gap-2.5', mine && 'flex-row-reverse', showSender ? 'mt-3' : 'mt-1')}>
            {showSender ? <ChatAvatar bot={m.fromBot} qq={botQq} /> : <span className="w-8 shrink-0" />}
            <div className={cn('flex min-w-0 max-w-[min(34rem,78%)] flex-col', mine ? 'items-end' : 'items-start')}>
                {showSender && <span className="mb-1 px-1 text-2xs text-text-tertiary">{m.senderName || (mine ? '我' : '麦麦')}</span>}
                <div
                    className={cn(
                        'text-[13.5px] leading-relaxed text-text',
                        bare
                            ? 'flex flex-wrap gap-1'
                            : cn(
                                  'rounded-lg px-3 py-2',
                                  mine
                                      ? 'rounded-tr-sm bg-brand-soft'
                                      : 'rounded-tl-sm border border-border-subtle bg-surface',
                              ),
                    )}
                    title={chatTimeLabel(m.at)}
                >
                    {m.segments.length === 0 ? (
                        <span className="text-text-tertiary">（空消息）</span>
                    ) : (
                        m.segments.map((s, i) => <Segment key={i} seg={s} mine={mine} onPreview={onPreview} />)
                    )}
                </div>
            </div>
        </div>
    );
};

export const TimeRow: React.FC<{ at: number }> = ({ at }) => (
    <div className="mb-1 mt-5 text-center text-2xs text-text-tertiary">{chatTimeLabel(at)}</div>
);

export const NoticeRow: React.FC<{ text: string; error: boolean }> = ({ text, error }) => (
    <div className="my-3 flex justify-center">
        <span
            className={cn(
                'max-w-[80%] rounded-pill px-3 py-1 text-center text-2xs leading-relaxed',
                error ? 'bg-danger-soft text-danger' : 'bg-inset text-text-tertiary',
            )}
        >
            {text}
        </span>
    </div>
);

/** 麦麦在想怎么回：三个点轮流亮，只动透明度 */
export const TypingRow: React.FC<{ botName: string; botQq?: string }> = ({ botName, botQq }) => (
    <div className="mt-3 flex items-center gap-2.5" aria-live="polite">
        <ChatAvatar bot qq={botQq} />
        <span className="flex items-center gap-1 rounded-lg rounded-tl-sm border border-border-subtle bg-surface px-3 py-2.5">
            {[0, 1, 2].map((i) => (
                <span
                    key={i}
                    className="h-1.5 w-1.5 animate-pulse rounded-full bg-text-tertiary"
                    style={{ animationDelay: `${i * 200}ms` }}
                />
            ))}
            <span className="sr-only">{botName}正在输入</span>
        </span>
    </div>
);

export const ImagePreview: React.FC<{ src: string | null; onClose: () => void }> = ({ src, onClose }) => (
    <Dialog open={src !== null} onOpenChange={(o) => !o && onClose()}>
        <DialogContent size="lg">
            <DialogTitle className="sr-only">看大图</DialogTitle>
            {src && <img src={src} alt="" className="mx-auto max-h-[70vh] max-w-full rounded-md object-contain" />}
        </DialogContent>
    </Dialog>
);
