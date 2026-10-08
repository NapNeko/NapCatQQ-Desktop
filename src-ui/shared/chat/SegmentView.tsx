// 消息段渲染入口：按段类型分发给 segments/ 下的子件；不认识的段显示成 [类型] 小标签。
//
// 每个段单独包一层错误边界：上游给了奇怪的形状，只坏这一个段。
// 纯工具与子件在 segments/ 一类型一文件，本文件只做分发。

import { memo } from 'react';
import { Hand, Mic } from 'lucide-react';
import type { Segment } from '../../core/domain/debug/segments';
import { markdownContent } from '../../core/domain/debug/markdown';
import { useQQFaceLookup } from '../../hooks/chat/useChatQqFaces';
import { QQFace } from './media/QQFace';
import { ChatRecord } from './media/ChatRecord';
import { ChatVideo } from './media/ChatVideo';
import { SafeBoundary } from './rightParts';
import type { QQFaceDisplaySegment } from '../../core/domain/chat/qqFaces';
import { projectMessageDisplay } from '../../core/domain/chat/messageDisplay';
import { mediaUrlOf, str } from './segments/model';
import { TextSeg } from './segments/textSeg';
import { AtSeg } from './segments/atSeg';
import { ImageSeg, imageResourceKey } from './segments/imageSeg';
import { ReplyQuote } from './segments/replyQuote';
import { Chip, MediaChip } from './segments/chips';
import { FileCard } from './segments/fileCard';
import { ForwardCard } from './segments/forwardCard';
import { RichCard } from './segments/richCard';
import { MarkdownSeg } from './segments/markdownSeg';
import { UnknownChip } from './segments/unknownChip';

export { isMediaOnly, isPictureOnly } from './segments/model';

export const SegmentList = memo(function SegmentList({
    segments,
    mine,
    messageId,
    imageIndexOffset = 0,
}: {
    segments: Segment[];
    mine: boolean;
    messageId?: string;
    imageIndexOffset?: number;
}) {
    const peekFace = useQQFaceLookup();
    if (segments.length === 0) return <span className="text-text-tertiary">（空消息）</span>;
    // 回复段不管排在哪都画在最上面
    const reply = segments.find((s) => s.type === 'reply');
    const displayed = projectMessageDisplay(segments, peekFace);
    const rest = reply ? displayed.filter((s) => s !== reply) : displayed;
    let imageIndex = imageIndexOffset;
    return (
        <>
            {reply && (
                <SafeBoundary>
                    <ReplyQuote seg={reply} mine={mine} />
                </SafeBoundary>
            )}
            {rest.map((seg, i) => {
                const ordinal = imageIndex;
                if (seg.type === 'image' || seg.type === 'mface') imageIndex++;
                return (
                    <SafeBoundary
                        key={i}
                        fallback={<UnknownChip seg={seg} note="这个段显示不了" />}
                    >
                        <SegmentView
                            seg={seg}
                            mine={mine}
                            messageId={messageId}
                            imageIndex={ordinal}
                        />
                    </SafeBoundary>
                );
            })}
        </>
    );
});

function SegmentView({
    seg,
    mine,
    messageId,
    imageIndex,
}: {
    seg: QQFaceDisplaySegment;
    mine: boolean;
    messageId?: string;
    imageIndex: number;
}) {
    const d = seg.data;
    switch (seg.type) {
        case 'text':
            return <TextSeg text={str(d.text)} />;
        case 'at':
            return <AtSeg qq={str(d.qq)} name={str(d.name)} mine={mine} />;
        case 'face':
            return <QQFace id={str(d.id)} data={d} displayLarge={seg.displayLarge} animated />;
        case 'image':
            return (
                <ImageSeg
                    key={imageResourceKey(d)}
                    data={d}
                    context={{ messageId, imageIndex }}
                    summary={str(d.summary)}
                    sticker={Number(d.sub_type ?? d.subType) === 1}
                />
            );
        case 'mface':
            return (
                <ImageSeg
                    key={imageResourceKey(d)}
                    data={d}
                    context={{ messageId, imageIndex }}
                    summary={str(d.summary) || '[表情包]'}
                    sticker
                />
            );
        case 'record':
            return (
                <ChatRecord
                    data={d}
                    messageId={messageId}
                    fallback={
                        <MediaChip
                            icon={<Mic size={11} aria-hidden />}
                            label="语音"
                            url={mediaUrlOf(d)}
                        />
                    }
                />
            );
        case 'video':
            return <ChatVideo data={d} />;
        case 'file':
            return <FileCard data={d} />;
        case 'forward':
            return <ForwardCard data={d} />;
        case 'json':
        case 'xml':
            return <RichCard seg={seg} />;
        case 'markdown':
            return <MarkdownSeg content={markdownContent(d)} />;
        case 'poke':
            return <Chip icon={<Hand size={11} aria-hidden />}>戳了戳</Chip>;
        default:
            return <UnknownChip seg={seg} />;
    }
}
