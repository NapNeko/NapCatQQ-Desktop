// 右下角悬浮三圆按钮（刷新 / 导入 / 去组件页安装），形态对齐 Bot 列表。

import { useEffect, useRef } from 'react';
import gsap from 'gsap';
import { Boxes, Import, RefreshCw } from 'lucide-react';
import { Tooltip, TooltipContent, TooltipTrigger } from '../../../shared/ui';
import { FAB_PRIMARY_MOTION, MotionIcon, refreshMotion } from '../../../shared/ui/motion';
import { cn } from '../../../shared/utils/cn';
import { BodyPortal } from '../../../shared/ui/BodyPortal';
import { useMotion } from '../../../hooks/preferences/useMotion';

interface FloatingActionsProps {
    onInstall: () => void;
    onImport: () => void;
    onRefresh: () => void;
    importDisabled?: boolean;
    busy?: boolean;
    showInstall?: boolean;
}

export function FloatingActions({
    onInstall,
    onImport,
    onRefresh,
    importDisabled = false,
    busy = false,
    showInstall = true,
}: FloatingActionsProps) {
    const m = useMotion();
    const groupRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
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
    }, [m.enabled, m.level, m.speed, m.duration, m.ease.enter]);

    return (
        <BodyPortal>
            <div
                ref={groupRef}
                className="pointer-events-none fixed bottom-6 right-6 z-[60] flex flex-col items-center gap-3"
                aria-label="应用端列表快捷操作"
            >
                <CircleButton tooltip="刷新列表" onClick={onRefresh} disabled={busy} variant="ghost">
                    <MotionIcon
                        icon={RefreshCw}
                        motion={refreshMotion(busy)}
                        size={18}
                        strokeWidth={2.2}
                        playEnter={false}
                        hoverAccent
                    />
                </CircleButton>
                <CircleButton
                    tooltip="导入已有项目"
                    onClick={onImport}
                    disabled={busy || importDisabled}
                    variant="ghost"
                >
                    <MotionIcon
                        icon={Import}
                        motion="none"
                        size={18}
                        strokeWidth={2.2}
                        playEnter={false}
                        hoverAccent
                    />
                </CircleButton>
                {showInstall ? (
                    <CircleButton tooltip="去组件页安装" onClick={onInstall} disabled={busy} variant="primary">
                        <MotionIcon
                            icon={Boxes}
                            motion={FAB_PRIMARY_MOTION}
                            playEnter
                            enterKey="app-fab-install"
                            size={20}
                            strokeWidth={2.4}
                            hoverAccent
                        />
                    </CircleButton>
                ) : null}
            </div>
        </BodyPortal>
    );
}

function CircleButton({
    tooltip,
    onClick,
    disabled,
    variant,
    children,
}: {
    tooltip: string;
    onClick: () => void;
    disabled?: boolean;
    variant: 'primary' | 'ghost';
    children: React.ReactNode;
}) {
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
