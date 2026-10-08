// 删除 Webhook 通道确认 Dialog：只报名字，真正的删除由主文件过滤通道列表。

import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from '../../../../shared/ui';
import type { WebhookChannelDraft } from '../../../../core/domain/settings/offline-notify-defaults';
import { channelDisplayName } from './channel-utils';

export function DeleteChannelDialog({
    target,
    onClose,
    onConfirm,
}: {
    target: WebhookChannelDraft | null;
    onClose: () => void;
    onConfirm: () => void;
}) {
    return (
        <Dialog
            open={target !== null}
            onOpenChange={(open) => {
                if (!open) onClose();
            }}
        >
            <DialogContent size="sm">
                <DialogHeader>
                    <DialogTitle>删除此通道？</DialogTitle>
                    <DialogDescription>
                        {target
                            ? `将删除「${channelDisplayName(target)}」。写入草稿后需保存设置才会落盘。`
                            : '写入草稿后需保存设置才会落盘。'}
                    </DialogDescription>
                </DialogHeader>
                <DialogFooter>
                    <Button type="button" variant="ghost" size="sm" onClick={onClose}>
                        取消
                    </Button>
                    <Button type="button" variant="danger" size="sm" onClick={onConfirm}>
                        删除
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
