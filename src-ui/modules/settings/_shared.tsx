// 设置页各 Tab 共用的小件:FieldRow(左标题右控件行)+ ThemeSegment(主题三选段控件)+
// MotionLevelSegment(动画档位三选段)+ MotionSpeedSlider(动画速度滑块)。

import type { ReactNode } from 'react';
import { Sun, Moon, MonitorCog, Sparkles, Wand2, Feather } from 'lucide-react';
import type { ThemeMode } from '../../hooks/preferences/preferencesStore';
import type { MotionLevel } from '../../core/design/motion';
import {
    MOTION_SPEED_DEFAULT,
    MOTION_SPEED_MAX,
    MOTION_SPEED_MIN,
} from '../../core/design/motion';

/// shadcn Settings recipe 的标准行：左 label/description 堆叠 + 右控件，
/// space-between 自动右对齐。行间用底部 border + padding 切，isLast 不画线。
export function FieldRow({
    label,
    description,
    isLast,
    children,
}: {
    label: string;
    description?: ReactNode;
    isLast?: boolean;
    children?: ReactNode;
}) {
    return (
        <div
            className={
                'flex items-center justify-between gap-6 ' +
                (isLast ? '' : 'border-b border-border-subtle pb-6')
            }
        >
            <div className="min-w-0 flex-1 space-y-1">
                <label className="block text-[13.5px] font-medium leading-none text-text">
                    {label}
                </label>
                {description && (
                    <p className="text-[12px] leading-relaxed text-text-tertiary">
                        {description}
                    </p>
                )}
            </div>
            {children && (
                <div className="flex shrink-0 items-center gap-2">{children}</div>
            )}
        </div>
    );
}

export function ThemeSegment({
    value,
    onChange,
}: {
    value: ThemeMode;
    onChange: (next: ThemeMode) => void;
}) {
    const items: ReadonlyArray<{
        value: ThemeMode;
        label: string;
        icon: ReactNode;
    }> = [
        { value: 'auto', label: '系统', icon: <MonitorCog size={13} /> },
        { value: 'light', label: '浅色', icon: <Sun size={13} /> },
        { value: 'dark', label: '暗色', icon: <Moon size={13} /> },
    ];
    return (
        <div className="flex h-7 items-center rounded-md bg-inset p-0.5">
            {items.map((it) => (
                <button
                    key={it.value}
                    type="button"
                    onClick={() => onChange(it.value)}
                    className={
                        'flex h-6 items-center gap-1 rounded-sm px-2.5 text-[12px] font-medium transition-colors ' +
                        (value === it.value
                            ? 'bg-surface text-text shadow-[0_1px_2px_rgba(0,0,0,0.04)]'
                            : 'text-text-tertiary hover:text-text')
                    }
                >
                    {it.icon}
                    <span>{it.label}</span>
                </button>
            ))}
        </div>
    );
}

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
        icon: ReactNode;
    }> = [
        { value: 'elegant', label: '优雅', icon: <Feather size={13} /> },
        { value: 'standard', label: '标准', icon: <Wand2 size={13} /> },
        { value: 'rich', label: '丰富', icon: <Sparkles size={13} /> },
    ];
    return (
        <div
            className={
                'flex h-7 items-center rounded-md bg-inset p-0.5 ' +
                (disabled ? 'pointer-events-none opacity-60' : '')
            }
        >
            {items.map((it) => (
                <button
                    key={it.value}
                    type="button"
                    onClick={() => onChange(it.value)}
                    disabled={disabled}
                    className={
                        'flex h-6 items-center gap-1 rounded-sm px-2.5 text-[12px] font-medium transition-colors ' +
                        (value === it.value
                            ? 'bg-surface text-text shadow-[0_1px_2px_rgba(0,0,0,0.04)]'
                            : 'text-text-tertiary hover:text-text')
                    }
                >
                    {it.icon}
                    <span>{it.label}</span>
                </button>
            ))}
        </div>
    );
}

/// 动画速度滑块。范围 [0.5, 1.5],步长 0.05,显示当前倍率。
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
                {value.toFixed(2)}x
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
