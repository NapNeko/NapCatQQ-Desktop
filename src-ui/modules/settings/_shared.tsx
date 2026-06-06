// 设置页各 Tab 共用的小件:FieldRow(左标题右控件行)+ ThemeSegment(主题三选段控件)+
// MotionLevelSegment(动画档位三选段)+ MotionSpeedSlider(动画速度滑块)+
// PerformanceMonitorIntervalSlider(性能监控采样间隔滑块)+
// SettingsTabSections / SettingsSection(分组；组间 divide-y，字段平铺)。

import type { ComponentType, ReactNode } from 'react';
import type { LucideProps } from 'lucide-react';
import { Sun, Moon, MonitorCog, Sparkles, Wand2, Feather } from 'lucide-react';
import { SegmentMotionIcon } from '../../shared/ui/motion';
import type { ThemeMode } from '../../hooks/preferences/preferencesStore';
import type { MotionLevel } from '../../core/design/motion';
import {
    MOTION_SPEED_DEFAULT,
    MOTION_SPEED_MAX,
    MOTION_SPEED_MIN,
} from '../../core/design/motion';
import {
    clampPerformanceMonitorIntervalMs,
    PERFORMANCE_MONITOR_INTERVAL_MS_DEFAULT,
    PERFORMANCE_MONITOR_INTERVAL_MS_MAX,
    PERFORMANCE_MONITOR_INTERVAL_MS_MIN,
} from '../../core/domain/performance/performanceSettings';

/** Tab 内多个分组：组间大留白，不用横线切（避免和行内分隔叠在一起显得乱）。 */
export function SettingsTabSections({ children }: { children: ReactNode }) {
    return <div className="flex w-full flex-col gap-14">{children}</div>;
}

/** Tab 内分组：子标题（带轻标记）+ 左侧引导线下的平铺字段。 */
export function SettingsSection({
    title,
    description,
    children,
}: {
    title: string;
    description?: ReactNode;
    children: ReactNode;
}) {
    return (
        <section className="space-y-4">
            <div className="space-y-1.5">
                <div className="flex items-center gap-2.5">
                    <span
                        className="h-3.5 w-0.5 shrink-0 rounded-full bg-brand/45"
                        aria-hidden
                    />
                    <h2 className="text-[13.5px] font-semibold leading-none tracking-tight text-text">
                        {title}
                    </h2>
                </div>
                {description && (
                    <p className="pl-3 text-[12px] leading-relaxed text-text-tertiary">
                        {description}
                    </p>
                )}
            </div>
            <div className="border-l border-border-subtle/80 pl-4 sm:pl-5">
                <div className="flex flex-col divide-y divide-border-subtle/70">
                    {children}
                </div>
            </div>
        </section>
    );
}

/// 标准设置行。组内行间分隔由 SettingsSection 内 divide-y 统一处理，勿再叠 border-b。
export function FieldRow({
    label,
    description,
    isLast: _isLast,
    children,
}: {
    label: string;
    description?: ReactNode;
    /** 保留以兼容调用方；组内最后一行由 divide-y 自然收尾，无需再传。 */
    isLast?: boolean;
    children?: ReactNode;
}) {
    return (
        <div className="flex items-center justify-between gap-6 py-5 first:pt-1 last:pb-1">
            <div className="min-w-0 flex-1 space-y-1">
                <label className="block text-[13px] font-medium leading-snug text-text">
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
        icon: ComponentType<LucideProps>;
    }> = [
        { value: 'auto', label: '系统', icon: MonitorCog },
        { value: 'light', label: '浅色', icon: Sun },
        { value: 'dark', label: '暗色', icon: Moon },
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
                    <SegmentMotionIcon
                        icon={it.icon}
                        selected={value === it.value}
                        segmentKey={`theme-${it.value}`}
                    />
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
                    <SegmentMotionIcon
                        icon={it.icon}
                        selected={value === it.value}
                        segmentKey={`motion-level-${it.value}`}
                    />
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

/** 性能监控采样间隔：滑块为主，避免受控数字框在输入过程中被 clamp 打断。 */
export function PerformanceMonitorIntervalSlider({
    value,
    onChange,
    disabled,
}: {
    value: number;
    onChange: (next: number) => void;
    disabled?: boolean;
}) {
    const clamped = clampPerformanceMonitorIntervalMs(value);
    return (
        <div className="flex items-center gap-2">
            <input
                type="range"
                min={PERFORMANCE_MONITOR_INTERVAL_MS_MIN}
                max={PERFORMANCE_MONITOR_INTERVAL_MS_MAX}
                step={100}
                value={clamped}
                disabled={disabled}
                onChange={(e) =>
                    onChange(clampPerformanceMonitorIntervalMs(Number(e.target.value)))
                }
                className={
                    'h-1.5 w-36 cursor-pointer appearance-none rounded-pill bg-inset outline-none ' +
                    'accent-brand ' +
                    'disabled:pointer-events-none disabled:opacity-50'
                }
            />
            <span className="w-14 text-right font-mono text-[11.5px] tabular-nums text-text-tertiary">
                {clamped} ms
            </span>
            <button
                type="button"
                onClick={() =>
                    onChange(PERFORMANCE_MONITOR_INTERVAL_MS_DEFAULT)
                }
                disabled={
                    disabled || clamped === PERFORMANCE_MONITOR_INTERVAL_MS_DEFAULT
                }
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
