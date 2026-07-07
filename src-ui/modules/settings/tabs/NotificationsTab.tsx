// 通知 Tab：桌面 Toast 与顶部 InfoBar 自动关闭策略。

import { DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED } from '../../../core/domain/ui/infoBarDismiss';
import { Switch } from '../../../shared/ui';
import type { SettingsDraft } from '../settings-draft';
import {
    FieldRow,
    InfoBarDismissDurationSlider,
    InfoBarDismissSliderPresence,
    SettingsSection,
    SettingsTabSections,
} from '../_shared';

interface Props {
    draft: SettingsDraft | null;
    patchDraft: (patch: Partial<SettingsDraft>) => void;
}

export function NotificationsTab({ draft, patchDraft }: Props) {
    if (!draft) {
        return (
            <p className="text-[13px] text-text-tertiary">正在加载设置…</p>
        );
    }

    return (
        <SettingsTabSections>
            <SettingsSection
                title="桌面通知"
                description="这些通知在主窗口隐藏或轻量模式下仍会通过系统 Toast 弹出。"
            >
                <FieldRow
                    label="Bot 掉线时通知"
                    description="仅当对应 Bot 高级配置开启「掉线时下发桌面通知」后生效"
                >
                    <Switch
                        checked={draft.notifyOnOffline}
                        onCheckedChange={(v) =>
                            patchDraft({ notifyOnOffline: v })
                        }
                    />
                </FieldRow>

                <FieldRow
                    label="Bot 异常退出时通知"
                    description="进程非正常结束（全局，与 Bot 配置无关）"
                >
                    <Switch
                        checked={draft.notifyOnBotCrashed}
                        onCheckedChange={(v) =>
                            patchDraft({ notifyOnBotCrashed: v })
                        }
                    />
                </FieldRow>

                <FieldRow
                    label="被踢下线时通知"
                    description="QQ 被踢或登录失效（全局）"
                    isLast
                >
                    <Switch
                        checked={draft.notifyOnLoginKicked}
                        onCheckedChange={(v) =>
                            patchDraft({ notifyOnLoginKicked: v })
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
