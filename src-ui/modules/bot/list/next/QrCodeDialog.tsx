// NapCat WebUI 登录二维码弹窗。
//
// NapCat WebUI `/api/QQLogin/CheckLoginStatus` 返回的 qrcodeurl 字段是 QR
// 编码内容字符串，形态通常是 `https://ssl.ptlogin2.qq.com/qrlogin?...` 这种
// QQ 客户端扫码后会跳转的登录链接文本，本质上就是 QR 的 payload，不是
// PNG 图片地址。NapCat 官方前端 (`napcat-webui-frontend/components/qr_code_login.tsx`)
// 也是直接 `<QRCodeSVG value={qrcode}/>` 把它当 payload 编码。
//
// 因此这里默认走 qrcode 库本地编码 SVG。极少数测试桩可能给 `data:image/png;base64,...`
// 直接塞图片，这种 fallback 单独处理。绝不能凭 `https?://` 前缀就当成图片
// URL 直接 <img src>，那是真实 NapCat 返回的常态，会导致只显示 alt 文字。
//
// 自动刷新行为：NapCat 不主动设过期，二维码过期由 NTQQ 内核
// (`onQRCodeSessionFailed` errType=1 errCode=3) 判定，过期后 qrcodeurl 不会
// 自动变新，需要点 napcat 官方前端的"刷新"按钮调 `loginService.getQRCodePicture()`。
// 我们这边没透传 `loginError` 字段，也没做"刷新"按钮，所以 UI 只显示"已等待
// Xs"客观事实，让用户自行判断要不要关掉弹窗重开。`qrcode_url` 字符串值
// 真发生变化时（说明内核推了新二维码），秒表回零。
//
// 登录成功自动关闭：弹窗打开期间观察 `isOnline` 从 false / null 跳到 true，
// 视为本次扫码登录成功的可靠信号。仅靠 qrcodeUrl=null 不够：踢线、停 Bot
// 都会让 qrcodeUrl 被清，但那不是登录成功。`onLoginSuccess` 由父组件提供，
// 弹窗自身只负责发出信号；push InfoBar / 关弹窗都在父组件的 callback 内做。
//
// 踢线自动关闭：弹窗打开期间观察 `invalidationReason === 'kicked'`，触发
// `onKicked` 让父组件关弹窗 + push 提示。被踢后 QQ 协议层会拒绝同账号在同
// 设备扫码恢复，留弹窗在那等"新二维码"是误导用户；恢复路径只有"重启 Bot"，
// BotCard 那层会显示自动 / 手动重启的对应文案。
//
// 主题适配：QR 前景 / 背景颜色从 design token 读取（--qr-foreground /
// --qr-background），主题切换时自动重渲染，未来引入 ThemeProvider 不用改组件。

import { useEffect, useRef, useState } from 'react';
import QRCode from 'qrcode';
import { Dialog, DialogContent, DialogTitle, DialogDescription } from '../../../../shared/ui';
import { useThemeTokens } from '../../../../hooks/theme/useThemeTokens';
import type { NapCatLoginInvalidationReason } from '../../../../core/ipc/types';

interface QrCodeDialogProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    /** 二维码内容字符串（NapCat 透传）。null 时该卡片应当不显示 QR 按钮，dialog 也不会被打开。 */
    qrcodeUrl: string | null;
    /** 卡片对应的 Bot QQID，仅作 dialog 标题展示，不参与登录逻辑。 */
    botId: string;
    /** 当前 Bot 在线状态。`true` → 已登录，`false` → 离线，`null/undefined` → 未知。 */
    isOnline?: boolean | null;
    /** 后端登录失效原因，仅 `'kicked'` 时弹窗自动关闭并通过 `onKicked` 通知父组件。 */
    invalidationReason?: NapCatLoginInvalidationReason | null;
    /** 弹窗打开期间检测到登录成功（isOnline 跳到 true）时触发。父组件应在此处关闭弹窗 + push InfoBar。 */
    onLoginSuccess?: () => void;
    /** 弹窗打开期间检测到账号被踢时触发。父组件应在此处关闭弹窗 + push InfoBar 引导用户重启。 */
    onKicked?: () => void;
}

