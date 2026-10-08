// 批量删除的二次确认对话框。删除是不可逆操作且会先停运行中的 Bot，
// 从页面主组件搬出，页面只掌握开合与确认回调。

import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogTitle,
} from '../../../../shared/ui';

export function BatchDeleteConfirmDialog({
    open,
    onOpenChange,
    selectedCount,
    busy,
    onConfirm,
}: {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    selectedCount: number;
    busy: boolean;
    onConfirm: () => void;
}) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent size="md">
                <DialogTitle>确认批量删除选中实例？</DialogTitle>
                <DialogDescription>
                    将删除选中的 {selectedCount} 个 Bot 实例的配置文件与数据项。 若有正在运行的
                    Bot，会先自动停止再删除。此操作不可撤销。
                </DialogDescription>
                <DialogFooter>
                    <Button variant="ghost" onClick={() => onOpenChange(false)} disabled={busy}>
                        取消
                    </Button>
                    <Button
                        variant="primary"
                        onClick={onConfirm}
                        disabled={busy}
                        className="bg-danger hover:bg-danger/90"
                    >
                        彻底删除
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
