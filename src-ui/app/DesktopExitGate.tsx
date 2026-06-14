// 标题栏关闭 / 托盘退出：本机 Bot 须先停；允许退出时远端保持运行。

import React, { useCallback, useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { isTauri } from '../core/ipc/transport';
import {
    prepareExitDesktop,
    requestExitApp,
    type PrepareExitDesktopResponse,
} from '../core/services/exit.service';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from '../shared/ui';

type ExitDialogMode = 'confirm' | 'blocked';

export const DesktopExitGate: React.FC = () => {
    const [open, setOpen] = useState(false);
    const [mode, setMode] = useState<ExitDialogMode>('confirm');
    const [stats, setStats] = useState<PrepareExitDesktopResponse | null>(null);
    const [exiting, setExiting] = useState(false);

    const runExitFlow = useCallback(async () => {
        if (!isTauri) return;
        try {
            const prep = await prepareExitDesktop();
            setStats(prep);
            if (!prep.can_exit) {
                setMode('blocked');
                setOpen(true);
                return;
            }
            setMode('confirm');
            setOpen(true);
        } catch (err) {
            console.error('prepare_exit_desktop failed:', err);
        }
    }, []);

    useEffect(() => {
        if (!isTauri) return;
        const unsubs: Array<() => void> = [];
        void (async () => {
            unsubs.push(
                await listen('desktop-request-close', () => {
                    void runExitFlow();
                }),
            );
            unsubs.push(
                await listen<number>('desktop-exit-blocked', () => {
                    void runExitFlow();
                }),
            );
        })();
        return () => {
            for (const u of unsubs) u();
        };
    }, [runExitFlow]);

    const handleConfirmExit = async () => {
        setExiting(true);
        try {
            await requestExitApp();
        } catch (err) {
            console.error('request_exit_app failed:', err);
            setExiting(false);
            void runExitFlow();
        }
    };

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
                    <Button
                        type="button"
                        disabled={exiting}
                        onClick={() => void handleConfirmExit()}
                    >
                        {exiting ? '正在退出…' : '退出'}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
};