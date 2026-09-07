// 配置版本冲突对话框：读取后文件被别处改过（Karin WebUI / 手改），让用户选重载还是覆盖。

import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from '../../../shared/ui';

export const ConfigConflictDialog: React.FC<{
    open: boolean;
    busy?: boolean;
    what?: string;
    onReload: () => void;
    onOverwrite: () => void;
    onCancel: () => void;
}> = ({ open, busy, what = '配置', onReload, onOverwrite, onCancel }) => (
    <Dialog open={open} onOpenChange={(o) => !o && !busy && onCancel()}>
        <DialogContent size="sm" dismissOnOutsideClick={!busy}>
            <DialogHeader>
                <DialogTitle>{what}已被别处修改</DialogTitle>
                <DialogDescription>
                    重新加载会丢掉这边的改动；覆盖会盖掉别处的改动。
                </DialogDescription>
            </DialogHeader>
            <DialogFooter>
                <Button variant="ghost" size="sm" onClick={onCancel} disabled={busy}>
                    取消
                </Button>
                <Button variant="secondary" size="sm" onClick={onReload} disabled={busy}>
                    重新加载
                </Button>
                <Button variant="danger" size="sm" onClick={onOverwrite} disabled={busy}>
                    覆盖
                </Button>
            </DialogFooter>
        </DialogContent>
    </Dialog>
);
