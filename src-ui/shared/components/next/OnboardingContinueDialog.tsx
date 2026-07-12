// 组件页遮罩结束后的衔接：体量对齐第一步 ChoiceBody（hero + 两选），细节留给机器人页遮罩。

import type { ComponentType } from 'react';
import type { LucideProps } from 'lucide-react';
import { ArrowRight, Bot, SkipForward } from 'lucide-react';
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogTitle,
} from '../../ui';
import { MotionIcon } from '../../ui/motion';
import { cn } from '../../utils/cn';

export interface OnboardingContinueDialogProps {
    open: boolean;
    submitting?: boolean;
    /** 继续：去机器人页看创建路径（遮罩） */
    onContinueBots: () => void;
    /** 结束引导，留在主界面 */
    onFinish: () => void;
}

export function OnboardingContinueDialog({
    open,
    submitting = false,
    onContinueBots,
    onFinish,
}: OnboardingContinueDialogProps) {
    return (
        <Dialog
            open={open}
            onOpenChange={() => {
                /* 门禁：忽略 Esc / 遮罩关闭，由按钮决定 */
            }}
        >
            <DialogContent
                size="onboarding"
                hideClose
                dismissOnOutsideClick={false}
                onEscapeKeyDown={(e) => e.preventDefault()}
            >
                <div className="flex min-h-0 flex-1 flex-col">
                    <div
                        className={cn(
                            'relative overflow-hidden border-b border-border-subtle/70',
                            'bg-[var(--surface-hero)] px-6 pb-5 pt-6 sm:px-8 sm:pt-7',
                        )}
                    >
                        <div
                            aria-hidden
                            className="pointer-events-none absolute -right-8 -top-10 h-40 w-40 rounded-full bg-brand/15 blur-2xl"
                        />
                        <div
                            aria-hidden
                            className="pointer-events-none absolute bottom-0 left-1/3 h-24 w-48 rounded-full bg-brand/10 blur-2xl"
                        />

                        <div className="relative min-w-0 max-w-xl">
                            <DialogTitle className="font-display text-[1.5rem] font-bold leading-snug text-[var(--text-hero-title)] sm:text-[1.65rem]">
                                组件这边告一段落
                            </DialogTitle>
                            <DialogDescription className="mt-2.5 text-[13px] leading-relaxed text-text-secondary">
                                框架和依赖已经认过路。接下来可以走一遍「添加 Bot」演示：会打开真实新建页并预填示例，点保存不会写入。也可以先自己逛。
                            </DialogDescription>
                        </div>
                    </div>

                    <div className="grid flex-1 grid-cols-1 gap-3 p-5 sm:grid-cols-2 sm:gap-3 sm:p-5">
                        <ChoiceCard
                            title="继续：演示创建 Bot"
                            description="打开新建页，逐步看身份、运行位置、连接和保存（不会真正添加）。"
                            icon={Bot}
                            disabled={submitting}
                            primary
                            onClick={onContinueBots}
                        />
                        <ChoiceCard
                            title="先自己逛"
                            description="结束引导。装好依赖后可自己点加号真实创建。设置 → 关于 可重新打开。"
                            icon={SkipForward}
                            disabled={submitting}
                            onClick={onFinish}
                        />
                    </div>
                </div>
            </DialogContent>
        </Dialog>
    );
}



function ChoiceCard({
    title,
    description,
    icon: Icon,
    disabled,
    primary,
    onClick,
}: {
    title: string;
    description: string;
    icon: ComponentType<LucideProps>;
    disabled?: boolean;
    primary?: boolean;
    onClick: () => void;
}) {
    return (
        <button
            type="button"
            disabled={disabled}
            onClick={onClick}
            className={cn(
                'group flex h-full flex-col items-start gap-2.5 rounded-lg border p-4 text-left',
                'transition-colors duration-150',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-2 focus-visible:ring-offset-elevated',
                'disabled:pointer-events-none disabled:opacity-50',
                primary
                    ? 'border-brand/40 bg-brand/[0.06] hover:border-brand hover:bg-brand/10'
                    : 'border-border-subtle bg-surface hover:border-border hover:bg-inset',
            )}
        >
            <span
                className={cn(
                    'inline-flex h-9 w-9 items-center justify-center rounded-md',
                    primary ? 'bg-brand/15 text-brand' : 'bg-inset text-text-secondary',
                )}
            >
                <MotionIcon
                    icon={Icon}
                    size={18}
                    strokeWidth={1.8}
                    playEnter={false}
                    className="text-inherit"
                />
            </span>
            <div className="min-w-0">
                <p className="text-[14px] font-semibold text-text">{title}</p>
                <p className="mt-1 text-[12.5px] leading-relaxed text-text-secondary">
                    {description}
                </p>
            </div>
            <span
                className={cn(
                    'mt-auto inline-flex items-center gap-1 text-[12px]',
                    primary ? 'text-brand' : 'text-text-tertiary',
                )}
            >
                {primary ? '继续' : '进入主界面'}
                <ArrowRight size={13} strokeWidth={2} />
            </span>
        </button>
    );
}

