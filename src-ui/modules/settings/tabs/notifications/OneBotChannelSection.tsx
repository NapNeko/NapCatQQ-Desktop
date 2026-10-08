// OneBot 通知区块：开关 + 就绪摘要。发送方 / 目标 / 模板编辑在 OneBotEditorDialog。

import { Pencil } from 'lucide-react';
import { Badge, Button, Switch } from '../../../../shared/ui';
import { ActionMotionIcon } from '../../../../shared/ui/motion';
import type { SettingsDraft } from '../../settings-draft';
import { FieldRow, SettingsSection } from '../../_shared';
import { oneBotIsReady, oneBotSummary, type OneBotEditorDraft } from './OneBotEditorDialog';

export function OneBotChannelSection({
    draft,
    patchDraft,
    oneBotDraft,
    onOpenEditor,
}: {
    draft: SettingsDraft;
    patchDraft: (patch: Partial<SettingsDraft>) => void;
    oneBotDraft: OneBotEditorDraft;
    onOpenEditor: () => void;
}) {
    return (
        <SettingsSection
            title={
                <span className="flex items-center gap-2">
                    OneBot 通知
                    {draft.onebotNoticeEnabled && !oneBotIsReady(oneBotDraft) ? (
                        <Badge tone="warning" appearance="soft">
                            待完善
                        </Badge>
                    ) : null}
                </span>
            }
            description="本机掉线用本机发送方，远端掉线由该机 ncd-watch 发送；不跨服务器调用 OneBot"
        >
            <FieldRow
                label="启用 OneBot 通知"
                description={
                    draft.onebotNoticeEnabled
                        ? oneBotIsReady(oneBotDraft)
                            ? `已就绪 · ${oneBotSummary(oneBotDraft)}`
                            : oneBotSummary(oneBotDraft)
                        : '关闭时折叠配置，已填内容会保留'
                }
                isLast
            >
                <div className="flex items-center gap-2">
                    {draft.onebotNoticeEnabled ? (
                        <Button type="button" variant="secondary" size="sm" onClick={onOpenEditor}>
                            <ActionMotionIcon icon={Pencil} size={14} />
                            配置…
                        </Button>
                    ) : null}
                    <Switch
                        checked={draft.onebotNoticeEnabled}
                        onCheckedChange={(v) => patchDraft({ onebotNoticeEnabled: v })}
                    />
                </div>
            </FieldRow>
        </SettingsSection>
    );
}
