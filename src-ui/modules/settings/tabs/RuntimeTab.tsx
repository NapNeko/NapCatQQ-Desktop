// 运行 Tab：Bot 轮询与概览性能监控（写入 app-settings）。

import {
    clampPerformanceMonitorIntervalMs,
    PERFORMANCE_MONITOR_INTERVAL_MS_MAX,
    PERFORMANCE_MONITOR_INTERVAL_MS_MIN,
} from '../../../core/domain/performance/performanceSettings';
import { NumberField, Switch } from '../../../shared/ui';
import type { SettingsDraft } from '../settings-draft';
import {
    FieldRow,
    PerformanceMonitorIntervalSlider,
    SettingsSection,
    SettingsTabSections,
} from '../_shared';

interface Props {
    draft: SettingsDraft | null;
    patchDraft: (patch: Partial<SettingsDraft>) => void;
}

export function RuntimeTab({ draft, patchDraft }: Props) {
    if (!draft) {
        return (
            <p className="text-[13px] text-text-tertiary">正在加载设置…</p>
        );
    }

    return (
        <SettingsTabSections>
            <SettingsSection title="Bot 与登录">
                <FieldRow
                    label="登录检查间隔"
                    description="已登录时轮询 NapCat WebUI；未登录时固定 1 秒。1000–60000 毫秒"
                    isLast
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
            </SettingsSection>

            <SettingsSection title="概览性能">
                <FieldRow
                    label="主页性能监控"
                    description="关闭并保存后概览不再显示 CPU / 内存曲线"
                >
                    <Switch
                        checked={draft.performanceMonitorEnabled}
                        onCheckedChange={(v) =>
                            patchDraft({ performanceMonitorEnabled: v })
                        }
                    />
                </FieldRow>

                <FieldRow
                    label="采样间隔"
                    description={`曲线刷新间隔。拖动滑块，${PERFORMANCE_MONITOR_INTERVAL_MS_MIN}–${PERFORMANCE_MONITOR_INTERVAL_MS_MAX} 毫秒`}
                    isLast
                >
                    <PerformanceMonitorIntervalSlider
                        value={draft.performanceMonitorIntervalMs}
                        onChange={(v) =>
                            patchDraft({
                                performanceMonitorIntervalMs:
                                    clampPerformanceMonitorIntervalMs(v),
                            })
                        }
                        disabled={!draft.performanceMonitorEnabled}
                    />
                </FieldRow>
            </SettingsSection>
        </SettingsTabSections>
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