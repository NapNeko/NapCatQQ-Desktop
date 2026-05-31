// 通用 Tab。两类设置混排但语义分明：
//   - 上半：纯客户端偏好（主题 / 吉祥物 / 窗口不透明度 / 关闭行为），切换即时生效。
//   - 下半：后端持久化设置（Bot 登录检查间隔 / 性能监控），改动进草稿，底部保存条落盘。

import { preferencesStore, type AppPreferences } from '../../../hooks/preferences/preferencesStore';
import type { BackendSettings } from '../../../core/services/settings.service';
import { NumberField, Select, Switch } from '../../../shared/ui';
import { FieldRow, ThemeSegment } from '../_shared';

interface Props {
    prefs: AppPreferences;
    draft: BackendSettings | null;
    patchDraft: (patch: Partial<BackendSettings>) => void;
}

export function GeneralTab({ prefs, draft, patchDraft }: Props) {
    return (
        <>
            <FieldRow label="主题" description="切换后立即生效，无需重启">
                <ThemeSegment value={prefs.theme} onChange={preferencesStore.setTheme} />
            </FieldRow>

            <FieldRow label="主页吉祥物" description="概览页右上角的猫娘">
                <Switch
                    checked={prefs.showMascot}
                    onCheckedChange={preferencesStore.setShowMascot}
                />
            </FieldRow>

            <FieldRow
                label="窗口不透明度"
                description="80–100，仅作用于主背景；真窗口透明需 Tauri 配置（待）"
            >
                <input
                    type="range"
                    min={80}
                    max={100}
                    step={1}
                    value={prefs.windowOpacity}
                    onChange={(e) => preferencesStore.setWindowOpacity(Number(e.target.value))}
                    className="h-1 w-32 cursor-pointer accent-brand"
                />
                <span className="w-9 text-right font-mono text-[11.5px] tabular-nums text-text-tertiary">
                    {prefs.windowOpacity}%
                </span>
            </FieldRow>

            <FieldRow
                label="点击关闭按钮"
                description="tray 模式需要 Tauri 系统托盘配套，当前选 tray 暂同 close"
            >
                <Select
                    value={prefs.closeAction}
                    onValueChange={(v) =>
                        preferencesStore.setCloseAction(v as 'close' | 'tray')
                    }
                    items={[
                        { value: 'close', label: '关闭程序' },
                        { value: 'tray', label: '最小化到托盘' },
                    ]}
                />
            </FieldRow>

            {/* 后端持久化设置：改动进草稿，底部保存条落盘 */}
            <FieldRow
                label="Bot 登录检查间隔"
                description="已登录状态下轮询 NapCat WebUI 检查在线态的间隔；未登录时固定 1 秒。范围 1000–60000 毫秒"
            >
                <BackendNumber
                    value={draft?.botLoginCheckIntervalMs ?? null}
                    min={1000}
                    max={60000}
                    step={500}
                    onChange={(v) => patchDraft({ botLoginCheckIntervalMs: v })}
                    suffix="ms"
                />
            </FieldRow>

            <FieldRow
                label="主页性能监控"
                description="关闭后概览页不再显示 CPU / 内存占用曲线"
            >
                <Switch
                    checked={draft?.performanceMonitorEnabled ?? true}
                    onCheckedChange={(v) => patchDraft({ performanceMonitorEnabled: v })}
                />
            </FieldRow>

            <FieldRow
                label="性能监控采样间隔"
                description="概览页 CPU / 内存曲线的刷新间隔。范围 500–10000 毫秒"
                isLast
            >
                <BackendNumber
                    value={draft?.performanceMonitorIntervalMs ?? null}
                    min={500}
                    max={10000}
                    step={100}
                    onChange={(v) => patchDraft({ performanceMonitorIntervalMs: v })}
                    suffix="ms"
                    disabled={draft ? !draft.performanceMonitorEnabled : false}
                />
            </FieldRow>
        </>
    );
}

/// 设置页统一的数值输入：固定 w，空值回落到 min，clamp 到 [min,max]。
function BackendNumber({
    value,
    min,
    max,
    step,
    suffix,
    disabled,
    onChange,
}: {
    value: number | null;
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
                    const next = n ?? min;
                    onChange(Math.max(min, Math.min(max, Math.round(next))));
                }}
            />
            {suffix && (
                <span className="text-[11.5px] text-text-tertiary">{suffix}</span>
            )}
        </div>
    );
}
