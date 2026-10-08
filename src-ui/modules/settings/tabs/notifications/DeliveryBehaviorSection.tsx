// 投递行为区块：恢复通知、防抖、历史上限与投递记录入口。
// 历史数据本体留在主文件（来自 settings.service），这里只收条数与打开回调。

import { History } from 'lucide-react';
import { Button, NumberField, Switch } from '../../../../shared/ui';
import { ActionMotionIcon } from '../../../../shared/ui/motion';
import type { SettingsDraft } from '../../settings-draft';
import { FieldRow, SettingsSection } from '../../_shared';

export function DeliveryBehaviorSection({
    draft,
    patchDraft,
    historyCount,
    onOpenHistory,
}: {
    draft: SettingsDraft;
    patchDraft: (patch: Partial<SettingsDraft>) => void;
    historyCount: number;
    onOpenHistory: () => void;
}) {
    return (
        <SettingsSection
            title="投递行为"
            description="恢复通知、掉线防抖与投递历史（仅内存，重启清空）"
        >
            <FieldRow label="上线恢复通知" description="掉线后恢复上线时再推一次；默认关闭">
                <Switch
                    checked={draft.notifyOnRecovered}
                    onCheckedChange={(v) => patchDraft({ notifyOnRecovered: v })}
                />
            </FieldRow>
            <FieldRow label="掉线防抖（秒）" description="同一 Bot 在此秒数内只投递一次；0 关闭">
                <NumberField
                    value={draft.offlineDebounceSeconds}
                    min={0}
                    max={600}
                    step={1}
                    onValueChange={(v) =>
                        patchDraft({
                            offlineDebounceSeconds: Math.max(0, Math.min(600, Math.round(v || 0))),
                        })
                    }
                    className="w-28"
                />
            </FieldRow>
            <FieldRow label="历史条数上限" description="内存保留最近 N 条；0 不记录">
                <NumberField
                    value={draft.offlineDeliveryHistoryLimit}
                    min={0}
                    max={200}
                    step={1}
                    onValueChange={(v) =>
                        patchDraft({
                            offlineDeliveryHistoryLimit: Math.max(
                                0,
                                Math.min(200, Math.round(v || 0)),
                            ),
                        })
                    }
                    className="w-28"
                />
            </FieldRow>
            <FieldRow
                label="投递记录"
                description={
                    historyCount > 0
                        ? `本次运行已有 ${historyCount} 条记录；重启后自动清空`
                        : '查看各渠道投递结果；重启后自动清空'
                }
                isLast
            >
                <Button type="button" variant="secondary" size="sm" onClick={onOpenHistory}>
                    <ActionMotionIcon icon={History} size={14} />
                    查看记录
                </Button>
            </FieldRow>
        </SettingsSection>
    );
}
