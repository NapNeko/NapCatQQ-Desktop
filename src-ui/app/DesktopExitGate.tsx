// 标题栏关闭 / 托盘退出：本机 Bot 须先停；允许退出时远端保持运行。

import React from 'react';
import { useDesktopExitGate } from '../hooks/desktop/useDesktopExitGate';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from '../shared/ui';

export const DesktopExitGate: React.FC = () => {
    const { open, mode, stats, exiting, setOpen, confirmExit } = useDesktopExitGate();

    if (!open || !stats) return null;

    const remoteHint =
        stats.remote_active > 0
            ? `退出后仍有 ${stats.remote_active} 个远端 Bot 在运行，下次打开可恢复状态。`
            : null;

    if (mode === 'blocked') {
        return (
            <Dialog open={open} onOpenChange={setOpen}>
                <DialogContent size="sm">
                    <DialogHeader>
                        <DialogTitle>无法退出</DialogTitle>
                        <DialogDescription>
                            有 {stats.local_active} 个本机 Bot 正在运行，请先在 Bot
                            列表中停止后再退出。
                            {remoteHint ? ` ${remoteHint}` : ''}
                        </DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                        <Button type="button" onClick={() => setOpen(false)}>
                            知道了
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        );
    }

    return (
        <Dialog open={open} onOpenChange={setOpen}>
            <DialogContent size="sm">
                <DialogHeader>
                    <DialogTitle>退出程序？</DialogTitle>
                    <DialogDescription>
                        将关闭 NapCatQQ Desktop。
                        {remoteHint ? ` ${remoteHint}` : ''}
                    </DialogDescription>
                </DialogHeader>
                <DialogFooter>
                    <Button type="button" onClick={() => setOpen(false)}>
                        取消
                    </Button>
                    <Button type="button" disabled={exiting} onClick={confirmExit}>
                        {exiting ? '正在退出…' : '退出'}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
};
