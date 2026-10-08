// 桌面通知区块：掉线 / 异常退出 / 被踢三类事件开关。

import { Switch } from '../../../../shared/ui';
import type { SettingsDraft } from '../../settings-draft';
import { FieldRow, SettingsSection } from '../../_shared';

export function DesktopNotifySection({
    draft,
    patchDraft,
}: {
    draft: SettingsDraft;
    patchDraft: (patch: Partial<SettingsDraft>) => void;
}) {
    return (
        <SettingsSection title="桌面通知" description="主窗口隐藏或进入轻量模式时仍会提醒">
            <FieldRow
                label="Bot 掉线"
                description="还需在 Bot 高级设置里打开「掉线时发送通知」（NapCat / SnowLuma 均需）；此开关同时门控 Webhook / 邮件 / OneBot"
            >
                <Switch
                    checked={draft.notifyOnOffline}
                    onCheckedChange={(v) => patchDraft({ notifyOnOffline: v })}
                />
            </FieldRow>
            <FieldRow label="Bot 异常退出" description="所有 Bot 进程非正常结束时">
                <Switch
                    checked={draft.notifyOnBotCrashed}
                    onCheckedChange={(v) => patchDraft({ notifyOnBotCrashed: v })}
                />
            </FieldRow>
            <FieldRow label="被踢下线" description="所有 Bot 被踢下线或登录失效时" isLast>
                <Switch
                    checked={draft.notifyOnLoginKicked}
                    onCheckedChange={(v) => patchDraft({ notifyOnLoginKicked: v })}
                />
            </FieldRow>
        </SettingsSection>
    );
}
