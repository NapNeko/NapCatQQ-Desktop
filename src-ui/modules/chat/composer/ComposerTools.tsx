// 工具栏只组合表情、文件、截图和提及入口，文件读取与草稿写入由各控制器负责。
import { useRef, type RefObject } from 'react';
import { AtSign, ImagePlus, Loader2, Paperclip, Smile } from 'lucide-react';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import type { useChatScreenshot } from '../../../hooks/chat/useChatScreenshot';
import { Button } from '../../../shared/ui/Button';
import { ActionMotionIcon } from '../../../shared/ui/motion/ActionMotionIcon';
import { Popover, PopoverContent, PopoverTrigger } from '../../../shared/ui/Popover';
import { ChatEmojiPicker } from '../media/ChatEmojiPicker';
import { ChatScreenshotTools } from '../ChatScreenshotTools';
import type { useComposerAttachments } from './useComposerAttachments';

export function ComposerTools({
    target,
    group,
    input,
    screenshot,
    attachments,
    collapsed,
    disabledReason,
    emoji,
    setEmoji,
    onMention,
}: {
    target: DebugTarget;
    group: boolean;
    input: RefObject<HTMLTextAreaElement>;
    screenshot: ReturnType<typeof useChatScreenshot>;
    attachments: ReturnType<typeof useComposerAttachments>;
    collapsed: boolean;
    disabledReason: string;
    emoji: boolean;
    setEmoji: (open: boolean) => void;
    onMention: () => void;
}) {
    const restoreAfterEmoji = useRef(false);
    return (
        <div className="native-chat-composer-tools">
            <Popover
                open={emoji && !collapsed}
                onOpenChange={(open) => {
                    if (open) restoreAfterEmoji.current = false;
                    setEmoji(open);
                }}
            >
                <PopoverTrigger asChild>
                    <Button
                        variant="ghost"
                        size="icon"
                        className="native-chat-icon"
                        aria-label="表情"
                        title="表情"
                    >
                        <Smile size={18} />
                    </Button>
                </PopoverTrigger>
                <PopoverContent
                    side="top"
                    align="start"
                    className="native-chat-popover w-auto p-2"
                    aria-label="选择表情"
                    onCloseAutoFocus={(e) => {
                        if (!collapsed && restoreAfterEmoji.current) {
                            e.preventDefault();
                            input.current?.focus();
                        }
                    }}
                >
                    <ChatEmojiPicker
                        target={target}
                        onSelect={(attachment) => {
                            if (!attachments.add(attachment)) return;
                            restoreAfterEmoji.current = true;
                            setEmoji(false);
                        }}
                        disabledReason={disabledReason}
                    />
                </PopoverContent>
            </Popover>
            <Button
                variant="ghost"
                size="icon"
                className="native-chat-icon"
                aria-label="添加图片"
                title="添加图片"
                disabled={!!attachments.pending}
                onClick={() => void attachments.pick('image')}
            >
                <ActionMotionIcon
                    icon={attachments.pending ? Loader2 : ImagePlus}
                    motion={attachments.pending ? 'spin' : 'none'}
                    size={18}
                />
            </Button>
            <ChatScreenshotTools screenshot={screenshot} collapsed={collapsed} />
            <Button
                variant="ghost"
                size="icon"
                className="native-chat-icon"
                aria-label="添加文件"
                title="添加文件"
                disabled={!!attachments.pending}
                onClick={() => void attachments.pick('file')}
            >
                <Paperclip size={18} />
            </Button>
            {group && (
                <Button
                    variant="ghost"
                    size="icon"
                    className="native-chat-icon"
                    aria-label="提及成员"
                    title="提及成员"
                    onClick={onMention}
                >
                    <AtSign size={18} />
                </Button>
            )}
        </div>
    );
}
