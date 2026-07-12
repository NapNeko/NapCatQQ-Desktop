// Desktop 协议门禁：启动时强制确认 + 关于页只读查看 + 关键操作兜底。
// 同意键为正文 content-hash（后端计算），应用升级正文不变则不重签。

import { useCallback, useRef, useState } from 'react';
import {
    desktopConsentService,
    type DesktopAgreementsPayload,
} from '../../core/services/desktop-consent.service';
import { requestExitApp } from '../../core/services/exit.service';
import { isTauri } from '../../core/ipc/transport';
import { pushInfoBar } from '../ui/globalInfoBarStore';

export type DesktopConsentMode = 'gate' | 'view';

type PendingAction = () => void | Promise<void>;

export function useDesktopConsentGate() {
    const [open, setOpen] = useState(false);
    const [mode, setMode] = useState<DesktopConsentMode>('gate');
    const [payload, setPayload] = useState<DesktopAgreementsPayload | null>(null);
    const [submitting, setSubmitting] = useState(false);
    /** 启动门禁未通过时为 true，主界面应视为不可用 */
    const [blocking, setBlocking] = useState(false);
    const pendingRef = useRef<PendingAction | null>(null);
    const bootstrappedRef = useRef(false);

    const exitApp = useCallback(async () => {
        if (!isTauri) return;
        try {
            await requestExitApp();
        } catch (err: unknown) {
            pushInfoBar({
                tone: 'danger',
                title: '无法退出应用',
                content: err instanceof Error ? err.message : String(err),
            });
        }
    }, []);

    /** 门禁下拒绝：关闭并退出（无账号体系，不能「稍后再说」继续用）。 */
    const decline = useCallback(async () => {
        if (submitting) return;
        setOpen(false);
        setBlocking(false);
        pendingRef.current = null;
        await exitApp();
    }, [exitApp, submitting]);

    const close = useCallback(() => {
        if (submitting) return;
        if (mode === 'gate') {
            void decline();
            return;
        }
        setOpen(false);
        pendingRef.current = null;
    }, [decline, mode, submitting]);

    const openViewer = useCallback(async () => {
        try {
            const next = await desktopConsentService.getAgreements();
            setMode('view');
            setPayload(next);
            pendingRef.current = null;
            setOpen(true);
        } catch (err: unknown) {
            pushInfoBar({
                tone: 'danger',
                title: '读取用户协议失败',
                content: err instanceof Error ? err.message : String(err),
            });
        }
    }, []);

    /**
     * 启动时调用一次：未同意则全屏门禁挡住主界面。
     * 已同意则放行。返回是否已放行。
     */
    const promptAtStartup = useCallback(async (): Promise<boolean> => {
        if (bootstrappedRef.current) {
            return !blocking;
        }
        bootstrappedRef.current = true;
        try {
            const next = await desktopConsentService.getAgreements();
            if (!next.consent_required) {
                setBlocking(false);
                return true;
            }
            setMode('gate');
            setPayload(next);
            pendingRef.current = null;
            setBlocking(true);
            setOpen(true);
            return false;
        } catch (err: unknown) {
            setMode('gate');
            setPayload(null);
            setBlocking(true);
            setOpen(true);
            pushInfoBar({
                tone: 'danger',
                title: '检查用户协议失败',
                content: err instanceof Error ? err.message : String(err),
            });
            return false;
        }
    }, [blocking]);

    /** 若已同意则直接执行 action；否则弹门禁，同意后再执行。返回是否当场已放行。 */
    const ensureConsent = useCallback(async (action?: PendingAction): Promise<boolean> => {
        try {
            const next = await desktopConsentService.getAgreements();
            if (!next.consent_required) {
                if (action) await action();
                return true;
            }
            setMode('gate');
            setPayload(next);
            pendingRef.current = action ?? null;
            setBlocking(true);
            setOpen(true);
            return false;
        } catch (err: unknown) {
            pushInfoBar({
                tone: 'danger',
                title: '检查用户协议失败',
                content: err instanceof Error ? err.message : String(err),
            });
            return false;
        }
    }, []);

    const accept = useCallback(async () => {
        if (!payload || submitting) return;
        setSubmitting(true);
        try {
            const next = await desktopConsentService.accept(payload.version);
            setPayload(next);
            setBlocking(false);
            setOpen(false);
            const pending = pendingRef.current;
            pendingRef.current = null;
            if (pending) await pending();
        } catch (err: unknown) {
            pushInfoBar({
                tone: 'danger',
                title: '保存协议同意失败',
                content: err instanceof Error ? err.message : String(err),
            });
        } finally {
            setSubmitting(false);
        }
    }, [payload, submitting]);

    return {
        open,
        mode,
        payload,
        submitting,
        blocking,
        promptAtStartup,
        ensureConsent,
        openViewer,
        accept,
        decline,
        close,
    };
}
