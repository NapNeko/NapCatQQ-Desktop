// OneBot 掉线通知配置 Dialog：发送方 + 目标 + 消息模板

import { DEFAULT_ONEBOT_MESSAGE } from '../../../../core/domain/settings/offline-notify-defaults';
import {
    Badge,
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Select,
} from '../../../../shared/ui';
import type { SettingsDraft } from '../../settings-draft';
import { OneBotMessageEditor } from './OneBotMessageEditor';
import {
    OneBotMessengerPicker,
    type OneBotCandidate,
} from './OneBotMessengerPicker';
import { TargetIdChipsInput } from './TargetIdChipsInput';

const ONEBOT_TARGET_ITEMS = [
    { value: 'private', label: '私聊' },
    { value: 'group', label: '群聊' },
] as const;

export type OneBotEditorDraft = Pick<
    SettingsDraft,
    | 'onebotMessengerBotIds'
    | 'onebotTargetType'
    | 'onebotTargetIds'
    | 'onebotMessageTemplate'
>;

export function oneBotIsReady(oneBot: OneBotEditorDraft): boolean {
    return (
        oneBot.onebotMessengerBotIds.some((id) => id.trim()) &&
        oneBot.onebotTargetIds.some((id) => id > 0)
    );
}

export function oneBotSummary(oneBot: OneBotEditorDraft): string {
    const messengers = oneBot.onebotMessengerBotIds.filter((id) => id.trim());
    const targets = oneBot.onebotTargetIds.filter((id) => id > 0);
    if (messengers.length === 0) return '尚未选择发送方 Bot';
    if (targets.length === 0) return '待填写接收目标';
    const messengerLabel =
        messengers.length === 1
            ? messengers[0]
            : `${messengers[0]} 等 ${messengers.length} 个`;
    const targetLabel =
        targets.length === 1
            ? String(targets[0])
            : `${targets[0]} 等 ${targets.length} 个`;
    return `${messengerLabel} → ${oneBot.onebotTargetType === 'group' ? '群' : '私聊'} ${targetLabel}`;
}

export function OneBotEditorDialog({
    open,
    draft,
    candidates,
    candidatesLoading,
    enablingId,
    onOpenChange,
    onDraftChange,
    onEnsureHttp,
    onSave,
}: {
    open: boolean;
    draft: OneBotEditorDraft;
    candidates: OneBotCandidate[];
    candidatesLoading: boolean;
    enablingId: string | null;
    onOpenChange: (open: boolean) => void;
    onDraftChange: (patch: Partial<OneBotEditorDraft>) => void;
    onEnsureHttp: (botId: string) => void;
    onSave: () => void;
}) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent size="sheetWide" dismissOnOutsideClick={false}>
                <DialogHeader className="shrink-0">
                    <DialogTitle>配置 OneBot 通知</DialogTitle>
                    <DialogDescription>
                        按主机勾选发送方：本机供 Desktop，远端供同机 ncd-watch。目标与模板全局共用。
                    </DialogDescription>
                </DialogHeader>

                {/* min-h 抬高 sheetWide 内容高度，避免矮对话框里目标区被压扁 */}
                <div className="grid min-h-[min(72dvh,40rem)] flex-1 grid-cols-1 gap-3 overflow-y-auto lg:grid-cols-[minmax(0,1.15fr)_minmax(18rem,0.85fr)] lg:overflow-hidden">
                    {/* 左：发送方列表（主焦点） */}
                    <div className="flex min-h-[22rem] min-w-0 flex-col lg:h-full lg:min-h-0">
                        <OneBotMessengerPicker
                            selected={draft.onebotMessengerBotIds}
                            candidates={candidates}
                            loading={candidatesLoading}
                            enablingId={enablingId}
                            onChange={(onebotMessengerBotIds) =>
                                onDraftChange({ onebotMessengerBotIds })
                            }
                            onEnsureHttp={onEnsureHttp}
                        />
                    </div>

                    {/* 右：接收目标（上）+ 消息内容（下） */}
                    <div className="flex min-h-0 min-w-0 flex-col gap-3 lg:h-full lg:overflow-hidden">
                        <section className="shrink-0 space-y-2.5 rounded-sm border border-border-subtle bg-field p-3">
                            <div className="flex items-center justify-between gap-2">
                                <p className="text-[13px] font-medium text-text">
                                    接收目标
                                </p>
                                {draft.onebotTargetIds.length > 0 ? (
                                    <Badge tone="neutral" appearance="soft">
                                        {draft.onebotTargetIds.length} 个
                                    </Badge>
                                ) : null}
                            </div>
                            <div className="space-y-2">
                                <div className="w-full max-w-[10rem]">
                                    <Select
                                        value={draft.onebotTargetType}
                                        onValueChange={(onebotTargetType) =>
                                            onDraftChange({ onebotTargetType })
                                        }
                                        items={[...ONEBOT_TARGET_ITEMS]}
                                    />
                                </div>
                                <TargetIdChipsInput
                                    values={draft.onebotTargetIds}
                                    className="min-h-[6.5rem] max-h-[9.5rem]"
                                    placeholder={
                                        draft.onebotTargetType === 'group'
                                            ? '群号，逗号添加多个'
                                            : 'QQ 号，逗号添加多个'
                                    }
                                    onChange={(onebotTargetIds) =>
                                        onDraftChange({ onebotTargetIds })
                                    }
                                />
                            </div>
                        </section>

                        <div className="flex min-h-[16rem] min-w-0 flex-1 flex-col lg:min-h-0">
                            <OneBotMessageEditor
                                value={draft.onebotMessageTemplate}
                                onChange={(onebotMessageTemplate) =>
                                    onDraftChange({ onebotMessageTemplate })
                                }
                                onReset={() =>
                                    onDraftChange({
                                        onebotMessageTemplate:
                                            DEFAULT_ONEBOT_MESSAGE,
                                    })
                                }
                            />
                        </div>
                    </div>
                </div>

                <DialogFooter className="mt-3 shrink-0">
                    <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    <Button type="button" size="sm" onClick={onSave}>
                        完成
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
