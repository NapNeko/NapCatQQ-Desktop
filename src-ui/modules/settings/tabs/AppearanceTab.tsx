// 外观 Tab：主题、吉祥物、动画、窗口关闭行为。

import type { CloseAction } from '../../../hooks/preferences/preferencesStore';
import { Select, Switch } from '../../../shared/ui';
import type { SettingsDraft } from '../settings-draft';
import {
    FieldRow,
    MotionLevelSegment,
    MotionSpeedSlider,
    RadiusStyleSegment,
    SettingsSection,
    SettingsTabSections,
    ThemeSegment,
} from '../_shared';

interface Props {
    draft: SettingsDraft | null;
    patchDraft: (patch: Partial<SettingsDraft>) => void;
}

export function AppearanceTab({ draft, patchDraft }: Props) {
    if (!draft) {
        return (
            <p className="text-[13px] text-text-tertiary">正在加载设置…</p>
        );
    }

    return (
        <SettingsTabSections>
            <SettingsSection title="界面" description="保存后生效；编辑过程中不会预览主题切换">
                <FieldRow
                    label="主题"
                    description="跟随系统、浅色或暗色"
                >
                    <ThemeSegment
                        value={draft.theme}
                        onChange={(v) => patchDraft({ theme: v })}
                    />
                </FieldRow>

                <FieldRow
                    label="圆角风格"
                    description="方正克制 · 标准平衡 · 圆润饱满，全局统一缩放"
                >
                    <RadiusStyleSegment
                        value={draft.radiusStyle}
                        onChange={(v) => patchDraft({ radiusStyle: v })}
                    />
                </FieldRow>

                <FieldRow
                    label="主页吉祥物"
                    description="概览页右上角猫娘"
                >
                    <Switch
                        checked={draft.showMascot}
                        onCheckedChange={(v) => patchDraft({ showMascot: v })}
                    />
                </FieldRow>

                <FieldRow
                    label="动画与体感"
                    description="总开关。关闭后过渡退化为瞬时；系统「减少动画」仍会覆盖"
                >
                    <Switch
                        checked={draft.motionEnabled}
                        onCheckedChange={(v) => patchDraft({ motionEnabled: v })}
                    />
                </FieldRow>

                <FieldRow
                    label="动画档位"
                    description="优雅 仅淡入淡出 · 标准 含轻 spring · 丰富 按钮弹性与卡片浮起"
                >
                    <MotionLevelSegment
                        value={draft.motionLevel}
                        onChange={(v) => patchDraft({ motionLevel: v })}
                        disabled={!draft.motionEnabled}
                    />
                </FieldRow>

                <FieldRow
                    label="动画速度"
                    description="0.5x 更克制；1.5x 更利落"
                    isLast
                >
                    <MotionSpeedSlider
                        value={draft.motionSpeed}
                        onChange={(v) => patchDraft({ motionSpeed: v })}
                        disabled={!draft.motionEnabled}
                    />
                </FieldRow>
            </SettingsSection>

            <SettingsSection title="窗口">
                <FieldRow
                    label="点击关闭按钮"
                    description="最小化到托盘，或退出程序（有本机 Bot 运行时会拦截退出）"
                    isLast
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
            </SettingsSection>
        </SettingsTabSections>
    );
}