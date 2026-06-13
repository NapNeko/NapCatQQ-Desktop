// 运行 Tab：Bot 轮询与概览性能监控（写入 app-settings）。

import {
    clampPerformanceMonitorIntervalMs,
    PERFORMANCE_MONITOR_INTERVAL_MS_MAX,
    PERFORMANCE_MONITOR_INTERVAL_MS_MIN,
} from '../../../core/domain/performance/performanceSettings';
import { DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED } from '../../../core/domain/ui/infoBarDismiss';
import { DEFAULT_TASK_QUEUE_CLEANUP_WHEN_ENABLED_MS } from '../../../core/domain/task-queue/cleanup';
import { NumberField, Switch } from '../../../shared/ui';
import type { SettingsDraft } from '../settings-draft';
import {
    FieldRow,
    InfoBarDismissSliderPresence,
    InfoBarDismissDurationSlider,
    PerformanceMonitorIntervalSlider,
    TaskQueueCleanupDurationSlider,
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

            <SettingsSection
                title="任务队列"
                description="已完成、失败或已取消的条目在列表中的保留时间；关闭自动清理则一直保留，直至重启应用。"
            >
                <FieldRow
                    label="自动清理"
                    description="开启后，终态任务在下方时长过后从任务队列移除"
                >
                    <InfoBarDismissSliderPresence
                        visible={draft.taskQueueCleanupEnabled}
                    >
                        <TaskQueueCleanupDurationSlider
                            value={draft.taskQueueCleanupLingerMs}
                            defaultMs={DEFAULT_TASK_QUEUE_CLEANUP_WHEN_ENABLED_MS}
                            onChange={(v) =>
                                patchDraft({ taskQueueCleanupLingerMs: v })
                            }
                        />
                    </InfoBarDismissSliderPresence>
                    <Switch
                        checked={draft.taskQueueCleanupEnabled}
                        onCheckedChange={(v) =>
                            patchDraft({ taskQueueCleanupEnabled: v })
                        }
                    />
                </FieldRow>
            </SettingsSection>

            <SettingsSection
                title="顶部提示条"
                description="错误类（红色）始终需手动关闭。以下开关控制说明 / 成功 / 警告是否自动消失。"
            >
                <FieldRow label="说明类" description="info 蓝色提示">
                    <InfoBarDismissSliderPresence
                        visible={draft.infoBarDismissInfoEnabled}
                    >
                        <InfoBarDismissDurationSlider
                            value={draft.infoBarDismissInfoMs}
                            defaultMs={
                                DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED.infoBarDismissInfoMs
                            }
                            onChange={(v) =>
                                patchDraft({ infoBarDismissInfoMs: v })
                            }
                        />
                    </InfoBarDismissSliderPresence>
                    <Switch
                        checked={draft.infoBarDismissInfoEnabled}
                        onCheckedChange={(v) =>
                            patchDraft({ infoBarDismissInfoEnabled: v })
                        }
                    />
                </FieldRow>
                <FieldRow label="成功类" description="操作成功等">
                    <InfoBarDismissSliderPresence
                        visible={draft.infoBarDismissSuccessEnabled}
                    >
                        <InfoBarDismissDurationSlider
                            value={draft.infoBarDismissSuccessMs}
                            defaultMs={
                                DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED.infoBarDismissSuccessMs
                            }
                            onChange={(v) =>
                                patchDraft({ infoBarDismissSuccessMs: v })
                            }
                        />
                    </InfoBarDismissSliderPresence>
                    <Switch
                        checked={draft.infoBarDismissSuccessEnabled}
                        onCheckedChange={(v) =>
                            patchDraft({ infoBarDismissSuccessEnabled: v })
                        }
                    />
                </FieldRow>
                <FieldRow label="警告类" description="被踢、取消等" isLast>
                    <InfoBarDismissSliderPresence
                        visible={draft.infoBarDismissWarningEnabled}
                    >
                        <InfoBarDismissDurationSlider
                            value={draft.infoBarDismissWarningMs}
                            defaultMs={
                                DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED.infoBarDismissWarningMs
                            }
                            onChange={(v) =>
                                patchDraft({ infoBarDismissWarningMs: v })
                            }
                        />
                    </InfoBarDismissSliderPresence>
                    <Switch
                        checked={draft.infoBarDismissWarningEnabled}
                        onCheckedChange={(v) =>
                            patchDraft({ infoBarDismissWarningEnabled: v })
                        }
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