const QR_PIXEL_SIZE = 280;

const QR_TOKEN_SPEC = {
    foreground: { name: '--qr-foreground', fallback: '#1a120d' },
    background: { name: '--qr-background', fallback: '#ffffff' },
} as const;

function isInlineImageSrc(value: string): boolean {
    // 只接 data:image/...；普通 http(s) URL 是 NapCat 二维码的 payload 字符串，
    // 必须走库编码不能当图片地址。
    return value.startsWith('data:image/');
}

export function QrCodeDialog({
    open,
    onOpenChange,
    qrcodeUrl,
    botId,
    isOnline,
    invalidationReason,
    onLoginSuccess,
    onKicked,
}: QrCodeDialogProps) {
    const elapsed = useElapsedSinceContent(qrcodeUrl, open);
    useLoginSuccessSignal(open, isOnline, onLoginSuccess);
    useKickedSignal(open, invalidationReason, onKicked);

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="max-w-sm">
                <DialogTitle>扫码登录 · {botId}</DialogTitle>
                <DialogDescription>
                    用 QQ 客户端扫描下方二维码完成登录。
                </DialogDescription>

                <div className="mt-4 flex items-center justify-center rounded-md bg-elevated p-6 ring-1 ring-border-subtle">
                    {qrcodeUrl ? <QrCanvas content={qrcodeUrl} /> : <QrPending />}
                </div>

                <div className="mt-3 flex items-center justify-center text-2xs text-text-tertiary tabular-nums">
                    {qrcodeUrl ? <ElapsedHint seconds={elapsed} /> : null}
                </div>
            </DialogContent>
        </Dialog>
    );
}

/// 监听 invalidationReason 跳变到 'kicked'。弹窗每次开启都会重置基线，
/// 避免上次开过留下的踢线状态在新弹窗瞬间被当成新事件触发。
function useKickedSignal(
    open: boolean,
    invalidationReason: NapCatLoginInvalidationReason | null | undefined,
    onKicked: (() => void) | undefined,
) {
    const triggeredRef = useRef(false);

    useEffect(() => {
        if (!open) {
            triggeredRef.current = false;
            return;
        }
        // 只触发一次：父组件清掉 invalidationReason 之前不要重复触发；
        // logged_out 不是踢线（normal logout，扫码可恢复），不在此处理。
        if (!triggeredRef.current && invalidationReason === 'kicked') {
            triggeredRef.current = true;
            onKicked?.();
        }
    }, [open, invalidationReason, onKicked]);
}

/// 监听 isOnline 跳变 false/null → true。弹窗每次开启重置基线，避免上次开过
/// 留下的"true"状态在下次开弹窗瞬间立刻被当成新的登录成功。
function useLoginSuccessSignal(
    open: boolean,
    isOnline: boolean | null | undefined,
    onLoginSuccess: (() => void) | undefined,
) {
    const baselineRef = useRef<boolean | null>(null);

    useEffect(() => {
        if (!open) {
            baselineRef.current = null;
            return;
        }
        // 弹窗刚打开：记下当时的 isOnline 作为基线（首次开启之后不再覆盖，避免
        // 在 isOnline 从 false 跳 true 之间被中间值反复刷新基线）
        if (baselineRef.current === null) {
            baselineRef.current = isOnline === true;
        }
    }, [open, isOnline]);

    useEffect(() => {
        if (!open) return;
        // 从 false / null / undefined 跳到 true → 认定为登录成功
        if (baselineRef.current === false && isOnline === true) {
            // 防重复：标记基线为 true 之后，后续 isOnline 起伏不会再次触发
            baselineRef.current = true;
            onLoginSuccess?.();
        }
    }, [open, isOnline, onLoginSuccess]);
}

