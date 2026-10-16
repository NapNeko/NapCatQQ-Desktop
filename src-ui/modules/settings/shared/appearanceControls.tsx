// 外观 Tab 的三选段与速度滑块：动画档位、动画速度、圆角风格、侧栏形态。

import type { ComponentType } from 'react';
import type { LucideProps } from 'lucide-react';
import { Sparkles, Wand2, Feather, PanelLeftDashed, RectangleVertical } from 'lucide-react';
import { SegmentMotionIcon } from '../../../shared/ui/motion';
import type { MotionLevel } from '../../../core/design/motion';
import type { RadiusStyle } from '../../../core/design/radius';
import { RADIUS_LABELS, RADIUS_PRESETS } from '../../../core/design/radius';
import type { SidebarStyle } from '../../../core/domain/settings/preferencesStore';
import {
    MOTION_SPEED_DEFAULT,
    MOTION_SPEED_MAX,
    MOTION_SPEED_MIN,
    motionSpeedDisplayMultiplier,
} from '../../../core/design/motion';

/// 动画档位三选段。三档语义见 core/design/motion.ts:
///   优雅 elegant - 仅 fade,无 spring 弹性
///   标准 standard - fade+slide+轻 spring(默认)
///   丰富 rich - 按钮 QQ 弹 + 卡片 hover lift + 状态点呼吸 + 数字 rolling
export function MotionLevelSegment({
    value,
    onChange,
    disabled,
}: {
    value: MotionLevel;
    onChange: (next: MotionLevel) => void;
    disabled?: boolean;
}) {
    const items: ReadonlyArray<{
        value: MotionLevel;
        label: string;
        icon: ComponentType<LucideProps>;
    }> = [
        { value: 'elegant', label: '优雅', icon: Feather },
        { value: 'standard', label: '标准', icon: Wand2 },
        { value: 'rich', label: '丰富', icon: Sparkles },
    ];
    return (
        <div
            className={
                'flex h-7 items-center rounded-md bg-inset p-0.5 ' +
                (disabled ? 'pointer-events-none opacity-60' : '')
            }
        >
            {items.map((it) => {
                const selected = value === it.value;
                return (
                    <button
                        key={it.value}
                        type="button"
                        onClick={() => onChange(it.value)}
                        disabled={disabled}
                        className={
                            'flex h-6 items-center gap-1 rounded-sm px-2.5 text-[12px] font-medium transition-all ' +
                            (selected
                                ? 'border border-border/50 bg-surface text-text shadow-sm'
                                : 'border border-transparent text-text-tertiary hover:text-text')
                        }
                    >
                        <SegmentMotionIcon
                            icon={it.icon}
                            selected={selected}
                            segmentKey={`motion-level-${it.value}`}
                        />
                        <span>{it.label}</span>
                    </button>
                );
            })}
        </div>
    );
}

/// 动画速度滑块。内部 [0.5, 1.5]；展示倍率以 0.5 为 1.00× 基准。
/// 不引入 Radix Slider(避免增加依赖),用原生 input[type=range] + Tailwind 美化。
export function MotionSpeedSlider({
    value,
    onChange,
    disabled,
}: {
    value: number;
    onChange: (next: number) => void;
    disabled?: boolean;
}) {
    return (
        <div className="flex items-center gap-2">
            <input
                type="range"
                min={MOTION_SPEED_MIN}
                max={MOTION_SPEED_MAX}
                step={0.05}
                value={value}
                disabled={disabled}
                onChange={(e) => onChange(parseFloat(e.target.value))}
                className={
                    'h-1.5 w-32 cursor-pointer appearance-none rounded-pill bg-inset outline-none ' +
                    'accent-brand ' +
                    'disabled:pointer-events-none disabled:opacity-50'
                }
            />
            <span className="w-10 text-right font-mono text-[11.5px] tabular-nums text-text-tertiary">
                {motionSpeedDisplayMultiplier(value).toFixed(2)}x
            </span>
            <button
                type="button"
                onClick={() => onChange(MOTION_SPEED_DEFAULT)}
                disabled={disabled || value === MOTION_SPEED_DEFAULT}
                className={
                    'rounded-sm px-1.5 py-0.5 text-[11px] text-text-tertiary transition-colors ' +
                    'hover:bg-inset hover:text-text disabled:pointer-events-none disabled:opacity-40'
                }
            >
                重置
            </button>
        </div>
    );
}

export function RadiusStyleSegment({
    value,
    onChange,
}: {
    value: RadiusStyle;
    onChange: (next: RadiusStyle) => void;
}) {
    const items: ReadonlyArray<{
        value: RadiusStyle;
        label: string;
    }> = [
        { value: 'square', label: RADIUS_LABELS.square },
        { value: 'standard', label: RADIUS_LABELS.standard },
        { value: 'round', label: RADIUS_LABELS.round },
    ];
    return (
        <div className="flex h-7 items-center rounded-md bg-inset p-0.5">
            {items.map((it) => {
                const selected = value === it.value;
                return (
                    <button
                        key={it.value}
                        type="button"
                        aria-pressed={selected}
                        onClick={() => onChange(it.value)}
                        className={
                            'flex h-6 items-center gap-1 rounded-sm px-2.5 text-[12px] font-medium transition-all ' +
                            (selected
                                ? 'border border-border/50 bg-surface text-text shadow-sm'
                                : 'border border-transparent text-text-tertiary hover:text-text')
                        }
                    >
                        <span
                            aria-hidden="true"
                            className="h-3.5 w-3.5 shrink-0 border-[1.5px] border-current"
                            style={{ borderRadius: RADIUS_PRESETS[it.value].sm }}
                        />
                        <span>{it.label}</span>
                    </button>
                );
            })}
        </div>
    );
}

const SIDEBAR_LABELS: Record<SidebarStyle, string> = {
    classic: '经典',
    floating: '悬浮',
};

export function SidebarStyleSegment({
    value,
    onChange,
}: {
    value: SidebarStyle;
    onChange: (next: SidebarStyle) => void;
}) {
    const items: ReadonlyArray<{
        value: SidebarStyle;
        label: string;
        icon: ComponentType<LucideProps>;
    }> = [
        { value: 'classic', label: SIDEBAR_LABELS.classic, icon: PanelLeftDashed },
        { value: 'floating', label: SIDEBAR_LABELS.floating, icon: RectangleVertical },
    ];
    return (
        <div className="flex h-7 items-center rounded-md bg-inset p-0.5">
            {items.map((it) => {
                const selected = value === it.value;
                return (
                    <button
                        key={it.value}
                        type="button"
                        onClick={() => onChange(it.value)}
                        className={
                            'flex h-6 items-center gap-1 rounded-sm px-2.5 text-[12px] font-medium transition-all ' +
                            (selected
                                ? 'border border-border/50 bg-surface text-text shadow-sm'
                                : 'border border-transparent text-text-tertiary hover:text-text')
                        }
                    >
                        <SegmentMotionIcon
                            icon={it.icon}
                            selected={selected}
                            segmentKey={`sidebar-${it.value}`}
                        />
                        <span>{it.label}</span>
                    </button>
                );
            })}
        </div>
    );
}
