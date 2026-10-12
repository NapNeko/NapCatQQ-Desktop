// 扫码材料只在可见弹窗中展示，手机确认和登录终态不保留二维码。
import { useCallback, useEffect, useState } from 'react';
import QRCode from 'qrcode';
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

const LABELS: Record<SnowLumaQrLoginState, string> = {
    disabled: '简化扫码已关闭',
    unavailable: '请打开 QQ 界面继续登录',
    starting: '正在等待 QQ 登录窗口',
    waiting_for_qr: '请用手机 QQ 扫码',
    scanned: '已扫码，请在手机上确认',
    expired: '二维码已失效，请刷新',
    logged_in: '登录成功，二维码已移除',
    login_detected: '正在确认 SL 登录状态',
    cancelled: '扫码已结束',
    account_mismatch: '登录账号与 Bot 不一致',
    cleanup_failed: '扫码通道关闭失败，请停止此 Bot',
};

function LoginQrImage({ payload, onError }: { payload: string; onError: () => void }) {
    const [src, setSrc] = useState<string | null>(null);
    useEffect(() => {
        let active = true;
        setSrc(null);
        if (payload.startsWith('data:image/png;base64,')) setSrc(payload);
        else {
            void QRCode.toDataURL(payload, { width: 280, margin: 4, errorCorrectionLevel: 'M' })
                .then((image) => {
                    if (active) setSrc(image);
                })
                .catch(() => {
                    if (active) onError();
                });
        }
        return () => {
            active = false;
        };
    }, [onError, payload]);
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
    const { snapshot, error, refreshing, requesting, paused, refresh } = useRemoteSnowlumaQrLogin(
        botId,
        open && !isOnline,
        api,
        initialRefresh,
    );
    useEffect(() => {
        if (open && isOnline) onOpenChange(false);
    }, [isOnline, open, onOpenChange]);
    useEffect(() => setImageUnavailable(false), [snapshot?.qrcode_payload]);
    const state = snapshot?.state;
    const canRefresh = !paused && (state === 'waiting_for_qr' || state === 'expired');
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
                            <div className="overflow-hidden rounded-md bg-white">
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
                <DialogFooter>
                    {onOpenVnc && (
                        <Button
                            variant="secondary"
                            onClick={() => {
                                onOpenChange(false);
                                onOpenVnc();
                            }}
                        >
                            打开 QQ 界面
                        </Button>
                    )}
                    <Button
                        variant="primary"
                        disabled={!canRefresh || refreshing || requesting}
                        onClick={() => void refresh()}
                    >
                        {refreshing ? '正在刷新' : '刷新二维码'}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