/// 秒表：当 `content` 字符串真值变化时回零，每秒 +1。dialog 关闭时停表。
///
/// 注意 `content` 必须用值比较而不是引用比较：后端 poller 每秒都重新发布
/// NapCatLoginQrcode 事件，reducer 每次都返回新对象引用，但内容多数情况下
/// 没变。useEffect 默认 deps 已经是浅比较，所以直接传 string 就够。
function useElapsedSinceContent(content: string | null, active: boolean): number {
    const [elapsed, setElapsed] = useState(0);
    const startedAtRef = useRef<number | null>(null);

    // content 真值变化或弹窗刚打开 → 重置秒表起点
    useEffect(() => {
        if (!active || !content) {
            startedAtRef.current = null;
            setElapsed(0);
            return;
        }
        startedAtRef.current = Date.now();
        setElapsed(0);
    }, [content, active]);

    // 每秒推进 elapsed
    useEffect(() => {
        if (!active || !content) return;
        const id = window.setInterval(() => {
            const startedAt = startedAtRef.current;
            if (startedAt == null) return;
            setElapsed(Math.floor((Date.now() - startedAt) / 1000));
        }, 1000);
        return () => window.clearInterval(id);
    }, [active, content]);

    return elapsed;
}

function ElapsedHint({ seconds }: { seconds: number }) {
    // 60s 内只显示秒数；超过 60s 拼成"分:秒"，避免出现"已等待 137s"这种
    // 一眼读不出来的数字。
    //
    // 不用 font-mono：Mono 字体只覆盖 latin / 数字字形，CJK 部分会 fallback
    // 到系统等宽 CJK，Windows 上常见命中点阵 SimHei，整串就花了。数字对齐
    // 已经在父级用 tabular-nums 解决，足够稳。
    const text =
        seconds < 60
            ? `已等待 ${seconds}s`
            : `已等待 ${Math.floor(seconds / 60)} 分 ${String(seconds % 60).padStart(2, '0')} 秒`;
    return <span>{text}</span>;
}

function QrCanvas({ content }: { content: string }) {
    const { foreground, background } = useThemeTokens(QR_TOKEN_SPEC);
    const [svgMarkup, setSvgMarkup] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        // 后端如果直接给的是 data:image data URL（兼容路径 / 测试桩），跳过库渲染
        if (isInlineImageSrc(content)) {
            setSvgMarkup(null);
            setError(null);
            return;
        }
        let cancelled = false;
        QRCode.toString(content, {
            type: 'svg',
            errorCorrectionLevel: 'M',
            margin: 1,
            width: QR_PIXEL_SIZE,
            color: { dark: foreground, light: background },
        })
            .then((svg) => {
                if (!cancelled) {
                    setSvgMarkup(svg);
                    setError(null);
                }
            })
            .catch((err: unknown) => {
                if (!cancelled) {
                    setSvgMarkup(null);
                    setError(err instanceof Error ? err.message : String(err));
                }
            });
        return () => {
            cancelled = true;
        };
    }, [content, foreground, background]);

    if (isInlineImageSrc(content)) {
        return (
            <img
                src={content}
                alt="WebUI 登录二维码"
                className="h-[280px] w-[280px] select-none"
                draggable={false}
            />
        );
    }

    if (error) {
        return (
            <div className="flex h-[280px] w-[280px] flex-col items-center justify-center gap-1 text-center text-xs text-danger">
                <span>二维码渲染失败</span>
                <span className="text-2xs text-text-tertiary">{error}</span>
            </div>
        );
    }

    if (!svgMarkup) {
        return <QrPending />;
    }

    return (
        <div
            className="h-[280px] w-[280px] select-none"
            // SVG 来自 qrcode 库本地编码，输入是后端透传字符串；尽管字符串可控且
            // 库本身只输出 <svg> 元素，仍把 SVG 当受信本地资源处理（无 onerror、
            // 不会发外网请求）。
            dangerouslySetInnerHTML={{ __html: svgMarkup }}
        />
    );
}

function QrPending() {
    return (
        <div className="flex h-[280px] w-[280px] items-center justify-center text-sm text-text-tertiary">
            二维码已失效，请稍候…
        </div>
    );
}
