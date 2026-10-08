// 各设置 Tab 共用的采样/时长滑块，外加 InfoBar 时长滑块的显隐动效。
// 滑块为主而非受控数字框：避免输入过程中被 clamp 打断。

import type { ReactNode } from 'react';
import { GsapPresence, type EnterFn, type ExitFn } from '../../../shared/ui/motion/GsapPresence';
import gsap from 'gsap';
import {
    clampPerformanceMonitorIntervalMs,
    PERFORMANCE_MONITOR_INTERVAL_MS_DEFAULT,
    PERFORMANCE_MONITOR_INTERVAL_MS_MAX,
    PERFORMANCE_MONITOR_INTERVAL_MS_MIN,
} from '../../../core/domain/performance/performanceSettings';
import {
    BOT_RUNTIME_METRICS_INTERVAL_MS_DEFAULT,
    BOT_RUNTIME_METRICS_INTERVAL_MS_MAX,
    BOT_RUNTIME_METRICS_INTERVAL_MS_MIN,
    clampBotRuntimeMetricsIntervalMs,
} from '../../../core/domain/bot/runtime-metrics-settings';
import {
    clampRemoteHostHealthProbeIntervalMs,
    REMOTE_HOST_HEALTH_PROBE_INTERVAL_MS_DEFAULT,
    REMOTE_HOST_HEALTH_PROBE_INTERVAL_MS_MAX,
    REMOTE_HOST_HEALTH_PROBE_INTERVAL_MS_MIN,
} from '../../../core/domain/remote-host/healthProbeSettings';
import {
    clampTaskQueueCleanupSliderMs,
    TASK_QUEUE_CLEANUP_SLIDER_MAX,
    TASK_QUEUE_CLEANUP_SLIDER_MIN,
    TASK_QUEUE_CLEANUP_SLIDER_STEP,
} from '../../../core/domain/task-queue/cleanup';
import {
    clampInfoBarDismissSliderMs,
    INFOBAR_DISMISS_SLIDER_MAX,
    INFOBAR_DISMISS_SLIDER_MIN,
    INFOBAR_DISMISS_SLIDER_STEP,
} from '../../../core/domain/ui/infoBarDismiss';

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
                onClick={() => onChange(PERFORMANCE_MONITOR_INTERVAL_MS_DEFAULT)}
                disabled={disabled || clamped === PERFORMANCE_MONITOR_INTERVAL_MS_DEFAULT}
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

/** Bot 指标采样间隔：展示用户更容易理解的秒，底层仍保存毫秒。 */
export function BotRuntimeMetricsIntervalSlider({
    value,
    onChange,
    disabled,
}: {
    value: number;
    onChange: (next: number) => void;
    disabled?: boolean;
}) {
    const clamped = clampBotRuntimeMetricsIntervalMs(value);
    const seconds = clamped / 1000;
    return (
        <div className="flex items-center gap-2">
            <input
                aria-label="实例指标采样间隔"
                type="range"
                min={BOT_RUNTIME_METRICS_INTERVAL_MS_MIN}
                max={BOT_RUNTIME_METRICS_INTERVAL_MS_MAX}
                step={500}
                value={clamped}
                disabled={disabled}
                onChange={(e) => onChange(clampBotRuntimeMetricsIntervalMs(Number(e.target.value)))}
                className={
                    'h-1.5 w-36 cursor-pointer appearance-none rounded-pill bg-inset outline-none ' +
                    'accent-brand focus-visible:ring-2 focus-visible:ring-brand/45 focus-visible:ring-offset-2 ' +
                    'disabled:pointer-events-none disabled:opacity-50'
                }
            />
            <span className="w-12 text-right font-mono text-[11.5px] tabular-nums text-text-tertiary">
                {Number.isInteger(seconds) ? seconds : seconds.toFixed(1)} 秒
            </span>
            <button
                type="button"
                onClick={() => onChange(BOT_RUNTIME_METRICS_INTERVAL_MS_DEFAULT)}
                disabled={disabled || clamped === BOT_RUNTIME_METRICS_INTERVAL_MS_DEFAULT}
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

/** 远程主机健康探活间隔滑块。范围 10s~5min，步进 1s。 */
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
                onChange={(e) => onChange(clampInfoBarDismissSliderMs(Number(e.target.value)))}
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
                onChange={(e) => onChange(clampTaskQueueCleanupSliderMs(Number(e.target.value)))}
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
