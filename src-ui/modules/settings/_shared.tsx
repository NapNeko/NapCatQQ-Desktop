// 设置页各 Tab 共用的小件:FieldRow(左标题右控件行)+ ThemePicker(主题卡片网格)+
// MotionLevelSegment(动画档位三选段)+ MotionSpeedSlider(动画速度滑块)+
// PerformanceMonitorIntervalSlider(性能监控采样间隔滑块)+
// SettingsTabSections / SettingsSection(分组；组间 divide-y，字段平铺)。

import { useCallback, useEffect, useRef, useState, type ComponentType, type ReactNode } from 'react';
import { Popover, PopoverTrigger, PopoverContent } from '../../shared/ui';
import { useMotion } from '../../hooks/preferences/useMotion';
import type { LucideProps } from 'lucide-react';
import { Sparkles, Wand2, Feather, Square, Circle, RectangleHorizontal, ChevronDown } from 'lucide-react';
import { SegmentMotionIcon } from '../../shared/ui/motion';
import {
    GsapPresence,
    type EnterFn,
    type ExitFn,
} from '../../shared/ui/motion/GsapPresence';
import gsap from 'gsap';
import type { ThemeMode } from '../../hooks/preferences/preferencesStore';
import type { MotionLevel } from '../../core/design/motion';
import type { RadiusStyle } from '../../core/design/radius';
import {
    RADIUS_LABELS,
} from '../../core/design/radius';
import {
    MOTION_SPEED_DEFAULT,
    MOTION_SPEED_MAX,
    MOTION_SPEED_MIN,
    motionSpeedDisplayMultiplier,
} from '../../core/design/motion';
import {
    clampPerformanceMonitorIntervalMs,
    PERFORMANCE_MONITOR_INTERVAL_MS_DEFAULT,
    PERFORMANCE_MONITOR_INTERVAL_MS_MAX,
    PERFORMANCE_MONITOR_INTERVAL_MS_MIN,
} from '../../core/domain/performance/performanceSettings';
import {
    clampRemoteHostHealthProbeIntervalMs,
    REMOTE_HOST_HEALTH_PROBE_INTERVAL_MS_DEFAULT,
    REMOTE_HOST_HEALTH_PROBE_INTERVAL_MS_MAX,
    REMOTE_HOST_HEALTH_PROBE_INTERVAL_MS_MIN,
} from '../../core/domain/remote-host/healthProbeSettings';
import {
    clampTaskQueueCleanupSliderMs,
    TASK_QUEUE_CLEANUP_SLIDER_MAX,
    TASK_QUEUE_CLEANUP_SLIDER_MIN,
    TASK_QUEUE_CLEANUP_SLIDER_STEP,
} from '../../core/domain/task-queue/cleanup';
import {
    clampInfoBarDismissSliderMs,
    INFOBAR_DISMISS_SLIDER_MAX,
    INFOBAR_DISMISS_SLIDER_MIN,
    INFOBAR_DISMISS_SLIDER_STEP,
} from '../../core/domain/ui/infoBarDismiss';

/** Tab 内多个分组：组间大留白，不用横线切（避免和行内分隔叠在一起显得乱）。 */
export function SettingsTabSections({ children }: { children: ReactNode }) {
    return <div className="flex w-full flex-col gap-14">{children}</div>;
}

function SettingsSectionHeader({
    title,
    description,
}: {
    title: ReactNode;
    description?: ReactNode;
}) {
    return (
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
    );
}

/** Tab 内分组：子标题（带轻标记）+ 左侧引导线下的平铺字段。 */
export function SettingsSection({
    title,
    description,
    children,
    layout = 'fields',
}: {
    title: ReactNode;
    description?: ReactNode;
    children: ReactNode;
    /** fields：左引导线 + FieldRow 分隔；panel：全宽内容区（日志/大面板，勿套竖线）。 */
    layout?: 'fields' | 'panel';
}) {
    return (
        <section className="space-y-4">
            <SettingsSectionHeader title={title} description={description} />
            {layout === 'panel' ? (
                <div className="min-w-0">{children}</div>
            ) : (
                <div className="border-l border-border-subtle/80 pl-4 sm:pl-5">
                    <div className="flex flex-col divide-y divide-border-subtle/70">
                        {children}
                    </div>
                </div>
            )}
        </section>
    );
}

