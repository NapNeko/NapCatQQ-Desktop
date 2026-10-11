// 撤回占位和保留内容共用气泡，避免多媒体继续沿用透明底。
import { useId, useMemo } from 'react';
import type { Message } from '../../core/domain/chat/model';
import { SegmentList, isMediaOnly, isPictureOnly } from '../../shared/chat/SegmentView';
import { useQQFaceLookup } from '../../hooks/chat/useChatQqFaces';
import { cn } from '../../shared/utils/cn';

export function ChatMessageContent({
    message,
    preventRecall = false,
}: {
    message: Message;
    preventRecall?: boolean;
}) {
    const recallDescription = useId();
    const peekFace = useQQFaceLookup();
    const segments = useMemo(
        () =>
            message.fileId
                ? message.segments.map((segment) =>
                      segment.type === 'file' && !segment.data.file_id
                          ? { ...segment, data: { ...segment.data, file_id: message.fileId } }
                          : segment,
                  )
                : message.segments,
        [message.segments, message.fileId],
    );
    const retained = message.recalled && preventRecall && message.segments.length > 0;
    const hidden = message.recalled && !retained;
    return (
        <div
            className={cn(
                'native-chat-bubble',
                !message.recalled && isMediaOnly(message.segments, peekFace) && 'is-media-only',
                !message.recalled &&
                    message.segments.length > 0 &&
                    message.segments.every((segment) => segment.type === 'file') &&
                    'is-media-only',
                !hidden && isPictureOnly(message.segments) && 'is-picture-only',
                hidden && 'is-recall-placeholder',
                retained && 'is-recall-retained',
            )}
            role={retained ? 'group' : undefined}
            aria-describedby={retained ? recallDescription : undefined}
            title={retained ? '消息已撤回，内容已保留' : undefined}
        >
            {hidden ? (
                <span>消息已撤回</span>
            ) : (
                <SegmentList segments={segments} mine={message.mine} messageId={message.id} />
            )}
            {retained && (
                <span id={recallDescription} className="sr-only">
                    消息已撤回，内容已保留
                </span>
            )}
        </div>
    );
}
