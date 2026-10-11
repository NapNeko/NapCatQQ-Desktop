// 退出闸门：关窗选了「退出」、托盘退出被拦下时，先问后端本机还有没有 Bot 在跑，
// 再决定弹「退出程序？」还是「无法退出」。

import { useCallback, useEffect, useState } from 'react';
import { isTauri } from '../../core/domain/runtime/env';
import { windowEventService } from '../../core/services/desktop.service';
import {
    consumePendingTrayExit,
    prepareExitDesktop,
    requestExitApp,
} from '../../core/services/exit.service';
import type { PrepareExitDesktopResponse } from '../../core/ipc/types';

export type ExitDialogMode = 'confirm' | 'blocked';

export interface DesktopExitGate {
    open: boolean;
    mode: ExitDialogMode;
    stats: PrepareExitDesktopResponse | null;
    exiting: boolean;
    setOpen: (open: boolean) => void;
    confirmExit: () => void;
}

// StrictMode 重新挂载时，已从后端领取的通知要留给下一份监听。
let claimedTrayExit = false;
let claimingTrayExit: Promise<void> | null = null;
async function claimTrayExit() {
    if (claimedTrayExit) return;
    claimingTrayExit ??= consumePendingTrayExit()
        .then((pending) => {
            claimedTrayExit ||= pending;
        })
        .finally(() => {
            claimingTrayExit = null;
        });
    await claimingTrayExit;
}

export function useDesktopExitGate(): DesktopExitGate {
    const [open, setOpen] = useState(false);
    const [mode, setMode] = useState<ExitDialogMode>('confirm');
    const [stats, setStats] = useState<PrepareExitDesktopResponse | null>(null);
    const [exiting, setExiting] = useState(false);

    const runExitFlow = useCallback(async () => {
        try {
            const prep = await prepareExitDesktop();
            setStats(prep);
            setMode(prep.can_exit ? 'confirm' : 'blocked');
            setOpen(true);
        } catch (err) {
            console.error('prepare_exit_desktop failed:', err);
        }
    }, []);

    useEffect(() => {
        if (!isTauri) return;
        let cancelled = false;
        const unsubs: Array<() => void> = [];
        // 监听是异步挂上的，卸载赶在挂好之前（StrictMode 双挂载）就当场退掉，免得漏一份没人退
        const keep = (unsub: () => void) => {
            if (cancelled) unsub();
            else unsubs.push(unsub);
        };
        const onSignal = () => void runExitFlow();
        const onListenFailed = (err: unknown) => console.error('退出闸门监听窗口通知失败:', err);
        const consumeTrayExit = async () => {
            if (cancelled) return;
            await claimTrayExit();
            if (cancelled || !claimedTrayExit) return;
            claimedTrayExit = false;
            await runExitFlow();
        };
        const onTraySignal = () => void consumeTrayExit().catch(onListenFailed);
        Promise.all([
            windowEventService.onRequestClose(onSignal).then(keep),
            windowEventService.onExitBlocked(onTraySignal).then(keep),
        ]).then(() => {
            if (!cancelled) void consumeTrayExit().catch(onListenFailed);
        }, onListenFailed);
        return () => {
            cancelled = true;
            for (const u of unsubs) u();
        };
    }, [runExitFlow]);

    const confirmExit = useCallback(() => {
        setExiting(true);
        requestExitApp().catch((err: unknown) => {
            console.error('request_exit_app failed:', err);
            setExiting(false);
            void runExitFlow();
        });
    }, [runExitFlow]);

    return { open, mode, stats, exiting, setOpen, confirmExit };
}
