// 右下角悬浮三圆按钮(新增 / 刷新 / 进入批量模式)。
//
// 不用 GsapPresence 包整组：进场依赖 refReady + 初始 hidden，在 Tauri/WebView 里
// 容易一直卡在不可见。定位与远端页 FloatingAddButton 一致，仅多 BodyPortal 防裁切。

import { useEffect, useRef } from 'react';
import gsap from 'gsap';
import { Plus, RefreshCw, ListChecks } from 'lucide-react';
import { Tooltip, TooltipContent, TooltipTrigger } from '../../../../shared/ui';
import {
    MotionIcon,
    FAB_PRIMARY_MOTION,
    BATCH_MOTION,
    refreshMotion,
} from '../../../../shared/ui/motion';
import { cn } from '../../../../shared/utils/cn';
import { BodyPortal } from '../../../../shared/ui/BodyPortal';
import { useMotion } from '../../../../hooks/preferences/useMotion';

interface FloatingActionsProps {
    visible: boolean;
    onCreate: () => void;
    onRefresh: () => void;
    onEnterBatch: () => void;
    busy?: boolean;
}

export function FloatingActions({
    visible,
    onCreate,
    onRefresh,
    onEnterBatch,
    busy = false,
}: FloatingActionsProps) {
    const m = useMotion();
    const groupRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        if (!visible) return;
        const el = groupRef.current;
        if (!el || !m.enabled) {
            if (el) gsap.set(el, { opacity: 1, visibility: 'visible' });
            return;
        }
        gsap.fromTo(
            el,
            { opacity: 0, y: 12 },
            {
                opacity: 1,
                y: 0,
                duration: m.duration('base'),
                ease: m.ease.enter,
            },
        );
    }, [visible, m.enabled, m.level, m.speed, m.duration, m.ease.enter]);

    if (!visible) return null;

    return (
        <BodyPortal>
            <div
                ref={groupRef}
                className="pointer-events-none fixed bottom-6 right-6 z-[60] flex flex-col items-center gap-3"
                aria-label="Bot 列表快捷操作"
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
        </BodyPortal>
    );
}

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