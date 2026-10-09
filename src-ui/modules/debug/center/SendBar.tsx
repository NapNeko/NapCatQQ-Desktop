// 请求头里的发送控制与参数问题浮层。
import { memo, useEffect, useRef, useState } from 'react';
import { AlertCircle, AlertTriangle, Send, Square } from 'lucide-react';
import { Button, Popover, PopoverContent, PopoverTrigger, Spinner } from '../../../shared/ui';
import { cn } from '../../../shared/utils/cn';
import { useMotion } from '../../../hooks/preferences/useMotion';
import { useNowMs } from '../../../hooks/ui/useNowMs';
import { progressText } from '../../../core/domain/debug/streamActions';
import type { ParamIssue } from '../../../core/domain/debug/validate';
import type { DebugActionSafety } from '../../../core/ipc/generated/debug/DebugActionSafety';
import type { DebugStreamProgress } from '../../../core/ipc/generated/debug/DebugStreamProgress';
import { MOD_KEY_LABEL } from '../TopBar';
import { issueRoot } from './ParamsForm';
import { IconTip, Kbd } from './centerParts';
import { elapsedText } from './viewHelpers';

export interface SendBarProps {
    blocker: string | null;
    issues: readonly ParamIssue[];
    safety: DebugActionSafety | null;
    inflightSince: number | null;
    progress: DebugStreamProgress | null;
    onSend: () => void;
    onCancel: () => void;
    blockedNonce: number;
}

export const SendBar = memo(function SendBar({
    blocker,
    issues,
    safety,
    inflightSince,
    progress,
    onSend,
    onCancel,
    blockedNonce,
}: SendBarProps) {
    const m = useMotion();
    const reasonRef = useRef<HTMLSpanElement>(null);
    const inflight = inflightSince !== null;
    const now = useNowMs(inflight, 100);
    const danger = safety === 'dangerous';
    const hasIssues = issues.length > 0 && !blocker;
    const [blockedSay, setBlockedSay] = useState('');
    useEffect(() => {
        if (blockedNonce > 0 && reasonRef.current) m.shake(reasonRef.current);
        if (blockedNonce > 0 && blocker)
            setBlockedSay(`发不了：${blocker}${blockedNonce % 2 === 0 ? '\u00a0' : ''}`);
        // 只在再次尝试发送时播报，输入过程不重复读错误。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [blockedNonce]);
    useEffect(() => setBlockedSay(''), [blocker]);

    return (
        <div className="flex min-w-0 flex-col items-end gap-1">
            <span className="sr-only" role="status">
                {inflight ? '已发送，正在等回包' : blockedSay}
            </span>
            {inflight ? (
                <Button
                    size="sm"
                    variant="secondary"
                    onClick={onCancel}
                    className="h-7 gap-1.5 px-2"
                >
                    <Square size={11} aria-hidden />
                    取消
                    <Kbd>Esc</Kbd>
                </Button>
            ) : (
                <Button
                    size="sm"
                    variant={hasIssues ? 'secondary' : danger ? 'danger' : 'primary'}
                    disabled={!!blocker}
                    onClick={onSend}
                    className="h-7 gap-1.5 px-2"
                    title={`发送（${MOD_KEY_LABEL}+Enter）`}
                    aria-keyshortcuts="Control+Enter Meta+Enter"
                >
                    {danger ? (
                        <AlertTriangle size={12} aria-hidden />
                    ) : (
                        <Send size={12} aria-hidden />
                    )}
                    {hasIssues ? '仍然发送' : '发送'}
                </Button>
            )}
            <span
                ref={reasonRef}
                className="max-w-40 text-right text-2xs leading-snug @min-[640px]:max-w-56"
            >
                {inflight ? (
                    <span
                        className="inline-flex items-center gap-1.5 text-text-tertiary"
                        title="取消只是不再等回包，上游可能已经执行了"
                    >
                        <Spinner size="xs" label="正在等回包" />
                        {progress ? progressText(progress) : elapsedText(now - inflightSince)}
                    </span>
                ) : blocker ? (
                    <span className="text-warning">{blocker}</span>
                ) : null}
            </span>
        </div>
    );
});

export function ParamIssuesButton({
    issues,
    onJumpToIssue,
}: {
    issues: readonly ParamIssue[];
    onJumpToIssue: (name: string) => void;
}) {
    const [open, setOpen] = useState(false);
    const jumpingRef = useRef(false);
    if (!issues.length) return null;
    return (
        <Popover open={open} onOpenChange={setOpen}>
            <PopoverTrigger asChild>
                <IconTip
                    icon={AlertCircle}
                    label={`${issues.length} 处参数有问题`}
                    tone="danger"
                    className="text-danger"
                />
            </PopoverTrigger>
            <PopoverContent
                align="start"
                className="w-80 p-2"
                onCloseAutoFocus={(event) => {
                    // 跳转交给字段聚焦；浮层退出后不能把焦点抢回感叹号。
                    if (!jumpingRef.current) return;
                    event.preventDefault();
                    jumpingRef.current = false;
                }}
            >
                <p className="px-2 pb-2 pt-1 text-xs font-medium text-danger">
                    {issues.length} 处参数有问题
                </p>
                <div className="max-h-64 overflow-y-auto">
                    {issues.map((issue, i) => (
                        <button
                            key={`${issue.path}-${i}`}
                            type="button"
                            onClick={() => {
                                jumpingRef.current = true;
                                setOpen(false);
                                onJumpToIssue(issueRoot(issue.path));
                            }}
                            className={cn(
                                'flex w-full min-w-0 items-start gap-2 rounded-sm px-2 py-1.5 text-left text-xs hover:bg-inset',
                                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                            )}
                        >
                            <code className="min-w-0 flex-1 break-all font-mono text-text">
                                {issue.path}
                            </code>
                            <span className="shrink-0 text-danger">{issue.message}</span>
                        </button>
                    ))}
                </div>
            </PopoverContent>
        </Popover>
    );
}
