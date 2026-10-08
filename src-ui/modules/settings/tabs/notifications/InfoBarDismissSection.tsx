// 应用内提示条区块：info / success / warning 三档自动关闭时长。

import { Switch } from '../../../../shared/ui';
import { DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED } from '../../../../core/domain/ui/infoBarDismiss';
import type { SettingsDraft } from '../../settings-draft';
import {
    FieldRow,
    InfoBarDismissDurationSlider,
    InfoBarDismissSliderPresence,
    SettingsSection,
} from '../../_shared';

export function InfoBarDismissSection({
    draft,
    patchDraft,
}: {
    draft: SettingsDraft;
    patchDraft: (patch: Partial<SettingsDraft>) => void;
}) {
    return (
        <SettingsSection title="应用内提示条" description="错误提示需手动关闭；以下三类可自动消失">
            <FieldRow label="说明">
                <InfoBarDismissSliderPresence visible={draft.infoBarDismissInfoEnabled}>
                    <InfoBarDismissDurationSlider
                        value={draft.infoBarDismissInfoMs}
                        defaultMs={DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED.infoBarDismissInfoMs}
                        onChange={(v) => patchDraft({ infoBarDismissInfoMs: v })}
                    />
                </InfoBarDismissSliderPresence>
                <Switch
                    checked={draft.infoBarDismissInfoEnabled}
                    onCheckedChange={(v) => patchDraft({ infoBarDismissInfoEnabled: v })}
                />
            </FieldRow>
            <FieldRow label="成功">
                <InfoBarDismissSliderPresence visible={draft.infoBarDismissSuccessEnabled}>
                    <InfoBarDismissDurationSlider
                        value={draft.infoBarDismissSuccessMs}
                        defaultMs={DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED.infoBarDismissSuccessMs}
                        onChange={(v) => patchDraft({ infoBarDismissSuccessMs: v })}
                    />
                </InfoBarDismissSliderPresence>
                <Switch
                    checked={draft.infoBarDismissSuccessEnabled}
                    onCheckedChange={(v) => patchDraft({ infoBarDismissSuccessEnabled: v })}
                />
            </FieldRow>
            <FieldRow label="警告" isLast>
                <InfoBarDismissSliderPresence visible={draft.infoBarDismissWarningEnabled}>
                    <InfoBarDismissDurationSlider
                        value={draft.infoBarDismissWarningMs}
                        defaultMs={DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED.infoBarDismissWarningMs}
                        onChange={(v) => patchDraft({ infoBarDismissWarningMs: v })}
                    />
                </InfoBarDismissSliderPresence>
                <Switch
                    checked={draft.infoBarDismissWarningEnabled}
                    onCheckedChange={(v) => patchDraft({ infoBarDismissWarningEnabled: v })}
                />
            </FieldRow>
        </SettingsSection>
    );
}
