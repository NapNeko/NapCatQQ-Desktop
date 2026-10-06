// 危险接口发送前的确认：用这次的参数写明后果（「会把 10001 移出群 100001」），附参数预览。
// 中栏发送和左栏收藏的一键发送共用这一个框，「本次不再询问」也是同一份记录：
// 勾过之后，不管从哪儿发同一个 Bot 上的同一个接口都不再问，到程序退出为止（记在模块里，页面卸载再回来也还在）。
// 默认焦点在「取消」上：确认危险操作要多按一下，回车不会误发。

import { useEffect, useRef, useState } from 'react';
import { AlertTriangle } from 'lucide-react';
import {
    Button,
    Checkbox,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from '../../shared/ui';
import { dangerConsequence } from '../../core/domain/debug/dangerCopy';

const skipped = new Set<string>();

const skipKey = (botId: string, action: string) => `${botId}:${action}`;

export function dangerConfirmSkipped(botId: string, action: string): boolean {
    return skipped.has(skipKey(botId, action));
}

/** 测试用 */
export function _resetDangerSkipsForTests(): void {
    skipped.clear();
}

function previewJson(v: unknown): string {
    try {
        return JSON.stringify(v, null, 2) ?? String(v);
    } catch {
        return String(v);
    }
}

export interface DangerConfirmDialogProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    botId: string;
    botName: string;
    action: string;
    /** 后果文案按哪个接口写：`delete_msg_async` 这类变体按原接口 `delete_msg` 写，不给就是 action 本身 */
    consequenceAction?: string;
    /**
     * 不按接口写后果、直接给一句原因。分级查不到（目录没读出来、目录里没有这个接口）时用：
     * 这时说不出具体后果，只能说清楚为什么要确认
     */
    reason?: string;
    /** 后果下面补一句来龙去脉，比如「这是收藏「群列表」」 */
    note?: string;
    params: Record<string, unknown>;
    onConfirm: () => void;
}

export function DangerConfirmDialog({ open, onOpenChange, ...body }: DangerConfirmDialogProps) {
    // 打开时挂上内容；关的动画期间内容还在，收起时不闪空，退场播完才真卸
    const [mounted, setMounted] = useState(open);
    if (open && !mounted) setMounted(true);
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent size="md" onExited={() => setMounted(false)}>
                {mounted && (
                    <ConfirmBody
                        {...body}
                        onCancel={() => onOpenChange(false)}
                        onConfirm={() => {
                            onOpenChange(false);
                            body.onConfirm();
                        }}
                    />
                )}
            </DialogContent>
        </Dialog>
    );
}

function ConfirmBody({
    botId,
    botName,
    action,
    consequenceAction,
    reason,
    note,
    params,
    onCancel,
    onConfirm,
}: Omit<DangerConfirmDialogProps, 'open' | 'onOpenChange'> & { onCancel: () => void }) {
    const [dontAsk, setDontAsk] = useState(false);
    const cancelRef = useRef<HTMLButtonElement>(null);
    const hasParams = Object.keys(params).length > 0;
    // Radix 默认把焦点给第一个可聚焦元素（勾选框）；打开后挪到「取消」上
    useEffect(() => {
        const frame = requestAnimationFrame(() => cancelRef.current?.focus());
        return () => cancelAnimationFrame(frame);
    }, []);
    return (
        <div>
            <DialogHeader>
                <DialogTitle className="flex items-center gap-2">
                    <span className="inline-flex h-7 w-7 items-center justify-center rounded-md bg-danger-soft text-danger">
                        <AlertTriangle size={15} strokeWidth={2.2} aria-hidden />
                    </span>
                    确认调用 <code className="font-mono">{action}</code>？
                </DialogTitle>
                <DialogDescription className="pt-1 text-[13.5px] font-medium text-danger">
                    {reason ?? dangerConsequence(consequenceAction ?? action, params, botName)}
                </DialogDescription>
                {note && <p className="text-xs text-text-secondary">{note}</p>}
            </DialogHeader>
            {hasParams ? (
                <pre className="max-h-44 overflow-auto rounded-sm border border-border-subtle bg-inset px-3 py-2 font-mono text-[11.5px] leading-relaxed text-text">
                    {previewJson(params)}
                </pre>
            ) : (
                <p className="text-xs text-text-tertiary">不带参数。</p>
            )}
            <Checkbox
                className="mt-3"
                checked={dontAsk}
                onCheckedChange={setDontAsk}
                label="本次不再询问（到程序退出）"
                hint={`只对 ${botName} 上的 ${action} 生效`}
            />
            <DialogFooter>
                <Button ref={cancelRef} variant="ghost" size="sm" onClick={onCancel}>
                    取消
                </Button>
                <Button
                    variant="danger"
                    size="sm"
                    onClick={() => {
                        if (dontAsk) skipped.add(skipKey(botId, action));
                        onConfirm();
                    }}
                >
                    确认调用
                </Button>
            </DialogFooter>
        </div>
    );
}
