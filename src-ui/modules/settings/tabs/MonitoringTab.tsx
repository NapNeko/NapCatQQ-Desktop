// 监控 Tab：概览性能采样与任务队列终态清理。

import {
    clampPerformanceMonitorIntervalMs,
    PERFORMANCE_MONITOR_INTERVAL_MS_MAX,
    PERFORMANCE_MONITOR_INTERVAL_MS_MIN,
} from '../../../core/domain/performance/performanceSettings';
import { DEFAULT_TASK_QUEUE_CLEANUP_WHEN_ENABLED_MS } from '../../../core/domain/task-queue/cleanup';
import { Switch } from '../../../shared/ui';
import type { SettingsDraft } from '../settings-draft';
import {
    FieldRow,
    InfoBarDismissSliderPresence,
    PerformanceMonitorIntervalSlider,
    TaskQueueCleanupDurationSlider,
    SettingsSection,
    SettingsTabSections,
} from '../_shared';

interface Props {
    draft: SettingsDraft | null;
    patchDraft: (patch: Partial<SettingsDraft>) => void;
}

export function MonitoringTab({ draft, patchDraft }: Props) {
    if (!draft) {
        return (
            <p className="text-[13px] text-text-tertiary">正在加载设置…</p>
        );
    }

    return (
        <SettingsTabSections>
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
                    isLast
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
        </SettingsTabSections>
    );
}
