// 外观 Tab：主题、吉祥物、动画、窗口关闭行为。

import type { CloseAction } from '../../../hooks/preferences/preferencesStore';
import type {
    AfterCloseUiBehavior,
    UiModeOnStartup,
} from '../../../core/services/settings.service';
import { Select, Switch } from '../../../shared/ui';
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
            <SettingsSection title="界面" description="保存后生效；编辑过程中不会预览主题切换">
                <FieldRow
                    label="主题"
                    description="系统跟随 / 浅色 / 暗色 / Catppuccin 风味"
                >
                    <ThemePicker
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
                    description="1.00× 为默认体感；更快可拉到 3.00×"
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

                {draft.closeAction === 'tray' && (
                    <>
                        <FieldRow
                            label="关窗后界面"
                            description="最小化到托盘后，是否在一段时间不用后自动释放界面内存"
                        >
                            <Select
                                value={draft.afterCloseUiBehavior}
                                onValueChange={(v) =>
                                    patchDraft({
                                        afterCloseUiBehavior:
                                            v as AfterCloseUiBehavior,
                                    })
                                }
                                items={[
                                    {
                                        value: 'hide',
                                        label: '保持隐藏（占内存）',
                                    },
                                    {
                                        value: 'delayed_lightweight',
                                        label: '一段时间不用后释放（推荐）',
                                    },
                                    {
                                        value: 'immediate_lightweight',
                                        label: '立即释放界面内存',
                                    },
                                ]}
                            />
                        </FieldRow>
                        {draft.afterCloseUiBehavior ===
                            'delayed_lightweight' && (
                            <FieldRow
                                label="释放前等待"
                                description="主窗口不可见累计多久后释放 WebView"
                            >
                                <Select
                                    value={String(
                                        draft.enterLightweightDelaySecs,
                                    )}
                                    onValueChange={(v) =>
                                        patchDraft({
                                            enterLightweightDelaySecs: Number(
                                                v,
                                            ),
                                        })
                                    }
                                    items={[
                                        { value: '60', label: '1 分钟' },
                                        { value: '180', label: '3 分钟' },
                                        { value: '300', label: '5 分钟' },
                                        { value: '900', label: '15 分钟' },
                                        { value: '1800', label: '30 分钟' },
                                    ]}
                                />
                            </FieldRow>
                        )}
                    </>
                )}

                <FieldRow
                    label="启动时"
                    description="仅托盘：无界面直接托管 Bot，需从托盘打开主界面"
                >
                    <Select
                        value={draft.uiModeOnStartup}
                        onValueChange={(v) =>
                            patchDraft({
                                uiModeOnStartup: v as UiModeOnStartup,
                            })
                        }
                        items={[
                            { value: 'normal', label: '显示主界面' },
                            { value: 'tray_only', label: '仅托盘' },
                        ]}
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
                    description="QQ 被踢或登录失效（全局）；「掉线」类通知请在各 Bot 高级里开启"
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
        </SettingsTabSections>
    );
}