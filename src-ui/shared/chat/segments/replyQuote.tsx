// 回复引用段：能定位到缓冲里的原消息就画成可点击的跳转。

import { cn } from '../../utils/cn';
import { messagePreview, type Segment } from '../../../core/domain/debug/segments';
import { useChatView } from '../chatContext';
import { str } from './model';

export function ReplyQuote({ seg, mine }: { seg: Segment; mine: boolean }) {
    const { findMessage, revealMessage } = useChatView();
    const id = Number(str(seg.data.id));
    const target = Number.isFinite(id) ? findMessage(id) : undefined;
    const text = target
        ? `${target.senderName}：${messagePreview(target.segments) || '（空消息）'}`
        : `回复 #${str(seg.data.id)}`;
    return (
        <button
            type="button"
            onClick={(e) => {
                e.stopPropagation();
                if (target) revealMessage(id);
            }}
            title={target ? '跳到被回复的消息' : '被回复的消息不在当前缓冲里'}
            className={cn(
                'mb-1 block w-fit max-w-full border-l-2 pl-2 text-left text-2xs leading-relaxed text-text-tertiary',
                mine ? 'border-brand/40' : 'border-border',
                target ? 'cursor-pointer hover:text-text-secondary' : 'cursor-default',
            )}
        >
            <span className="line-clamp-2 break-words">{text}</span>
        </button>
    );
}
