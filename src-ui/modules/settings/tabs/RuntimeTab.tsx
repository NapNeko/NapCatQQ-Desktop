// 运行 Tab：Bot 轮询与远程主机后台探活。

import {
    clampRemoteHostHealthProbeIntervalMs,
    REMOTE_HOST_HEALTH_PROBE_INTERVAL_MS_MAX,
    REMOTE_HOST_HEALTH_PROBE_INTERVAL_MS_MIN,
} from '../../../core/domain/remote-host/healthProbeSettings';
import { NumberField, Switch } from '../../../shared/ui';
import type { SettingsDraft } from '../settings-draft';
import {
    FieldRow,
    RemoteHostHealthProbeIntervalSlider,
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

            <SettingsSection
                title="远程主机"
                description="后台定期对已连接的远端主机执行低频探测（is_healthy）。探测失败时自动标记并通知，不影响本机主机。默认开启低频（30 秒一次）。"
            >
                <FieldRow
                    label="启用后台探活"
                    description="关闭后不再主动探测远端主机连通性；已有的失败状态仍保留，直至下次成功连接或手动测试。"
                >
                    <Switch
                        checked={draft.remoteHostHealthProbeEnabled}
                        onCheckedChange={(v) =>
                            patchDraft({ remoteHostHealthProbeEnabled: v })
                        }
                    />
                </FieldRow>

                <FieldRow
                    label="探活间隔"
                    description={`仅在启用时生效。${REMOTE_HOST_HEALTH_PROBE_INTERVAL_MS_MIN / 1000}–${REMOTE_HOST_HEALTH_PROBE_INTERVAL_MS_MAX / 1000} 秒，拖动滑块调整。`}
                    isLast
                >
                    <RemoteHostHealthProbeIntervalSlider
                        value={draft.remoteHostHealthProbeIntervalMs}
                        onChange={(v) =>
                            patchDraft({
                                remoteHostHealthProbeIntervalMs:
                                    clampRemoteHostHealthProbeIntervalMs(v),
                            })
                        }
                        disabled={!draft.remoteHostHealthProbeEnabled}
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
