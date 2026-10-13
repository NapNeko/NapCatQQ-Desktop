// 扫码材料只在可见弹窗中展示，手机确认和登录终态不保留二维码。
import { useCallback, useEffect, useState } from 'react';
import QRCode from 'qrcode';
import { Check, Monitor, RefreshCw, X } from 'lucide-react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogTitle,
    DialogDescription,
    DialogHeader,
    DialogFooter,
} from '../../../../shared/ui';
import {
    useRemoteSnowlumaQrLogin,
    type RemoteQrLoginApi,
} from '../../../../hooks/webui/useRemoteSnowlumaQrLogin';
import type { SnowLumaQrLoginState } from '../../../../core/ipc/generated/domain/SnowLumaQrLoginState';
import { useThemeTokens } from '../../../../hooks/theme/useThemeTokens';
import { resolveLoginQrPayload } from './loginQrImage';

const QR_COLORS = {
    foreground: { name: '--qr-foreground', fallback: '#1a120d' },
    background: { name: '--qr-background', fallback: '#ffffff' },
} as const;

const LABELS: Record<SnowLumaQrLoginState, string> = {
    disabled: '简化扫码已关闭',
    unavailable: '请打开 QQ 界面继续登录',
    starting: '正在等待 QQ 登录窗口',
    waiting_for_qr: '请用手机 QQ 扫码',
    scanned: '已扫码，请在手机上确认',
    expired: '二维码已失效，正在自动刷新',
    logged_in: '登录成功，二维码已移除',
    login_detected: '正在确认 SL 登录状态',
    cancelled: '扫码已结束',
    account_mismatch: '登录账号与 Bot 不一致',
    cleanup_failed: '扫码通道关闭失败，请停止此 Bot',
};

function LoginQrImage({ payload, onError }: { payload: string; onError: () => void }) {
    const [src, setSrc] = useState<string | null>(null);
    const { foreground, background } = useThemeTokens(QR_COLORS);
    useEffect(() => {
        let active = true;
        const controller = new AbortController();
        setSrc(null);
        void resolveLoginQrPayload(payload, controller.signal)
            .then(async (content) => {
                if (!active) return;
                const image = await QRCode.toDataURL(content, {
                    width: 280,
                    margin: 4,
                    errorCorrectionLevel: 'M',
                    color: { dark: foreground, light: background },
                });
                if (active) setSrc(image);
            })
            .catch(() => {
                if (active) onError();
            });
        return () => {
            active = false;
            controller.abort();
        };
    }, [background, foreground, onError, payload]);
    return src ? (
        <img src={src} alt="QQ 登录二维码" width={280} height={280} onError={onError} />
    ) : null;
}

interface Props {
    botId: string;
    open: boolean;
    onOpenChange: (open: boolean) => void;
    isOnline: boolean;
    onOpenVnc?: () => void;
    api?: RemoteQrLoginApi;
    initialRefresh?: boolean;
}

export function RemoteSnowLumaQrLoginDialog({
    botId,
    open,
    onOpenChange,
    isOnline,
    onOpenVnc,
    api,
    initialRefresh = false,
}: Props) {
    const [imageUnavailable, setImageUnavailable] = useState(false);
    const imageError = useCallback(() => setImageUnavailable(true), []);
    const { snapshot, error, refreshing, requesting, paused, refresh, canRetry, retry } =
        useRemoteSnowlumaQrLogin(botId, open && !isOnline, api, initialRefresh);
    useEffect(() => {
        if (open && isOnline) onOpenChange(false);
    }, [isOnline, open, onOpenChange]);
    useEffect(() => setImageUnavailable(false), [snapshot?.qrcode_payload]);
    const state = snapshot?.state;
    const canRefresh = !paused && (canRetry || state === 'waiting_for_qr' || state === 'expired');
    const completed = isOnline || state === 'logged_in';
    const fallbackOnly =
        Boolean(error) ||
        state === 'disabled' ||
        state === 'unavailable' ||
        state === 'cancelled' ||
        state === 'account_mismatch' ||
        state === 'cleanup_failed';
    const confirmation =
        state === 'scanned' ||
        state === 'login_detected' ||
        state === 'logged_in' ||
        state === 'cleanup_failed';
    const status = isOnline
        ? LABELS.logged_in
        : (error ??
          (paused
              ? '扫码已暂停'
              : confirmation && state
                ? LABELS[state]
                : imageUnavailable
                  ? '二维码无法显示，请刷新或在 QQ 界面继续登录'
                  : (refreshing || (requesting && !snapshot)) && !snapshot?.qrcode_payload
                    ? '正在获取二维码'
                    : state === 'waiting_for_qr' && !snapshot?.qrcode_payload
                      ? '正在等待 QQ 生成二维码'
                      : (snapshot?.message ?? (state ? LABELS[state] : '正在加载扫码状态'))));
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent size="sm">
                <DialogHeader>
                    <DialogTitle>扫码登录 · {botId}</DialogTitle>
                    <DialogDescription className="sr-only">
                        使用手机 QQ 扫描二维码并确认登录。
                    </DialogDescription>
                </DialogHeader>
                <div
                    className="flex min-h-80 flex-col items-center justify-center gap-4 py-4"
                    aria-busy={requesting}
                >
                    {open &&
                        !isOnline &&
                        !paused &&
                        !refreshing &&
                        state === 'waiting_for_qr' &&
                        snapshot?.qrcode_payload && (
                            <div className="overflow-hidden rounded-md bg-[var(--qr-background)]">
                                <LoginQrImage
                                    key={snapshot.qrcode_payload}
                                    payload={snapshot.qrcode_payload}
                                    onError={imageError}
                                />
                            </div>
                        )}
                    <p role="status" className="text-center text-sm text-text-secondary">
                        {status}
                    </p>
                </div>
                <DialogFooter className="mt-4 justify-between gap-3 border-t border-border-subtle pt-4">
                    {onOpenVnc && !completed ? (
                        <Button
                            variant={fallbackOnly ? 'primary' : 'ghost'}
                            size="md"
                            onClick={() => {
                                onOpenChange(false);
                                onOpenVnc();
                            }}
                        >
                            <Monitor size={16} aria-hidden />
                            打开 QQ 界面
                        </Button>
                    ) : null}
                    {completed || (fallbackOnly && !canRetry) ? (
                        <Button
                            variant={completed ? 'primary' : 'secondary'}
                            size="md"
                            className="ml-auto min-w-28"
                            onClick={() => onOpenChange(false)}
                        >
                            {completed ? (
                                <Check size={16} aria-hidden />
                            ) : (
                                <X size={16} aria-hidden />
                            )}
                            {completed ? '完成' : '关闭'}
                        </Button>
                    ) : (
                        <Button
                            variant={state === 'expired' ? 'primary' : 'secondary'}
                            size="md"
                            className="ml-auto min-w-28"
                            disabled={!canRefresh || refreshing || requesting}
                            onClick={() => (canRetry ? retry() : void refresh())}
                        >
                            <RefreshCw size={16} aria-hidden />
                            {refreshing ? '正在刷新' : canRetry ? '重新获取' : '刷新二维码'}
                        </Button>
                    )}
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
