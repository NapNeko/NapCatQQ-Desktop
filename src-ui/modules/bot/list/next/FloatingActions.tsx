// 右下角悬浮三圆按钮(新增 / 刷新 / 进入批量模式)。GSAP 版。
//
// 视觉:暖粉桃色调,新增按钮 brand primary,其它两个 ghost。
// 互斥:批量模式 visible=false,GsapPresence 跑 fly-out exit,父级 BatchBottomBar
// 跑 fly-in enter。
//
// 进退场:整组从右下角斜向滑入(x 24, y 24, autoAlpha 0 → 0/0/1),三个按钮
// stagger 错位 30ms 依次落位。

import { useEffect, useRef } from 'react';
import gsap from 'gsap';
import { Plus, RefreshCw, ListChecks } from 'lucide-react';
import { Tooltip, TooltipContent, TooltipTrigger } from '../../../../shared/ui';
import { GsapPresence, type EnterFn, type ExitFn } from '../../../../shared/ui/motion';
import { cn } from '../../../../shared/utils/cn';
import { useMotion } from '../../../../hooks/preferences/useMotion';

interface FloatingActionsProps {
    visible: boolean;
    onCreate: () => void;
    onRefresh: () => void;
    onEnterBatch: () => void;
    busy?: boolean;
}

const enter: EnterFn = (el, env) => {
    const tl = gsap.timeline();
    tl.fromTo(
        el,
        { autoAlpha: 0, x: 24, y: 24 },
        {
            autoAlpha: 1,
            x: 0,
            y: 0,
            duration: env.duration('base'),
            ease: env.preset.enterEase,
        },
    );
    // 三个圆按钮 stagger 进场
    const buttons = el.querySelectorAll('[data-circle-btn]');
    if (buttons.length > 0) {
        tl.from(
            buttons,
            {
                autoAlpha: 0,
                scale: 0.6,
                y: 8,
                duration: env.duration('fast'),
                ease: env.preset.bouncyEase,
                stagger: env.preset.stagger,
            },
            '<0.05',
        );
    }
    return tl;
};

const exit: ExitFn = (el, env) =>
    gsap.to(el, {
        autoAlpha: 0,
        x: 24,
        y: 24,
        duration: env.duration('fast'),
        ease: env.preset.exitEase,
    });

export function FloatingActions({
    visible,
    onCreate,
    onRefresh,
    onEnterBatch,
    busy = false,
}: FloatingActionsProps) {
    return (
        <GsapPresence visible={visible} onEnter={enter} onExit={exit}>
            <FloatingActionsBody
                onCreate={onCreate}
                onRefresh={onRefresh}
                onEnterBatch={onEnterBatch}
                busy={busy}
            />
        </GsapPresence>
    );
}

import { forwardRef } from 'react';

const FloatingActionsBody = forwardRef<
    HTMLDivElement,
    Omit<FloatingActionsProps, 'visible'>
>(({ onCreate, onRefresh, onEnterBatch, busy }, ref) => (
    <div
        ref={ref}
        style={{ visibility: 'hidden', opacity: 0 }}
        className="pointer-events-none fixed bottom-8 right-8 z-30 flex flex-col items-center gap-3"
    >
        <CircleButton
            tooltip="批量管理"
            onClick={onEnterBatch}
            disabled={busy}
            variant="ghost"
        >
            <ListChecks size={18} strokeWidth={2.2} />
        </CircleButton>
        <CircleButton
            tooltip="刷新列表"
            onClick={onRefresh}
            disabled={busy}
            variant="ghost"
        >
            <RefreshCw size={18} strokeWidth={2.2} />
        </CircleButton>
        <CircleButton
            tooltip="新增 Bot"
            onClick={onCreate}
            disabled={busy}
            variant="primary"
        >
            <Plus size={20} strokeWidth={2.4} />
        </CircleButton>
    </div>
));
FloatingActionsBody.displayName = 'FloatingActionsBody';

interface CircleButtonProps {
    tooltip: string;
    onClick: () => void;
    disabled?: boolean;
    variant: 'primary' | 'ghost';
    children: React.ReactNode;
}

function CircleButton({
    tooltip,
    onClick,
    disabled,
    variant,
    children,
}: CircleButtonProps) {
    const m = useMotion();
    const ref = useRef<HTMLButtonElement | null>(null);

    useEffect(() => {
        const el = ref.current;
        if (!el || !m.enabled) return;
        const onEnterBtn = () => {
            gsap.to(el, {
                scale: 1.08,
                duration: m.duration('fast'),
                ease: m.preset.hoverEase,
            });
        };
        const onLeaveBtn = () => {
            gsap.to(el, {
                scale: 1,
                duration: m.duration('fast'),
                ease: m.preset.hoverEase,
            });
        };
        const onDown = () => {
            gsap.to(el, {
                scale: 0.92,
                duration: m.duration('fast') * 0.6,
                ease: 'power2.out',
            });
        };
        const onUp = () => {
            gsap.to(el, {
                scale: 1.08,
                duration: m.duration('base'),
                ease: m.preset.bouncyEase,
            });
        };
        el.addEventListener('mouseenter', onEnterBtn);
        el.addEventListener('mouseleave', onLeaveBtn);
        el.addEventListener('mousedown', onDown);
        el.addEventListener('mouseup', onUp);
        return () => {
            el.removeEventListener('mouseenter', onEnterBtn);
            el.removeEventListener('mouseleave', onLeaveBtn);
            el.removeEventListener('mousedown', onDown);
            el.removeEventListener('mouseup', onUp);
        };
    }, [m.enabled, m.preset.hoverEase, m.preset.bouncyEase, m.speed]);

    return (
        <Tooltip>
            <TooltipTrigger asChild>
                <button
                    ref={ref}
                    type="button"
                    data-circle-btn
                    onClick={onClick}
                    disabled={disabled}
                    className={cn(
                        'pointer-events-auto inline-flex h-11 w-11 items-center justify-center rounded-full',
                        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                        'disabled:cursor-not-allowed disabled:opacity-50',
                        variant === 'primary'
                            ? 'bg-brand text-white shadow-popover hover:bg-brand-hover'
                            : 'bg-elevated text-text-secondary ring-1 ring-border-subtle shadow-card hover:bg-inset hover:text-text hover:shadow-popover',
                    )}
                >
                    {children}
                </button>
            </TooltipTrigger>
            <TooltipContent side="left">{tooltip}</TooltipContent>
        </Tooltip>
    );
}
