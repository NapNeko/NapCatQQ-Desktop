// 合并转发段：有 readForward 就展开成嵌套时间线，否则降级为可复制 id 的小卡。
// SegmentList 从主文件回环导入：只在渲染时用到，模块初始化不触碰。

import { Check, Copy, Forward } from 'lucide-react';
import { useCopy } from '../rightParts';
import { ChatForward } from '../media/ChatForward';
import { useChatView } from '../chatContext';
import { SegmentList } from '../SegmentView';
import { str } from './model';
import { Card } from './card';

export function ForwardCard({ data }: { data: Record<string, unknown> }) {
    const { readForward } = useChatView();
    const { copied, copy } = useCopy();
    const id = str(data.id);
    const count = Array.isArray(data.messages)
        ? data.messages.length
        : Array.isArray(data.content)
          ? data.content.length
          : 0;
    if (readForward)
        return (
            <ChatForward
                data={data}
                read={readForward}
                renderSegments={(segments) => <SegmentList mine={false} segments={segments} />}
            />
        );
    return (
        <Card
            icon={<Forward size={16} aria-hidden />}
            title="聊天记录"
            sub={count > 0 ? `${count} 条消息` : id ? `id ${id}` : undefined}
        >
            {id && (
                <button
                    type="button"
                    onClick={(e) => {
                        e.stopPropagation();
                        copy(id);
                    }}
                    className="mt-0.5 inline-flex w-fit items-center gap-1 text-2xs text-text-tertiary hover:text-text"
                >
                    {copied ? <Check size={10} aria-hidden /> : <Copy size={10} aria-hidden />}
                    {copied ? '已复制' : '复制 id（get_forward_msg 用）'}
                </button>
            )}
        </Card>
    );
}
