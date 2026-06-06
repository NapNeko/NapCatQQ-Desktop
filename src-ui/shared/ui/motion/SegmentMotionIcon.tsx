// 设置页分段控件（主题 / 动效档位）内的小图标：选中时描边进场 + 轻呼吸。

import type { ComponentType } from 'react';
import type { LucideProps } from 'lucide-react';
import { MotionIcon } from './MotionIcon';
import { segmentMotion } from './motionIconSemantics';
import { cn } from '../../utils/cn';

export interface SegmentMotionIconProps {
    icon: ComponentType<LucideProps>;
    selected: boolean;
    segmentKey: string;
    size?: number;
    className?: string;
}

export function SegmentMotionIcon({
    icon,
    selected,
    segmentKey,
    size = 13,
    className,
}: SegmentMotionIconProps) {
    return (
        <MotionIcon
            icon={icon}
            motion={segmentMotion(selected)}
            playEnter={selected}
            enterKey={selected ? segmentKey : undefined}
            size={size}
            strokeWidth={1.75}
            className={cn(selected && 'text-brand', className)}
        />
    );
}