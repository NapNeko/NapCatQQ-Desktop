// 不认识的段：小标签 + 悬停看原始 JSON，段级错误边界的兜底也用它。

import { safeJson } from '../../../core/domain/debug/chatFormat';
import type { Segment } from '../../../core/domain/debug/segments';
import { Tooltip, TooltipContent, TooltipTrigger } from '../../ui/Tooltip';

export function UnknownChip({ seg, note }: { seg: Segment; note?: string }) {
    const json = safeJson(seg.data);
    return (
        <Tooltip>
            <TooltipTrigger asChild>
                <span
                    tabIndex={0}
                    className="mx-0.5 inline-flex cursor-help items-center rounded-xs border border-dashed border-border px-1.5 py-px align-[1px] font-mono text-[11px] text-text-tertiary"
                >
                    [{seg.type || '?'}]
                </span>
            </TooltipTrigger>
            <TooltipContent
                side="top"
                className="max-w-[320px] whitespace-pre-wrap break-all font-mono text-[11px]"
            >
                {note ? `${note}\n` : ''}
                {json.length > 1200 ? `${json.slice(0, 1200)}…` : json}
            </TooltipContent>
        </Tooltip>
    );
}
