// 通用 Tab。所有项改动进入统一草稿，右上角「保存设置」后落盘（后端 + localStorage）。

import {
    clampPerformanceMonitorIntervalMs,
    PERFORMANCE_MONITOR_INTERVAL_MS_MAX,
    PERFORMANCE_MONITOR_INTERVAL_MS_MIN,
} from '../../../core/domain/performance/performanceSettings';
import type { CloseAction } from '../../../hooks/preferences/preferencesStore';
import { NumberField, Select, Switch } from '../../../shared/ui';
import type { SettingsDraft } from '../settings-draft';
import {
    FieldRow,
    MotionLevelSegment,
    MotionSpeedSlider,
    ThemeSegment,
} from '../_shared';

interface Props {
    draft: SettingsDraft | null;
    patchDraft: (patch: Partial<SettingsDraft>) => void;
}

export function GeneralTab({ draft, patchDraft }: Props) {
    if (!draft) {
        return (
            <p className="text-[13px] text-text-tertiary">正在加载设置…</p>
        );
    }

    return (
        <>
            <FieldRow
                label="主题"
                description="保存后生效；编辑过程中界面不会预览切换"
            >
                <ThemeSegment
                    value={draft.theme}
                    onChange={(v) => patchDraft({ theme: v })}
                />
            </FieldRow>

            <FieldRow label="主页吉祥物" description="保存后更新概览页右上角猫娘">
                <Switch
                    checked={draft.showMascot}
                    onCheckedChange={(v) => patchDraft({ showMascot: v })}
                />
            </FieldRow>

            <FieldRow
                label="动画与体感"
                description="总开关。关闭后过渡退化为瞬时。系统「减少动画」仍会覆盖。"
            >
                <Switch
                    checked={draft.motionEnabled}
                    onCheckedChange={(v) => patchDraft({ motionEnabled: v })}
                />
            </FieldRow>

            <FieldRow
                label="动画档位"
                description="优雅 仅淡入淡出 · 标准 含轻 spring · 丰富 按钮弹性 + 卡片浮起等"
            >
                <MotionLevelSegment
                    value={draft.motionLevel}
                    onChange={(v) => patchDraft({ motionLevel: v })}
                    disabled={!draft.motionEnabled}
                />
            </FieldRow>

            <FieldRow label="动画速度" description="0.5x 更克制；1.5x 更利落。保存后全局动画生效">
                <MotionSpeedSlider
                    value={draft.motionSpeed}
                    onChange={(v) => patchDraft({ motionSpeed: v })}
                    disabled={!draft.motionEnabled}
                />
            </FieldRow>

            <FieldRow
                label="点击关闭按钮"
                description="关闭时隐藏到系统托盘；退出则结束程序（有本机 Bot 运行时会拦截退出）"
            >
                <Select
                    value={draft.closeAction}
                    onValueChange={(v) =>
                        patchDraft({ closeAction: v as CloseAction })
                    }
                    items={[
                        { value: 'close', label: '关闭程序' },
                        { value: 'tray', label: '最小化到托盘' },
                    ]}
                />
            </FieldRow>

            <FieldRow
                label="Bot 登录检查间隔"
                description="已登录状态下轮询 NapCat WebUI 的间隔；未登录时固定 1 秒。1000–60000 毫秒"
            >
                <BackendNumber
                    value={draft.botLoginCheckIntervalMs}
                    min={1000}
                    max={60000}
                    step={500}
                    onChange={(v) => patchDraft({ botLoginCheckIntervalMs: v })}
                    suffix="ms"
                />
            </FieldRow>

            <FieldRow
                label="主页性能监控"
                description="关闭并保存后概览不再显示 CPU / 内存曲线；开启后按采样间隔刷新"
            >
                <Switch
                    checked={draft.performanceMonitorEnabled}
                    onCheckedChange={(v) => patchDraft({ performanceMonitorEnabled: v })}
                />
            </FieldRow>

            <FieldRow
                label="性能监控采样间隔"
                description={`概览页 CPU / 内存曲线刷新间隔。${PERFORMANCE_MONITOR_INTERVAL_MS_MIN}–${PERFORMANCE_MONITOR_INTERVAL_MS_MAX} 毫秒，步进 100`}
                isLast
            >
                <BackendNumber
                    value={draft.performanceMonitorIntervalMs}
                    min={PERFORMANCE_MONITOR_INTERVAL_MS_MIN}
                    max={PERFORMANCE_MONITOR_INTERVAL_MS_MAX}
                    step={100}
                    onChange={(v) =>
                        patchDraft({
                            performanceMonitorIntervalMs: clampPerformanceMonitorIntervalMs(v),
                        })
                    }
                    suffix="ms"
                    disabled={!draft.performanceMonitorEnabled}
                />
            </FieldRow>
        </>
    );
}

function BackendNumber({
    value,
    min,
    max,
    step,
    suffix,
    disabled,
    onChange,
}: {
    value: number;
    min: number;
    max: number;
    step: number;
    suffix?: string;
    disabled?: boolean;
    onChange: (v: number) => void;
}) {
    return (
        <div className="flex items-center gap-1.5">
            <NumberField
                className="w-28"
                value={value}
                min={min}
                max={max}
                step={step}
                disabled={disabled}
                onValueChange={(n) => {
                    if (n === null) return;
                    onChange(Math.max(min, Math.min(max, Math.round(n))));
                }}
                onBlur={() => {
                    onChange(Math.max(min, Math.min(max, Math.round(value))));
                }}
            />
            {suffix && (
                <span className="text-[11.5px] text-text-tertiary">{suffix}</span>
            )}
        </div>
    );
}