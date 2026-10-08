// Markdown 段：直接交给 SimpleMarkdown，折叠由外层气泡管。

import { SimpleMarkdown } from '../../ui/SimpleMarkdown';
import { useChatView } from '../chatContext';

export function MarkdownSeg({ content }: { content: string }) {
    const { openLink } = useChatView();
    return (
        // 不在这里另外截高：气泡的 12 行折叠 +「展开」管它
        <span className="my-0.5 block">
            <SimpleMarkdown
                text={content}
                emptyFallback="（空的 Markdown）"
                onOpenLink={openLink}
                className="text-[13px] leading-5 text-text"
            />
        </span>
    );
}
