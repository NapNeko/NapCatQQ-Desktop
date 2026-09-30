// 响应状态行：成功 / 失败、retcode（带人话提示）、耗时、走的哪条通道、回包大小。

import { memo } from 'react';
import { Badge, Tooltip, TooltipContent, TooltipTrigger } from '../../../shared/ui';
import { channelShortLabel } from '../../../core/domain/debug/channelCopy';
import { retcodeHint } from '../../../core/domain/debug/errorCopy';
import { formatBytes } from '../../../core/domain/debug/responseView';
import type { DebugCallOutcome } from '../../../core/ipc/generated/debug/DebugCallOutcome';

function elapsed(ms: number): string {
    return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(ms < 10_000 ? 2 : 1)} s`;
}

export const ResponseStatus = memo(function ResponseStatus({ outcome }: { outcome: DebugCallOutcome }) {
    const hint = retcodeHint(outcome.retcode);
    return (
        <div className="flex min-w-0 flex-wrap items-center gap-x-2.5 gap-y-1 text-xs">
            <Badge tone={outcome.ok ? 'success' : 'danger'} dot={outcome.ok}>
                {outcome.ok ? '成功' : '失败'}
            </Badge>
            {hint ? (
                <Tooltip>
                    <TooltipTrigger asChild>
                        <span tabIndex={0} className="cursor-help font-mono tabular-nums text-text-secondary underline decoration-dotted underline-offset-2">
                            retcode {outcome.retcode}
                        </span>
                    </TooltipTrigger>
                    <TooltipContent side="bottom" className="max-w-xs whitespace-normal">
                        {hint}
                    </TooltipContent>
                </Tooltip>
            ) : (
                <span className="font-mono tabular-nums text-text-secondary">retcode {outcome.retcode}</span>
            )}
            <span className="tabular-nums text-text-tertiary" title="从发出到收到回包">
                {elapsed(outcome.elapsed_ms)}
            </span>
            <span className="text-text-tertiary" title="这次走的通道">
                {channelShortLabel(outcome.channel)}
            </span>
            <span className="tabular-nums text-text-tertiary" title="回包大小">
                {formatBytes(outcome.size_bytes)}
            </span>
        </div>
    );
});
