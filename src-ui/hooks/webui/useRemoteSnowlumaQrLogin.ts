// 二维码只存在于打开的弹窗中，不进入 Query 缓存。
import { useCallback, useEffect, useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { snowlumaService } from '../../core/services/desktop.service';
import { snowlumaAppService } from '../../core/services/snowlumaApp.service';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import type { SnowLumaQrLoginSnapshot } from '../../core/ipc/generated/domain/SnowLumaQrLoginSnapshot';

export const REMOTE_SL_QR_ENABLED_KEY = ['snowluma-remote-qr-enabled'] as const;
export const LOCAL_SL_QR_ENABLED_KEY = ['snowluma-local-qr-enabled'] as const;

export function useRemoteSnowlumaQrEnabled(eligible: boolean, local = false) {
    return (
        useQuery({
            queryKey: local ? LOCAL_SL_QR_ENABLED_KEY : REMOTE_SL_QR_ENABLED_KEY,
            queryFn: local
                ? async () => (await snowlumaAppService.get()).snowlumaLocalQrLoginEnabled
                : snowlumaService.remoteQrLoginEnabled,
            enabled: eligible,
            staleTime: 5000,
        }).data === true && eligible
    );
}

export interface RemoteQrLoginApi {
    qrLoginSnapshot: (botId: string, refresh?: boolean) => Promise<SnowLumaQrLoginSnapshot>;
    closeQrLogin: (botId: string) => Promise<void>;
}

const POLLING_STATES = new Set([
    'starting',
    'waiting_for_qr',
    'scanned',
    'expired',
    'login_detected',
]);
export const SL_QR_REQUEST_TIMEOUT_MS = 25000;
export const SL_QR_SESSION_TIMEOUT_MS = 10 * 60 * 1000;

function isPageVisible() {
    return typeof document === 'undefined' || document.visibilityState !== 'hidden';
}

export function useRemoteSnowlumaQrLogin(
    botId: string,
    open: boolean,
    api: RemoteQrLoginApi = snowlumaService,
    initialRefresh = false,
) {
    const [snapshot, setSnapshot] = useState<SnowLumaQrLoginSnapshot | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [refreshing, setRefreshing] = useState(false);
    const [requesting, setRequesting] = useState(false);
    const [visible, setVisible] = useState(isPageVisible);
    const [attempt, setAttempt] = useState(0);
    const epoch = useRef(0);
    const current = useRef({ botId, open });
    current.current = { botId, open };
    const requestRef = useRef<((refresh: boolean) => Promise<void>) | null>(null);
    const session = useRef({
        botId,
        open: false,
        startedAt: 0,
        initialized: false,
        scanned: false,
        loginDetected: false,
        qqPid: null as number | null,
        attempt: 0,
        forceRefresh: false,
    });

    useEffect(() => {
        const update = () => setVisible(isPageVisible());
        document.addEventListener('visibilitychange', update);
        return () => document.removeEventListener('visibilitychange', update);
    }, []);

    useEffect(() => {
        const generation = ++epoch.current;
        let active = open;
        let pending = false;
        let timer: ReturnType<typeof setTimeout> | undefined;
        let requestTimer: ReturnType<typeof setTimeout> | undefined;
        let cancelPending: (() => void) | undefined;
        let terminal = false;
        let refreshExpired = false;
        let expiredRefreshAttempts = 0;
        let closeRequested = false;
        if (
            session.current.botId !== botId ||
            session.current.open !== open ||
            session.current.attempt !== attempt
        ) {
            const forceRefresh =
                session.current.botId === botId &&
                session.current.open === open &&
                session.current.attempt !== attempt;
            session.current = {
                botId,
                open,
                startedAt: Date.now(),
                initialized: false,
                scanned: false,
                loginDetected: false,
                qqPid: null,
                attempt,
                forceRefresh,
            };
            setSnapshot(null);
        } else {
            setSnapshot((previous) => (previous ? { ...previous, qrcode_payload: null } : null));
        }
        setError(null);
        setRefreshing(false);
        setRequesting(false);
        if (!open || !visible) return;

        const close = () => {
            if (closeRequested) return;
            closeRequested = true;
            void api.closeQrLogin(botId).catch(() => {
                if (active && generation === epoch.current) {
                    setSnapshot({
                        v: 1,
                        state: 'cleanup_failed',
                        qq_pid: null,
                        qrcode_payload: null,
                        uin: null,
                        observer_removed: false,
                        message: null,
                    });
                    setError(null);
                }
                pushInfoBar({
                    tone: 'warning',
                    title: '扫码通道未确认关闭',
                    content: `Bot ${botId}：请停止此 Bot 清理扫码会话`,
                    autoDismissMs: 8000,
                });
            });
        };

        const expire = () => {
            if (!active || generation !== epoch.current) return;
            terminal = true;
            clearTimeout(timer);
            cancelPending?.();
            setSnapshot(null);
            setRequesting(false);
            setRefreshing(false);
            setError('扫码等待已超时，请重新打开扫码窗口或在 QQ 界面继续登录');
            close();
        };
        const remaining = SL_QR_SESSION_TIMEOUT_MS - (Date.now() - session.current.startedAt);
        if (remaining <= 0) {
            expire();
            return () => {
                active = false;
            };
        }
        const leaseTimer = setTimeout(expire, remaining);

        const request = async (refresh: boolean) => {
            if (!active || pending || terminal || !isPageVisible()) return;
            if (refresh && (session.current.scanned || session.current.loginDetected)) return;
            pending = true;
            session.current.initialized = true;
            setRequesting(true);
            if (refresh) setRefreshing(true);
            if (refresh)
                setSnapshot((previous) =>
                    previous ? { ...previous, qrcode_payload: null } : null,
                );
            clearTimeout(timer);
            try {
                const next = await Promise.race([
                    api.qrLoginSnapshot(botId, refresh),
                    new Promise<never>((_, reject) => {
                        cancelPending = () => reject(new Error('qr_request_cancelled'));
                        requestTimer = setTimeout(
                            () => reject(new Error('qr_request_timeout')),
                            SL_QR_REQUEST_TIMEOUT_MS,
                        );
                    }),
                ]);
                if (!active || terminal || generation !== epoch.current || !isPageVisible()) return;
                if (next.qq_pid !== null) {
                    // QQ 退出账号可能更换进程，旧进程的手机确认不能锁住新扫码。
                    if (session.current.qqPid !== null && session.current.qqPid !== next.qq_pid) {
                        session.current.scanned = false;
                        session.current.loginDetected = false;
                        expiredRefreshAttempts = 0;
                    }
                    session.current.qqPid = next.qq_pid;
                }
                if (next.state === 'scanned') session.current.scanned = true;
                if (next.state === 'login_detected') session.current.loginDetected = true;
                if (next.state === 'expired') session.current.scanned = false;
                terminal = !POLLING_STATES.has(next.state);
                const state =
                    session.current.loginDetected && !terminal
                        ? 'login_detected'
                        : session.current.scanned && !terminal
                          ? 'scanned'
                          : next.state;
                refreshExpired = state === 'expired';
                if (refreshExpired && refresh) expiredRefreshAttempts += 1;
                if (state === 'waiting_for_qr' && next.qrcode_payload) expiredRefreshAttempts = 0;
                setSnapshot({
                    ...next,
                    state,
                    qrcode_payload:
                        state === 'waiting_for_qr' &&
                        !session.current.scanned &&
                        !session.current.loginDetected
                            ? next.qrcode_payload
                            : null,
                });
                setError(null);
                if (terminal) {
                    clearTimeout(leaseTimer);
                    if (!next.observer_removed && next.state !== 'cleanup_failed') close();
                }
            } catch {
                if (active && !terminal && generation === epoch.current && isPageVisible()) {
                    terminal = true;
                    setSnapshot(null);
                    setError('扫码通道未响应，请重新打开扫码窗口或在 QQ 界面继续登录');
                    clearTimeout(leaseTimer);
                    close();
                }
            } finally {
                clearTimeout(requestTimer);
                cancelPending = undefined;
                pending = false;
                if (active && generation === epoch.current) {
                    setRefreshing(false);
                    setRequesting(false);
                    if (!terminal && isPageVisible()) {
                        // QQ 换码尚未完成时退避，避免反复点击；有效二维码只做状态查询。
                        const delay = refreshExpired
                            ? Math.min(1000 * 2 ** expiredRefreshAttempts, 30000)
                            : 1000;
                        timer = setTimeout(() => void request(refreshExpired), delay);
                    }
                }
            }
        };
        requestRef.current = request;
        queueMicrotask(() => {
            if (active && generation === epoch.current)
                void request(
                    (initialRefresh || session.current.forceRefresh) &&
                        !session.current.initialized,
                );
        });
        return () => {
            active = false;
            clearTimeout(timer);
            cancelPending?.();
            clearTimeout(requestTimer);
            clearTimeout(leaseTimer);
            requestRef.current = null;
            // StrictMode 的重新挂载不得取消刚建立的远端会话。
            // eslint-disable-next-line react-hooks/exhaustive-deps -- 此处需要读取最新会话代次。
            const isCurrent = () => epoch.current === generation;
            queueMicrotask(() => {
                if (isCurrent() || !current.current.open || current.current.botId !== botId) {
                    close();
                }
            });
        };
    }, [api, botId, open, initialRefresh, visible, attempt]);

    const refresh = useCallback(() => requestRef.current?.(true), []);
    const canRetry =
        open &&
        visible &&
        (snapshot?.state === 'unavailable' || snapshot?.state === 'cancelled') &&
        snapshot.observer_removed &&
        !session.current.scanned &&
        !session.current.loginDetected;
    const retry = useCallback(() => {
        if (canRetry && !requesting) setAttempt((value) => value + 1);
    }, [canRetry, requesting]);
    return {
        snapshot: open && visible ? snapshot : null,
        error: open ? error : null,
        refreshing,
        requesting,
        paused: open && !visible,
        refresh,
        canRetry,
        retry,
    };
}
