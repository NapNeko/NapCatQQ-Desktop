// 搜索结果里的消息内容渲染：图片/表情走时间线同款组件，文本段带关键词高亮。
import { type ReactNode } from 'react';
import { markdownContent } from '../../core/domain/debug/markdown';
import { segmentPreview, type Segment } from '../../core/domain/debug/segments';
import { projectMessageDisplay } from '../../core/domain/chat/messageDisplay';
import { splitHighlight } from '../../core/domain/chat/chatSearchModel';
import type { Message } from '../../core/domain/chat/model';
import { QQFace } from '../../shared/chat/media/QQFace';
import { SegmentList } from '../../shared/chat/SegmentView';
import { useQQFaceLookup } from '../../hooks/chat/useChatQqFaces';
import { SimpleMarkdown } from '../../shared/ui/SimpleMarkdown';

function SearchPicture({
    picture,
    messageId,
    imageIndex,
}: {
    picture: Segment;
    messageId?: string;
    imageIndex: number;
}) {
    return (
        <span className="native-chat-search-picture">
            <SegmentList
                segments={[picture]}
                mine={false}
                messageId={messageId}
                imageIndexOffset={imageIndex}
            />
        </span>
    );
}

function highlightPreview(text: string, pattern: RegExp): ReactNode[] {
    return splitHighlight(text, pattern).map((part) =>
        part.hit ? (
            <mark className="native-chat-search-highlight" key={part.start}>
                {part.text}
            </mark>
        ) : (
            part.text
        ),
    );
}

export function SearchContent({ message, pattern }: { message: Message; pattern: RegExp | null }) {
    const peekFace = useQQFaceLookup();
    let imageIndex = 0;
    return (
        <div className="native-chat-search-result-content">
            {projectMessageDisplay(message.segments, peekFace).map((segment, index) => {
                if (segment.type === 'image' || segment.type === 'mface')
                    return (
                        <SearchPicture
                            key={index}
                            picture={segment}
                            messageId={message.id}
                            imageIndex={imageIndex++}
                        />
                    );
                if (segment.type === 'face')
                    return (
                        <QQFace
                            key={index}
                            id={String(segment.data.id ?? '')}
                            data={segment.data}
                            displayLarge={segment.displayLarge}
                            animated
                            name={
                                typeof segment.data.name === 'string'
                                    ? segment.data.name
                                    : undefined
                            }
                            url={
                                typeof segment.data.url === 'string' ? segment.data.url : undefined
                            }
                        />
                    );
                if (segment.type === 'markdown')
                    return (
                        <SimpleMarkdown
                            key={index}
                            text={markdownContent(segment.data)}
                            emptyFallback="（空的 Markdown）"
                            className="native-chat-search-markdown"
                        />
                    );
                const preview = segmentPreview(segment);
                return (
                    <span key={index} className="native-chat-search-result-text">
                        {pattern ? highlightPreview(preview, pattern) : preview}
                    </span>
                );
            })}
        </div>
    );
}
