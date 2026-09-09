// 外观 Tab：主题、吉祥物、圆角与动画体感。

import { Switch } from '../../../shared/ui';
import type { SettingsDraft } from '../settings-draft';
import {
    FieldRow,
    MotionLevelSegment,
    MotionSpeedSlider,
    RadiusStyleSegment,
    SettingsSection,
    SettingsTabSections,
    ThemePicker,
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
            <SettingsSection title="界面" description="保存后生效，编辑时不预览主题切换">
                <FieldRow label="主题">
                    <ThemePicker
                        value={draft.theme}
                        onChange={(v) => patchDraft({ theme: v })}
                    />
                </FieldRow>

                <FieldRow label="圆角风格">
                    <RadiusStyleSegment
                        value={draft.radiusStyle}
                        onChange={(v) => patchDraft({ radiusStyle: v })}
                    />
                </FieldRow>

                <FieldRow label="主页吉祥物" isLast>
                    <Switch
                        checked={draft.showMascot}
                        onCheckedChange={(v) => patchDraft({ showMascot: v })}
                    />
                </FieldRow>
            </SettingsSection>

            <SettingsSection title="动效">
                <FieldRow
                    label="动画与体感"
                    description="关闭后过渡变为瞬时；系统「减少动画」仍会覆盖"
                >
                    <Switch
                        checked={draft.motionEnabled}
                        onCheckedChange={(v) => patchDraft({ motionEnabled: v })}
                    />
                </FieldRow>

                <FieldRow label="动画档位">
                    <MotionLevelSegment
                        value={draft.motionLevel}
                        onChange={(v) => patchDraft({ motionLevel: v })}
                        disabled={!draft.motionEnabled}
                    />
                </FieldRow>

                <FieldRow
                    label="动画速度"
                    description="默认 1.00×，最快 3.00×"
                    isLast
                >
                    <MotionSpeedSlider
                        value={draft.motionSpeed}
                        onChange={(v) => patchDraft({ motionSpeed: v })}
                        disabled={!draft.motionEnabled}
                    />
                </FieldRow>
            </SettingsSection>
        </SettingsTabSections>
    );
}
