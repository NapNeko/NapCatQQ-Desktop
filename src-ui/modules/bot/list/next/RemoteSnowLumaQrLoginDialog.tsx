// 扫码材料只在可见弹窗中展示，手机确认和登录终态不保留二维码。
import { useCallback, useEffect, useState } from 'react';
import QRCode from 'qrcode';
import { Check, Monitor, RefreshCw, X } from 'lucide-react';
import { Button } from '../../../../shared/ui';
import {
    useRemoteSnowlumaQrLogin,
    type RemoteQrLoginApi,
} from '../../../../hooks/webui/useRemoteSnowlumaQrLogin';
import type { SnowLumaQrLoginState } from '../../../../core/ipc/generated/domain/SnowLumaQrLoginState';
import { useThemeTokens } from '../../../../hooks/theme/useThemeTokens';
import { resolveLoginQrPayload } from './loginQrImage';
import {
    QrLoginDialogShell,
    QrStagePlaceholder,
    type QrPlaceholderVariant,
    type QrStageTone,
} from './QrLoginDialogShell';

const QR_PIXEL_SIZE = 280;

const QR_TOKEN_SPEC = {
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
    const { foreground, background } = useThemeTokens(QR_TOKEN_SPEC);
    useEffect(() => {
        let active = true;
        const controller = new AbortController();
        setSrc(null);
        void resolveLoginQrPayload(payload, controller.signal)
            .then(async (content) => {
                if (!active) return;
                const image = await QRCode.toDataURL(content, {
                    width: QR_PIXEL_SIZE,
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
        <img
            src={src}
            alt="QQ 登录二维码"
            width={QR_PIXEL_SIZE}
            height={QR_PIXEL_SIZE}
            className="h-[280px] w-[280px] select-none rounded-sm"
            draggable={false}
            onError={onError}
        />
    ) : (
        <QrStagePlaceholder variant="loading" title="正在渲染二维码…" />
    );
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

    const showQrImage =
        open &&
        !isOnline &&
        !paused &&
        !refreshing &&
        !imageUnavailable &&
        state === 'waiting_for_qr' &&
        Boolean(snapshot?.qrcode_payload);

    const statusTone: QrStageTone = completed
        ? 'success'
        : state === 'cleanup_failed' || state === 'account_mismatch' || Boolean(error)
          ? 'danger'
          : state === 'expired' || imageUnavailable || fallbackOnly
            ? 'warning'
            : showQrImage || state === 'scanned' || state === 'login_detected'
              ? 'brand'
              : 'neutral';

    const placeholder: { variant: QrPlaceholderVariant; title: string; hint?: string } = (() => {
        if (completed) {
            return {
                variant: 'success',
                title: '登录成功',
                hint: '二维码已安全移除，即将进入在线状态',
            };
        }
        if (state === 'scanned') {
            return {
                variant: 'scanned',
                title: '已扫码，请在手机上确认',
                hint: '在手机 QQ 点击「允许登录」完成授权',
            };
        }
        if (state === 'login_detected') {
            return {
                variant: 'loading',
                title: '正在确认登录状态…',
                hint: '手机已授权，正在等待 SnowLuma 会话就绪',
            };
        }
        if (state === 'expired') {
            return {
                variant: 'expired',
                title: '二维码已过期',
                hint: '正在自动获取新二维码，也可点击下方立即刷新',
            };
        }
        if (fallbackOnly || imageUnavailable) {
            return {
                variant: 'warning',
                title: status,
                hint: onOpenVnc
                    ? '可点击左下角「打开 QQ 界面」继续完成登录'
                    : '请尝试重新获取二维码或检查 QQ 运行状态',
            };
        }
        return {
            variant: 'loading',
            title: status,
            hint: '请稍候，正在准备 QQ 登录二维码',
        };
    })();

    return (
        <QrLoginDialogShell
            open={open}
            onOpenChange={onOpenChange}
            botId={botId}
            flavorLabel="SnowLuma"
            hasQrCode={showQrImage}
            ariaBusy={requesting}
            viewport={
                showQrImage && snapshot?.qrcode_payload ? (
                    <LoginQrImage
                        key={snapshot.qrcode_payload}
                        payload={snapshot.qrcode_payload}
                        onError={imageError}
                    />
                ) : (
                    <QrStagePlaceholder
                        variant={placeholder.variant}
                        title={placeholder.title}
                        hint={placeholder.hint}
                    />
                )
            }
            statusText={status}
            statusTone={statusTone}
            footer={
                <>
                    {onOpenVnc && !completed ? (
                        <Button
                            variant={fallbackOnly ? 'primary' : 'ghost'}
                            size="sm"
                            onClick={() => {
                                onOpenChange(false);
                                onOpenVnc();
                            }}
                        >
                            <Monitor size={14} aria-hidden />
                            打开 QQ 界面
                        </Button>
                    ) : null}
                    {completed || (fallbackOnly && !canRetry) ? (
                        <Button
                            variant={completed ? 'primary' : 'secondary'}
                            size="sm"
                            className="ml-auto min-w-24"
                            onClick={() => onOpenChange(false)}
                        >
                            {completed ? (
                                <Check size={14} aria-hidden />
                            ) : (
                                <X size={14} aria-hidden />
                            )}
                            {completed ? '完成' : '关闭'}
                        </Button>
                    ) : (
                        <Button
                            variant={state === 'expired' ? 'primary' : 'secondary'}
                            size="sm"
                            className="ml-auto min-w-24"
                            disabled={!canRefresh || refreshing || requesting}
                            onClick={() => (canRetry ? retry() : void refresh())}
                        >
                            <RefreshCw
                                size={14}
                                className={refreshing ? 'animate-spin' : undefined}
                                aria-hidden
                            />
                            {refreshing ? '正在刷新' : canRetry ? '重新获取' : '刷新二维码'}
                        </Button>
                    )}
                </>
            }
        />
    );
}
