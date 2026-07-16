// 监控 Tab：概览性能采样与任务队列终态清理。

import {
    clampPerformanceMonitorIntervalMs,
    PERFORMANCE_MONITOR_INTERVAL_MS_MAX,
    PERFORMANCE_MONITOR_INTERVAL_MS_MIN,
} from '../../../core/domain/performance/performanceSettings';
import {
    BOT_RUNTIME_METRICS_RETENTION_DAYS_DEFAULT,
    BOT_RUNTIME_METRICS_RETENTION_DAYS_MAX,
    BOT_RUNTIME_METRICS_RETENTION_DAYS_MIN,
    clampBotRuntimeMetricsRetentionDays,
} from '../../../core/domain/bot/runtime-metrics-settings';
import { DEFAULT_TASK_QUEUE_CLEANUP_WHEN_ENABLED_MS } from '../../../core/domain/task-queue/cleanup';
import { NumberField, Switch } from '../../../shared/ui';
import type { SettingsDraft } from '../settings-draft';
import {
    FieldRow,
    BotRuntimeMetricsIntervalSlider,
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
                title="实例运行时指标"
                description="内存与 OneBot 网络节点收发。不修改你的连接配置；探针需重新启动实例后生效。远端已装 ncd-watch 时，Desktop 退出后仍继续记录历史。"
            >
                <FieldRow
                    label="启用实例指标"
                    description="默认关闭。保存后重启需要观测的 Bot，轻量探针才会随进程载入"
                >
                    <Switch
                        checked={draft.botRuntimeMetricsEnabled}
                        onCheckedChange={(v) =>
                            patchDraft({ botRuntimeMetricsEnabled: v })
                        }
                    />
                </FieldRow>
                <FieldRow
                    label="采样间隔"
                    description="影响当前数据的刷新速度；历史趋势按不低于 1 分钟的粒度保存"
                >
                    <BotRuntimeMetricsIntervalSlider
                        disabled={!draft.botRuntimeMetricsEnabled}
                        value={draft.botRuntimeMetricsIntervalMs}
                        onChange={(value) =>
                            patchDraft({
                                botRuntimeMetricsIntervalMs: value,
                            })
                        }
                    />
                </FieldRow>
                <FieldRow
                    label="历史保留"
                    description="默认 7 天，最长 90 天；图表粒度可能降为约 1 分钟"
                    isLast
                >
                    <div className="flex items-center gap-2">
                        <NumberField
                            aria-label="指标历史保留天数"
                            name="botRuntimeMetricsRetentionDays"
                            min={BOT_RUNTIME_METRICS_RETENTION_DAYS_MIN}
                            max={BOT_RUNTIME_METRICS_RETENTION_DAYS_MAX}
                            disabled={!draft.botRuntimeMetricsEnabled}
                            value={draft.botRuntimeMetricsRetentionDays}
                            onValueChange={(value) =>
                                patchDraft({
                                    botRuntimeMetricsRetentionDays: clampBotRuntimeMetricsRetentionDays(
                                        value ?? BOT_RUNTIME_METRICS_RETENTION_DAYS_DEFAULT,
                                    ),
                                })
                            }
                            className="w-20"
                        />
                        <span className="text-[12px] text-text-tertiary">天</span>
                    </div>
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
