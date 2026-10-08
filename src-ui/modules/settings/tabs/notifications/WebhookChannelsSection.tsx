// Webhook 推送区块：开关 + 通道摘要行列表。编辑 / 测试 / 删除动作回主文件编排。

import { Pencil, Plus, Trash2 } from 'lucide-react';
import { Badge, Button, Switch } from '../../../../shared/ui';
import { ActionMotionIcon } from '../../../../shared/ui/motion';
import { cn } from '../../../../shared/utils/cn';
import type { WebhookChannelDraft } from '../../../../core/domain/settings/offline-notify-defaults';
import type { SettingsDraft } from '../../settings-draft';
import { FieldRow, SettingsSection } from '../../_shared';
import { channelDisplayName, channelSummary } from './channel-utils';

export function WebhookChannelsSection({
    draft,
    patchDraft,
    channels,
    onOpenCreate,
    onOpenEdit,
    onRequestDelete,
}: {
    draft: SettingsDraft;
    patchDraft: (patch: Partial<SettingsDraft>) => void;
    channels: WebhookChannelDraft[];
    onOpenCreate: () => void;
    onOpenEdit: (ch: WebhookChannelDraft) => void;
    onRequestDelete: (id: string) => void;
}) {
    const enabledChannelCount = channels.filter((c) => c.enabled && c.url.trim()).length;

    return (
        <SettingsSection title="Webhook 推送">
            <FieldRow
                label="启用 Webhook"
                description={
                    draft.botOfflineWebHookNotice
                        ? enabledChannelCount > 0
                            ? `当前 ${enabledChannelCount} 个通道会在掉线时发送`
                            : '已开启，但还没有可用通道'
                        : '关闭时折叠配置，通道会保留'
                }
                isLast={!draft.botOfflineWebHookNotice}
            >
                <Switch
                    checked={draft.botOfflineWebHookNotice}
                    onCheckedChange={(v) => patchDraft({ botOfflineWebHookNotice: v })}
                />
            </FieldRow>

            {draft.botOfflineWebHookNotice ? (
                channels.length === 0 ? (
                    <FieldRow label="推送通道" description="还没有通道" isLast>
                        <Button type="button" variant="secondary" size="sm" onClick={onOpenCreate}>
                            <ActionMotionIcon icon={Plus} size={14} />
                            添加
                        </Button>
                    </FieldRow>
                ) : (
                    <>
                        {channels.map((ch) => {
                            const ready = ch.enabled && !!ch.url.trim();
                            const status = !ch.enabled
                                ? '已关闭'
                                : !ch.url.trim()
                                  ? '缺地址'
                                  : ch.method || 'POST';
                            return (
                                <FieldRow
                                    key={ch.id}
                                    label={channelDisplayName(ch)}
                                    description={`${channelSummary(ch)} · ${status}`}
                                    isLast={false}
                                >
                                    <div className="flex items-center gap-1.5">
                                        <span
                                            className={cn(
                                                'mr-1 h-1.5 w-1.5 shrink-0 rounded-full',
                                                ready ? 'bg-success' : 'bg-text-tertiary/45',
                                            )}
                                            aria-hidden
                                        />
                                        {!ch.enabled ? (
                                            <Badge tone="neutral" appearance="soft">
                                                已关闭
                                            </Badge>
                                        ) : !ch.url.trim() ? (
                                            <Badge tone="warning" appearance="soft">
                                                缺地址
                                            </Badge>
                                        ) : null}
                                        <Button
                                            type="button"
                                            variant="ghost"
                                            size="sm"
                                            aria-label={`编辑 ${channelDisplayName(ch)}`}
                                            onClick={() => onOpenEdit(ch)}
                                        >
                                            <ActionMotionIcon icon={Pencil} size={14} />
                                        </Button>
                                        <Button
                                            type="button"
                                            variant="ghost"
                                            size="sm"
                                            aria-label={`删除 ${channelDisplayName(ch)}`}
                                            onClick={() => onRequestDelete(ch.id)}
                                        >
                                            <ActionMotionIcon icon={Trash2} size={14} />
                                        </Button>
                                    </div>
                                </FieldRow>
                            );
                        })}
                        <FieldRow label="添加通道" isLast>
                            <Button
                                type="button"
                                variant="secondary"
                                size="sm"
                                onClick={onOpenCreate}
                            >
                                <ActionMotionIcon icon={Plus} size={14} />
                                添加
                            </Button>
                        </FieldRow>
                    </>
                )
            ) : null}
        </SettingsSection>
    );
}
