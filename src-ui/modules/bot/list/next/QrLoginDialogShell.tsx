// 扫码登录弹窗公共视觉骨架（NapCat 与 SnowLuma 共用）。
//
// 视觉语言：
//   - 顶部：图标徽章 + 标题「扫码登录」+ 右侧协议/账号胶囊 + 副标题说明
//   - 中部：最大 280×280、随弹窗宽度缩小的正方形画布。有码时铺 --qr-background 静区底色；
//     无码时为柔和内嵌占位面板（加载动画 / 手机确认 / 异常或过期图标）
//   - 底部：状态胶囊 + 可选辅助信息（等待秒表等）+ 操作按钮栏

import type { ReactNode } from 'react';
import { AlertTriangle, CheckCircle2, Loader2, QrCode, RefreshCw, Smartphone } from 'lucide-react';
import {
    Badge,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';

const QR_FRAME_STYLE = {
    background: 'var(--qr-background)',
    color: 'var(--qr-foreground)',
} as const;

export type QrStageTone = 'neutral' | 'brand' | 'success' | 'warning' | 'danger';

export interface QrLoginDialogShellProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    botId: string;
    flavorLabel?: 'NapCat' | 'SnowLuma';
    /** 是否正在展示真实二维码图像（决定外框是否铺 --qr-background 扫码静区） */
    hasQrCode: boolean;
    ariaBusy?: boolean;
    /** 自适应正方形视口内的内容（二维码图像或 QrStagePlaceholder） */
    viewport: ReactNode;
    /** 底部状态栏主文案（带 role="status"） */
    statusText: ReactNode;
    statusTone?: QrStageTone;
    /** 状态栏右侧辅助信息（如「已等待 12s」） */
    statusAside?: ReactNode;
    /** 底部按钮区（可选） */
    footer?: ReactNode;
}

export function QrLoginDialogShell({
    open,
    onOpenChange,
    botId,
    flavorLabel,
    hasQrCode,
    ariaBusy,
    viewport,
    statusText,
    statusTone = 'neutral',
    statusAside,
    footer,
}: QrLoginDialogShellProps) {
    const dotClass =
        statusTone === 'success'
            ? 'bg-success'
            : statusTone === 'warning'
              ? 'bg-warning'
              : statusTone === 'danger'
                ? 'bg-danger'
                : statusTone === 'brand'
                  ? 'bg-brand animate-pulse'
                  : 'bg-text-tertiary';

    const textClass =
        statusTone === 'danger'
            ? 'text-danger'
            : statusTone === 'warning'
              ? 'text-warning'
              : statusTone === 'success'
                ? 'text-success'
                : 'text-text-secondary';

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent size="sm">
                <DialogHeader className="mb-3.5">
                    <div className="flex items-center gap-2.5 pr-6">
                        <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-brand/20 bg-brand-soft text-brand">
                            <QrCode size={16} strokeWidth={2.1} aria-hidden />
                        </span>
                        <div className="min-w-0 flex-1">
                            <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
                                <DialogTitle className="min-w-0 truncate text-[15px] leading-snug">
                                    扫码登录 · {botId}
                                </DialogTitle>
                                {flavorLabel && (
                                    <Badge
                                        tone={flavorLabel === 'SnowLuma' ? 'info' : 'brand'}
                                        appearance="soft"
                                        className="shrink-0 font-mono text-[10px]"
                                    >
                                        {flavorLabel}
                                    </Badge>
                                )}
                            </div>
                            <DialogDescription className="mt-0.5 text-xs text-text-tertiary">
                                使用手机 QQ 扫描二维码并确认登录
                            </DialogDescription>
                        </div>
                    </div>
                </DialogHeader>

                <div
                    aria-busy={ariaBusy}
                    className={cn(
                        'relative mx-auto flex w-full min-w-0 flex-col items-center justify-center overflow-hidden rounded-lg p-3 ring-1 transition-colors duration-200 sm:p-5',
                        hasQrCode
                            ? 'shadow-card ring-border-subtle'
                            : 'bg-inset/55 text-text-secondary ring-border-subtle/80',
                    )}
                    style={hasQrCode ? QR_FRAME_STYLE : undefined}
                >
                    <div className="relative flex aspect-square w-full max-w-[280px] items-center justify-center">
                        {viewport}
                    </div>
                </div>

                <div className="mt-3 flex min-h-7 flex-wrap items-center justify-between gap-x-3 gap-y-1 rounded-md border border-border-subtle/70 bg-inset/40 px-3 py-1.5">
                    <div className="flex min-w-0 flex-1 items-center gap-2">
                        <span
                            aria-hidden
                            className={cn('h-2 w-2 shrink-0 rounded-full', dotClass)}
                        />
                        <p
                            role="status"
                            className={cn('min-w-0 break-words text-xs font-medium', textClass)}
                        >
                            {statusText}
                        </p>
                    </div>
                    {statusAside ? (
                        <div className="shrink-0 font-mono text-2xs tabular-nums text-text-tertiary">
                            {statusAside}
                        </div>
                    ) : null}
                </div>

                {footer ? (
                    <DialogFooter className="mt-3.5 justify-between gap-2.5 border-t border-border-subtle/75 pt-3.5">
                        {footer}
                    </DialogFooter>
                ) : null}
            </DialogContent>
        </Dialog>
    );
}

export type QrPlaceholderVariant = 'loading' | 'scanned' | 'expired' | 'success' | 'warning';

export function QrStagePlaceholder({
    variant,
    title,
    hint,
}: {
    variant: QrPlaceholderVariant;
    title: string;
    hint?: string;
}) {
    const iconBadge = (() => {
        switch (variant) {
            case 'loading':
                return (
                    <span className="flex h-12 w-12 items-center justify-center rounded-full border border-border-subtle bg-surface text-brand shadow-card">
                        <Loader2 size={21} strokeWidth={2} className="animate-spin" aria-hidden />
                    </span>
                );
            case 'scanned':
                return (
                    <span className="flex h-12 w-12 items-center justify-center rounded-full border border-brand/25 bg-brand-soft text-brand shadow-card">
                        <Smartphone size={22} strokeWidth={2} aria-hidden />
                    </span>
                );
            case 'expired':
                return (
                    <span className="flex h-12 w-12 items-center justify-center rounded-full border border-warning/25 bg-warning-soft text-warning shadow-card">
                        <RefreshCw size={20} strokeWidth={2} aria-hidden />
                    </span>
                );
            case 'success':
                return (
                    <span className="flex h-12 w-12 items-center justify-center rounded-full border border-success/25 bg-success-soft text-success shadow-card">
                        <CheckCircle2 size={22} strokeWidth={2} aria-hidden />
                    </span>
                );
            case 'warning':
                return (
                    <span className="flex h-12 w-12 items-center justify-center rounded-full border border-warning/25 bg-warning-soft text-warning shadow-card">
                        <AlertTriangle size={21} strokeWidth={2} aria-hidden />
                    </span>
                );
        }
    })();

    return (
        <div className="flex h-full w-full flex-col items-center justify-center gap-3 px-4 text-center">
            {iconBadge}
            <div className="space-y-1">
                <p className="text-sm font-medium text-text">{title}</p>
                {hint ? <p className="text-xs leading-relaxed text-text-tertiary">{hint}</p> : null}
            </div>
        </div>
    );
}
