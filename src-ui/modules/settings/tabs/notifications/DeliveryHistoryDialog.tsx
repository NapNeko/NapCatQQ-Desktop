// 掉线投递历史 Dialog（内存记录，重启清空）

import {
    Badge,
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from '../../../../shared/ui';
import { settingsService } from '../../../../core/services/settings.service';

type HistoryItem = Awaited<
    ReturnType<typeof settingsService.listOfflineDeliveryHistory>
>[number];

function historyKindLabel(kind: string): string {
    switch (kind) {
        case 'auto_restart':
        case 'manual':
            return '掉线';
        case 'kicked':
            return '被踢下线';
        case 'process_crashed':
            return '异常退出';
        case 'recovered':
            return '恢复上线';
        default:
            return kind;
    }
}

function DeliveryResultBadge({
    label,
    value,
}: {
    label: string;
    value: 'ok' | 'failed' | 'skipped';
}) {
    if (value === 'ok') {
        return <Badge tone="success" appearance="soft">{label} 已发送</Badge>;
    }
    if (value === 'failed') {
        return <Badge tone="danger" appearance="soft">{label} 失败</Badge>;
    }
    return <Badge tone="neutral" appearance="soft">{label} 未投递</Badge>;
}

export function DeliveryHistoryDialog({
    open,
    history,
    loading,
    onOpenChange,
    onRefresh,
    onClear,
}: {
    open: boolean;
    history: HistoryItem[];
    loading: boolean;
    onOpenChange: (open: boolean) => void;
    onRefresh: () => void;
    onClear: () => void;
}) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent size="lg" dismissOnOutsideClick={false} hideClose>
                <DialogHeader>
                    <div className="flex min-w-0 items-center gap-2">
                        <DialogTitle>最近投递</DialogTitle>
                        <Badge tone="neutral" appearance="soft">
                            {history.length} 条
                        </Badge>
                    </div>
                    <DialogDescription>
                        本次运行的通知结果；重启应用后自动清空。
                    </DialogDescription>
                </DialogHeader>

                <div className="min-h-0 overflow-hidden rounded-sm border border-border-subtle bg-field">
                    {history.length === 0 ? (
                        <div className="flex min-h-48 flex-col justify-center px-5 py-8 text-center">
                            <p className="text-[13px] font-medium text-text-secondary">
                                暂无投递记录
                            </p>
                            <p className="mx-auto mt-1.5 max-w-sm text-[12px] leading-relaxed text-text-tertiary">
                                Bot 掉线、恢复或异常退出后，这里会记录桌面通知和各推送通道的结果。
                            </p>
                        </div>
                    ) : (
                        <ul className="max-h-[min(55vh,32rem)] divide-y divide-border-subtle/70 overflow-y-auto">
                            {history.map((h, i) => (
                                <li
                                    key={`${h.at}-${h.bot_id}-${i}`}
                                    className="space-y-2 px-4 py-3"
                                >
                                    <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1">
                                        <div className="flex min-w-0 items-center gap-2">
                                            <span className="truncate text-[13px] font-medium text-text">
                                                {h.botName || h.bot_id}
                                            </span>
                                            <Badge
                                                tone={h.debounced ? 'warning' : 'neutral'}
                                                appearance="soft"
                                            >
                                                {h.debounced
                                                    ? '已合并重复提醒'
                                                    : historyKindLabel(h.kind)}
                                            </Badge>
                                        </div>
                                        <span className="shrink-0 text-[11px] tabular-nums text-text-tertiary">
                                            {h.at}
                                        </span>
                                    </div>
                                    {!h.debounced ? (
                                        <div className="flex flex-wrap gap-1.5">
                                            <DeliveryResultBadge label="桌面" value={h.toast} />
                                            <DeliveryResultBadge label="Webhook" value={h.webhook} />
                                            <DeliveryResultBadge label="邮件" value={h.email} />
                                            <DeliveryResultBadge label="OneBot" value={h.onebot} />
                                        </div>
                                    ) : null}
                                    {h.note ? (
                                        <p className="text-[12px] leading-relaxed text-text-tertiary">
                                            {h.note}
                                        </p>
                                    ) : null}
                                </li>
                            ))}
                        </ul>
                    )}
                </div>

                <DialogFooter>
                    <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        disabled={loading}
                        onClick={onRefresh}
                    >
                        {loading ? '刷新中…' : '刷新'}
                    </Button>
                    <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        disabled={history.length === 0}
                        onClick={onClear}
                    >
                        清空记录
                    </Button>
                    <Button
                        type="button"
                        size="sm"
                        onClick={() => onOpenChange(false)}
                    >
                        完成
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
