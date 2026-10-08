// 邮件通知区块：开关 + 就绪摘要。连接详情编辑在 EmailEditorDialog。

import { Pencil } from 'lucide-react';
import { Badge, Button, Switch } from '../../../../shared/ui';
import { ActionMotionIcon } from '../../../../shared/ui/motion';
import type { SettingsDraft } from '../../settings-draft';
import { FieldRow, SettingsSection } from '../../_shared';
import { emailIsReady, emailSummary, type EmailEditorDraft } from './EmailEditorDialog';

export function EmailChannelSection({
    draft,
    patchDraft,
    emailDraft,
    onOpenEditor,
}: {
    draft: SettingsDraft;
    patchDraft: (patch: Partial<SettingsDraft>) => void;
    emailDraft: EmailEditorDraft;
    onOpenEditor: () => void;
}) {
    return (
        <SettingsSection
            title={
                <span className="flex items-center gap-2">
                    邮件通知
                    {draft.botOfflineEmailNotice && !emailIsReady(emailDraft) ? (
                        <Badge tone="warning" appearance="soft">
                            待完善
                        </Badge>
                    ) : null}
                </span>
            }
        >
            <FieldRow
                label="启用邮件"
                description={
                    draft.botOfflineEmailNotice
                        ? emailIsReady(emailDraft)
                            ? `已就绪 · ${emailSummary(emailDraft)}`
                            : emailSummary(emailDraft)
                        : '关闭时折叠配置，已填内容会保留'
                }
                isLast
            >
                <div className="flex items-center gap-2">
                    {draft.botOfflineEmailNotice ? (
                        <Button type="button" variant="secondary" size="sm" onClick={onOpenEditor}>
                            <ActionMotionIcon icon={Pencil} size={14} />
                            配置…
                        </Button>
                    ) : null}
                    <Switch
                        checked={draft.botOfflineEmailNotice}
                        onCheckedChange={(v) => patchDraft({ botOfflineEmailNotice: v })}
                    />
                </div>
            </FieldRow>
        </SettingsSection>
    );
}
