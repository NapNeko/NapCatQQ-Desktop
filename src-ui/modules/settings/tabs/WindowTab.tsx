// 窗口 Tab：关闭按钮、托盘、轻量模式和启动显示策略。

import type { CloseAction } from '../../../hooks/preferences/preferencesStore';
import type {
    AfterCloseUiBehavior,
    UiModeOnStartup,
} from '../../../core/services/settings.service';
import { Select, Switch } from '../../../shared/ui';
import type { SettingsDraft } from '../settings-draft';
import { FieldRow, SettingsSection, SettingsTabSections } from '../_shared';

interface Props {
    draft: SettingsDraft | null;
    patchDraft: (patch: Partial<SettingsDraft>) => void;
}

export function WindowTab({ draft, patchDraft }: Props) {
    if (!draft) {
        return (
            <p className="text-[13px] text-text-tertiary">正在加载设置…</p>
        );
    }

    return (
        <SettingsTabSections>
            <SettingsSection title="关闭与托盘">
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
            </SettingsSection>

            <SettingsSection title="启动">
                <FieldRow
                    label="开机自启"
                    description="登录当前 Windows 用户后自动启动（写入当前用户启动项，无需管理员）"
                >
                    <Switch
                        checked={draft.launchOnStartup}
                        onCheckedChange={(v) =>
                            patchDraft({ launchOnStartup: v })
                        }
                    />
                </FieldRow>
                <FieldRow
                    label="启动时"
                    description="仅托盘：无界面直接托管 Bot，需从托盘打开主界面"
                    isLast
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
            </SettingsSection>
        </SettingsTabSections>
    );
}
