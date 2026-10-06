// 本机 Bot 要用用户自己装的 QQ 时的启动前提醒。
// 只提醒不拦启动：关掉对话框等于不启动，「继续启动」照常走后面的流程。
import { useEffect, useState } from 'react';
import {
    Button,
    Checkbox,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from '../../../shared/ui';

interface SystemQqWarningDialogProps {
    open: boolean;
    /** dismissForever 为 true 时调用方负责记下「不再提醒」 */
    onContinue: (dismissForever: boolean) => void;
    onInstall: (dismissForever: boolean) => void;
    onCancel: () => void;
}

export function SystemQqWarningDialog({
    open,
    onContinue,
    onInstall,
    onCancel,
}: SystemQqWarningDialogProps) {
    // 每次打开重新问一遍：勾了「不再提醒」又反悔去点继续，不该把勾选记住
    const [dismissForever, setDismissForever] = useState(false);
    useEffect(() => {
        if (open) setDismissForever(false);
    }, [open]);

    return (
        <Dialog open={open} onOpenChange={(next) => !next && onCancel()}>
            <DialogContent size="md">
                <DialogHeader>
                    <DialogTitle>这个 Bot 会用你自己装的 QQ</DialogTitle>
                    <DialogDescription>
                        Bot 和你平时用的 QQ 共用同一份程序。QQ 自动更新后可能和框架版本对不上，Bot
                        会起不来；你重装或卸载 QQ 也会连带影响 Bot。
                    </DialogDescription>
                </DialogHeader>

                <div className="space-y-3">
                    <p className="text-sm text-text-secondary">
                        可以在组件页给 Bot 单独装一份 QQ，只解压到桌面端数据目录，不动你自己的 QQ。
                    </p>
                    <Checkbox
                        id="system-qq-warning-dismiss"
                        label="不再提醒"
                        checked={dismissForever}
                        onCheckedChange={setDismissForever}
                    />
                </div>

                <DialogFooter>
                    <Button variant="ghost" onClick={() => onContinue(dismissForever)}>
                        继续启动
                    </Button>
                    <Button variant="primary" onClick={() => onInstall(dismissForever)}>
                        去装一份
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
