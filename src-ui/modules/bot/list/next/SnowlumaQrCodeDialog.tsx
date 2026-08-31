import { useEffect, useState } from 'react';
import QRCode from 'qrcode';
import { AlertTriangle, Monitor } from 'lucide-react';
import { Button, Dialog, DialogContent, DialogDescription, DialogFooter, DialogTitle, Spinner } from '../../../../shared/ui';
import { useThemeTokens } from '../../../../hooks/theme/useThemeTokens';
import type { SnowlumaQrFailureCategory, SnowlumaQrLoginResult } from '../../../../core/ipc/types';

interface SnowlumaQrCodeDialogProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    botId: string;
    result: SnowlumaQrLoginResult | null;
    onOpenNovnc?: () => void;
}

const QR_TOKEN_SPEC = {
    foreground: { name: '--qr-foreground', fallback: '#1a120d' },
    background: { name: '--qr-background', fallback: '#ffffff' },
} as const;

const FAILURE_LABELS: Record<SnowlumaQrFailureCategory, string> = {
    unsupported_variant: '当前 QQ 窗口版本不受支持',
    capability_unavailable: '远端截图或输入能力不可用',
    ambiguous_binding: '无法唯一绑定当前 QQ 窗口',
    capture_failed: '远端二维码截图失败',
    decoder_unavailable: '本机二维码解码器不可用',
    decode_failed: '无法从截图中识别二维码',
    cancelled: '二维码提取已取消',
};

export function SnowlumaQrCodeDialog({
    open,
    onOpenChange,
    botId,
    result,
    onOpenNovnc,
}: SnowlumaQrCodeDialogProps) {
    const close = (nextOpen: boolean) => {
        onOpenChange(nextOpen);
    };

    return (
        <Dialog open={open} onOpenChange={close}>
            <DialogContent size="sm">
                <DialogTitle>SnowLuma 扫码登录 · {botId}</DialogTitle>
                <DialogDescription>
                    二维码仅在本机内存中处理，不会保存或复制到剪贴板。
                </DialogDescription>
                {!result ? (
                    <div className="mt-4 flex h-[280px] items-center justify-center rounded-md bg-elevated p-6 ring-1 ring-border-subtle">
                        <Spinner />
                    </div>
                ) : result.status === 'payload' ? (
                    <div className="mt-4 flex items-center justify-center rounded-md bg-elevated p-6 ring-1 ring-border-subtle">
                        <QrCanvas content={result.payload} />
                    </div>
                ) : (
                    <div className="mt-4 flex flex-col items-center gap-3 rounded-md bg-elevated p-6 text-center ring-1 ring-border-subtle">
                        <AlertTriangle className="text-warning" size={24} aria-hidden />
                        <p className="text-sm text-text">二维码提取不可用，请使用 noVNC 完成登录。</p>
                        <p className="text-xs text-text-tertiary">{FAILURE_LABELS[result.reason]}</p>
                        {onOpenNovnc && (
                            <Button variant="secondary" size="sm" onClick={onOpenNovnc}>
                                <Monitor size={14} />
                                打开 noVNC 桌面
                            </Button>
                        )}
                    </div>
                )}
                <DialogFooter>
                    <Button variant="ghost" size="sm" onClick={() => close(false)}>
                        关闭
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}

function QrCanvas({ content }: { content: string }) {
    const { foreground, background } = useThemeTokens(QR_TOKEN_SPEC);
    const [svgMarkup, setSvgMarkup] = useState<string | null>(null);
    const [error, setError] = useState(false);

    useEffect(() => {
        let cancelled = false;
        setSvgMarkup(null);
        setError(false);
        void QRCode.toString(content, {
            type: 'svg',
            errorCorrectionLevel: 'M',
            margin: 1,
            width: 280,
            color: { dark: foreground, light: background },
        })
            .then((svg) => {
                if (!cancelled) setSvgMarkup(svg);
            })
            .catch(() => {
                if (!cancelled) setError(true);
            });
        return () => {
            cancelled = true;
        };
    }, [content, foreground, background]);

    if (error) {
        return <div className="flex h-[280px] w-[280px] items-center justify-center text-xs text-danger">二维码渲染失败</div>;
    }
    if (!svgMarkup) {
        return <div className="flex h-[280px] w-[280px] items-center justify-center text-sm text-text-tertiary">二维码生成中…</div>;
    }
    return (
        <div
            data-testid="snowluma-qr-svg"
            className="h-[280px] w-[280px] select-none"
            dangerouslySetInnerHTML={{ __html: svgMarkup }}
        />
    );
}