/// 标准设置行。组内行间分隔由 SettingsSection 内 divide-y 统一处理，勿再叠 border-b。
/// layout="inline"（默认）：左标签右控件；layout="stacked"：标签在上、内容全宽在下。
export function FieldRow({
    label,
    description,
    isLast: _isLast,
    layout = 'inline',
    children,
}: {
    label: string;
    description?: ReactNode;
    /** 保留以兼容调用方；组内最后一行由 divide-y 自然收尾，无需再传。 */
    isLast?: boolean;
    /** inline: 左标签右控件（默认）；stacked: 标签在上、内容全宽在下。 */
    layout?: 'inline' | 'stacked';
    children?: ReactNode;
}) {
    if (layout === 'stacked') {
        return (
            <div className="flex flex-col gap-2 py-5 first:pt-1 last:pb-1">
                <div className="space-y-1">
                    <label className="block text-[13px] font-medium leading-snug text-text">
                        {label}
                    </label>
                    {description && (
                        <p className="text-[12px] leading-relaxed text-text-tertiary">
                            {description}
                        </p>
                    )}
                </div>
                {children && <div>{children}</div>}
            </div>
        );
    }
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

/** 单个主题的预览色值。 */
interface ThemeItem {
    value: ThemeMode;
    label: string;
    canvas: string;
    sidebar: string;
    text: string;
    subtext: string;
    brand: string;
    accent: string;
}

/** 主题分组：组名 + 子项列表。未来新增主题或自定义主题只需追加 ThemeGroup。 */
interface ThemeGroup {
    label: string;
    items: ReadonlyArray<ThemeItem>;
}

const THEME_GROUPS: ReadonlyArray<ThemeGroup> = [
    {
        label: '基础',
        items: [
            {
                value: 'auto', label: '系统',
                canvas: '#faf7f2', sidebar: '#ffe3ee',
                text: '#2c1f18', subtext: '#8a7d76', brand: '#ff6b3d', accent: '#f58fb6',
            },
            {
                value: 'light', label: '浅色',
                canvas: '#faf7f2', sidebar: '#ffe3ee',
                text: '#2c1f18', subtext: '#8a7d76', brand: '#ff6b3d', accent: '#f58fb6',
            },
            {
                value: 'dark', label: '暗色',
                canvas: '#211f1d', sidebar: '#292725',
                text: '#f5f1ed', subtext: '#9e9890', brand: '#ff8a57', accent: '#f58fb6',
            },
        ],
    },
    {
        label: 'Catppuccin',
        items: [
            {
                value: 'latte', label: 'Latte',
                canvas: '#eff1f5', sidebar: '#e6e9ef',
                text: '#4c4f69', subtext: '#6c6f85', brand: '#8839ef', accent: '#1e66f5',
            },
            {
                value: 'frappe', label: 'Frappé',
                canvas: '#303446', sidebar: '#292c3c',
                text: '#c6d0f5', subtext: '#949cbb', brand: '#ca9ee6', accent: '#8caaee',
            },
            {
                value: 'macchiato', label: 'Macchiato',
                canvas: '#24273a', sidebar: '#1e2030',
                text: '#cad3f5', subtext: '#939ab7', brand: '#c6a0f6', accent: '#8aadf4',
            },
            {
                value: 'mocha', label: 'Mocha',
                canvas: '#1e1e2e', sidebar: '#181825',
                text: '#cdd6f4', subtext: '#9399b2', brand: '#cba6f7', accent: '#89b4fa',
            },
        ],
    },
];

/** 从 THEME_GROUPS 中查找指定主题的元数据。 */
function findThemeItem(value: ThemeMode): ThemeItem | undefined {
    for (const group of THEME_GROUPS) {
        const found = group.items.find((it) => it.value === value);
        if (found) return found;
    }
    return undefined;
}

/**
 * 主题选择器弹窗组件。
 * FieldRow 中展示紧凑触发按钮（色块 + 当前主题名），点击弹出 Popover，
 * 弹窗内展示分组主题卡片网格（原始 h-9 预览比例，grid-cols-7）。
 * 未来扩展只需往 THEME_GROUPS 追加 ThemeGroup。
 */
export function ThemePicker({
    value,
    onChange,
}: {
    value: ThemeMode;
    onChange: (next: ThemeMode) => void;
}) {
    const [open, setOpen] = useState(false);
    const current = findThemeItem(value);
    const m = useMotion();

    // 卡片按钮 ref 映射，给 bindHover/bindPress 挂事件监听
    const cardRefs = useRef<Map<string, HTMLButtonElement>>(new Map());
    const setCardRef = useCallback(
        (val: string) => (el: HTMLButtonElement | null) => {
            if (el) cardRefs.current.set(val, el);
            else cardRefs.current.delete(val);
        },
        [],
    );

    // 卡片 hover/press 交互
    useEffect(() => {
        const cleanups: Array<() => void> = [];
        cardRefs.current.forEach((el) => {
            cleanups.push(m.bindHover(el));
            cleanups.push(m.bindPress(el));
        });
        return () => cleanups.forEach((fn) => fn());
    }, [m.bindHover, m.bindPress, open]);

    return (
        <Popover open={open} onOpenChange={setOpen}>
            {/* 触发按钮 */}
            <PopoverTrigger asChild>
                <button
                    type="button"
                    className="flex h-7 items-center gap-2 rounded-md bg-inset px-2.5 text-[12px] font-medium text-text transition-colors hover:bg-muted/50"
                >
                    {current && (
                        <span
                            className="h-3.5 w-3.5 shrink-0 rounded-sm"
                            style={{
                                background: current.brand,
                                boxShadow: 'inset 0 0 0 0.5px rgba(128,128,128,0.15)',
                            }}
                        />
                    )}
                    <span>{current?.label ?? value}</span>
                    <ChevronDown className="h-3 w-3 text-text-tertiary" />
                </button>
            </PopoverTrigger>

            {/* 弹窗内容 — 用户通过点击外部 / Escape / 再点触发器关闭 */}
            <PopoverContent
                side="bottom"
                align="start"
                sideOffset={6}
            >
                <div className="flex flex-col gap-3">
                    {THEME_GROUPS.map((group) => (
                        <div key={group.label} className="space-y-1.5">
                            {/* 分组标签 */}
                            <span className="text-[11px] font-medium tracking-wide text-text-tertiary">
                                {group.label}
                            </span>
                            {/* 卡片网格：4 列基准，最多 4 个主题一组 */}
                            <div className="grid grid-cols-4 gap-1.5">
                                {group.items.map((item) => {
                                    const selected = value === item.value;
                                    return (
                                        <button
                                            key={item.value}
                                            ref={setCardRef(item.value)}
                                            type="button"
                                            onClick={() => onChange(item.value)}
                                            className={
                                                'relative flex flex-col items-stretch gap-1 rounded-md p-1 transition-colors ' +
                                                (selected
                                                    ? 'bg-surface'
                                                    : 'hover:bg-muted/40')
                                            }
                                            style={
                                                selected
                                                    ? { boxShadow: `inset 0 0 0 1px ${item.brand}44` }
                                                    : undefined
                                            }
                                        >
                                            {/* 缩略窗口预览 */}
                                            <div
                                                className="relative h-9 w-full overflow-hidden rounded-[3px]"
                                                style={{
                                                    background: item.canvas,
                                                    boxShadow: 'inset 0 0 0 0.5px rgba(128,128,128,0.1)',
                                                }}
                                            >
                                                <div
                                                    className="absolute inset-y-0 left-0 w-[30%]"
                                                    style={{ background: item.sidebar }}
                                                />
                                                <div className="absolute inset-y-0 right-0 left-[30%] flex flex-col justify-center gap-[3px] px-1.5">
                                                    <div className="h-[2.5px] w-[60%] rounded-full" style={{ background: item.text, opacity: 0.5 }} />
                                                    <div className="h-[2.5px] w-[40%] rounded-full" style={{ background: item.subtext, opacity: 0.4 }} />
                                                    <div
                                                        className="mt-[1px] h-[4px] w-[32%] rounded-full"
                                                        style={{ background: item.brand }}
                                                    />
                                                </div>
                                                <div
                                                    className="absolute right-1 top-1 h-[4px] w-[4px] rounded-full"
                                                    style={{ background: item.accent }}
                                                />
                                            </div>

                                            {/* 标签 — 字重固定避免选中时 font-semibold 撑宽 grid */}
                                            <span
                                                className={
                                                    'text-center text-[11px] font-semibold leading-tight ' +
                                                    (selected ? 'text-text' : 'text-text-tertiary')
                                                }
                                            >
                                                {item.label}
                                            </span>
                                        </button>
                                    );
                                })}
                            </div>
                        </div>
                    ))}
                </div>
            </PopoverContent>
        </Popover>
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

/** 远程主机健康探活间隔滑块（P1 主动探活）。范围 10s~5min，步进 1s。 */
export function RemoteHostHealthProbeIntervalSlider({
    value,
    onChange,
    disabled,
}: {
    value: number;
    onChange: (next: number) => void;
    disabled?: boolean;
}) {
    const clamped = clampRemoteHostHealthProbeIntervalMs(value);
    return (
        <div className="flex items-center gap-2">
            <input
                type="range"
                min={REMOTE_HOST_HEALTH_PROBE_INTERVAL_MS_MIN}
                max={REMOTE_HOST_HEALTH_PROBE_INTERVAL_MS_MAX}
                step={1000}
                value={clamped}
                disabled={disabled}
                onChange={(e) =>
                    onChange(clampRemoteHostHealthProbeIntervalMs(Number(e.target.value)))
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
                onClick={() => onChange(REMOTE_HOST_HEALTH_PROBE_INTERVAL_MS_DEFAULT)}
                disabled={disabled || clamped === REMOTE_HOST_HEALTH_PROBE_INTERVAL_MS_DEFAULT}
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

const infoBarDismissSliderEnter: EnterFn = (el, env) =>
    gsap.fromTo(
        el,
        { autoAlpha: 0, x: -12 },
        {
            autoAlpha: 1,
            x: 0,
            duration: env.duration('base'),
            ease: env.ease.enter,
            clearProps: 'transform',
        },
    );

const infoBarDismissSliderExit: ExitFn = (el, env) =>
    gsap.to(el, {
        autoAlpha: 0,
        x: -10,
        duration: env.duration('fast'),
        ease: env.ease.exit,
    });

/** 设置页 InfoBar 时长滑块：开关打开时自左淡入，关闭时淡出（跟 useMotion 档位）。 */
export function InfoBarDismissSliderPresence({
    visible,
    children,
}: {
    visible: boolean;
    children: ReactNode;
}) {
    return (
        <GsapPresence
            visible={visible}
            onEnter={infoBarDismissSliderEnter}
            onExit={infoBarDismissSliderExit}
        >
            <div className="min-w-0 overflow-hidden">{children}</div>
        </GsapPresence>
    );
}

/** InfoBar 非错误类自动关闭时长（1000–60000 ms，步进 100）。 */
export function InfoBarDismissDurationSlider({
    value,
    onChange,
    defaultMs,
    disabled,
}: {
    value: number;
    onChange: (next: number) => void;
    defaultMs: number;
    disabled?: boolean;
}) {
    const clamped = clampInfoBarDismissSliderMs(value);
    const def = clampInfoBarDismissSliderMs(defaultMs);
    return (
        <div className="flex items-center gap-2">
            <input
                type="range"
                min={INFOBAR_DISMISS_SLIDER_MIN}
                max={INFOBAR_DISMISS_SLIDER_MAX}
                step={INFOBAR_DISMISS_SLIDER_STEP}
                value={clamped}
                disabled={disabled}
                onChange={(e) =>
                    onChange(clampInfoBarDismissSliderMs(Number(e.target.value)))
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
                onClick={() => onChange(def)}
                disabled={disabled || clamped === def}
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

function formatTaskQueueCleanupMs(ms: number): string {
    if (ms >= 60_000 && ms % 60_000 === 0) {
        const m = ms / 60_000;
        return `${m} 分钟`;
    }
    if (ms >= 1000 && ms % 1000 === 0) {
        return `${ms / 1000} 秒`;
    }
    return `${ms} ms`;
}

/** 任务队列终态条目保留时长（3 秒–1 小时，步进 1 秒）。 */
export function TaskQueueCleanupDurationSlider({
    value,
    onChange,
    defaultMs,
    disabled,
}: {
    value: number;
    onChange: (next: number) => void;
    defaultMs: number;
    disabled?: boolean;
}) {
    const clamped = clampTaskQueueCleanupSliderMs(value);
    const def = clampTaskQueueCleanupSliderMs(defaultMs);
    return (
        <div className="flex items-center gap-2">
            <input
                type="range"
                min={TASK_QUEUE_CLEANUP_SLIDER_MIN}
                max={TASK_QUEUE_CLEANUP_SLIDER_MAX}
                step={TASK_QUEUE_CLEANUP_SLIDER_STEP}
                value={clamped}
                disabled={disabled}
                onChange={(e) =>
                    onChange(clampTaskQueueCleanupSliderMs(Number(e.target.value)))
                }
                className={
                    'h-1.5 w-36 cursor-pointer appearance-none rounded-pill bg-inset outline-none ' +
                    'accent-brand ' +
                    'disabled:pointer-events-none disabled:opacity-50'
                }
            />
            <span className="w-16 text-right font-mono text-[11.5px] tabular-nums text-text-tertiary">
                {formatTaskQueueCleanupMs(clamped)}
            </span>
            <button
                type="button"
                onClick={() => onChange(def)}
                disabled={disabled || clamped === def}
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

/// 圆角风格三选段。三档语义见 core/design/radius.ts:
///   方正 square  — 0.5× 克制直角
///   标准 standard — 1.0× 默认平衡
///   圆润 round   — 1.5× 饱满圆角
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
        icon: ComponentType<LucideProps>;
    }> = [
            { value: 'square', label: RADIUS_LABELS.square, icon: Square },
            { value: 'standard', label: RADIUS_LABELS.standard, icon: RectangleHorizontal },
            { value: 'round', label: RADIUS_LABELS.round, icon: Circle },
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
                        segmentKey={`radius-${it.value}`}
                    />
                    <span>{it.label}</span>
                </button>
            ))}
        </div>
    );
}
