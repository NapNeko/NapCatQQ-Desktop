// 彻底删除 Bot 的二次确认框。从配置页壳外提，删除动作由调用方执行。

import {
    Button,
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from '../../../../shared/ui';

interface DeleteBotConfirmDialogProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    botId: string | null;
    onDelete: () => void;
    isDeleting: boolean;
}

export function DeleteBotConfirmDialog({
    open,
    onOpenChange,
    botId,
    onDelete,
    isDeleting,
}: DeleteBotConfirmDialogProps) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent size="sm">
                <DialogHeader>
                    <DialogTitle>彻底删除该 Bot？</DialogTitle>
                    <DialogDescription>
                        将永久删除 Bot {botId}{' '}
                        的全部配置与数据，运行中的进程会被强制停止。此操作不可撤销。
                    </DialogDescription>
                </DialogHeader>
                <DialogFooter>
                    <Button variant="ghost" size="sm" onClick={() => onOpenChange(false)}>
                        取消
                    </Button>
                    <Button variant="danger" size="sm" onClick={onDelete} disabled={isDeleting}>
                        {isDeleting ? '删除中…' : '彻底删除'}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
