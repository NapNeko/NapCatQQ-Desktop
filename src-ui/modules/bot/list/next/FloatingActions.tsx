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
import {
    GsapPresence,
    MotionIcon,
    FAB_PRIMARY_MOTION,
    BATCH_MOTION,
    refreshMotion,
    type EnterFn,
    type ExitFn,
} from '../../../../shared/ui/motion';
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
            ease: env.ease.enter,
        },
    );
    // 三个圆按钮 stagger 进场,各自带 release 弹性(rich 档余震明显)。
    const buttons = el.querySelectorAll('[data-circle-btn]');
    if (buttons.length > 0) {
        tl.from(
            buttons,
            {
                autoAlpha: 0,
                scale: 0.6,
                y: 8,
                duration: env.duration('fast'),
                ease: env.ease.release,
                stagger: env.stagger(),
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
        ease: env.ease.exit,
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
            <MotionIcon
                icon={ListChecks}
                motion={BATCH_MOTION}
                size={18}
                strokeWidth={2.2}
                playEnter={false}
                hoverAccent
            />
        </CircleButton>
        <CircleButton
            tooltip="刷新列表"
            onClick={onRefresh}
            disabled={busy}
            variant="ghost"
        >
            <MotionIcon
                icon={RefreshCw}
                motion={refreshMotion(busy ?? false)}
                size={18}
                strokeWidth={2.2}
                playEnter={false}
                hoverAccent
            />
        </CircleButton>
        <CircleButton
            tooltip="新增 Bot"
            onClick={onCreate}
            disabled={busy}
            variant="primary"
        >
            <MotionIcon
                icon={Plus}
                motion={FAB_PRIMARY_MOTION}
                playEnter
                enterKey="fab-plus"
                size={20}
                strokeWidth={2.4}
                hoverAccent
            />
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

    // CircleButton 用更夸张的 hoverScale=1.08(默认按钮 1.04 上限),所以这里显式
    // 传 scale,不走 preset 默认。lift/shadow/brightness 也关掉,FAB 已经有自己的
    // shadow 层级,不需要 helper 再叠 boxShadow。
    useEffect(() => {
        const el = ref.current;
        if (!el || !m.enabled || disabled) return;
        const cleanups = [
            m.bindHover(el, { scale: 1.08, lift: null, shadow: false, brightness: false }),
            m.bindPress(el),
        ];
        return () => cleanups.forEach((fn) => fn());
    }, [m.enabled, m.level, m.speed, m.bindHover, m.bindPress, disabled]);

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
