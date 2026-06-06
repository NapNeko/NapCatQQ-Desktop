// 图标按钮 / 工具栏 / 卡片操作区：统一尺寸与可选持续动效。

import type { ComponentType } from 'react';
import type { LucideProps } from 'lucide-react';
import { MotionIcon, type MotionIconPreset } from './MotionIcon';
import { cn } from '../../utils/cn';

export interface ActionMotionIconProps extends Omit<LucideProps, 'ref'> {
    icon: ComponentType<LucideProps>;
    motion?: MotionIconPreset;
    playEnter?: boolean;
    enterKey?: string;
    className?: string;
}

export function ActionMotionIcon({
    icon,
    motion = 'none',
    playEnter = false,
    enterKey,
    className,
    size = 16,
    strokeWidth = 2.2,
    ...rest
}: ActionMotionIconProps) {
    return (
        <MotionIcon
            icon={icon}
            motion={motion}
            playEnter={playEnter}
            enterKey={enterKey}
            size={size}
            strokeWidth={strokeWidth}
            className={cn('shrink-0', className)}
            {...rest}
        />
    );
}