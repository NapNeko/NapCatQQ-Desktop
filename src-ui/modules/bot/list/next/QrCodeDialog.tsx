// NapCat WebUI 登录二维码弹窗。
//
// 复刻旧 Python 版 BotCard 的二维码交互：卡上挂一个小图标按钮，仅当后端推送
// 了二维码（qrcodeUrl 非空）时显示；点开 dialog 弹大图。
//
// 二维码自动刷新机制：上层 hook（useNapcatLogin）已在 qrcodeUrl 字段上做事件
// 聚合 + 替换，本组件只渲染。dialog 开着时 url 变化会自动重渲染图。
//
// 不在卡内常驻图：旧 React 版把 200x200 二维码嵌在卡里挤掉副信息空间，旧
// Python 版从来都是按钮 + 弹窗。新版恢复 Python 设计。

import { Dialog, DialogContent, DialogTitle, DialogDescription } from '../../../../shared/ui';

interface QrCodeDialogProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    /** 二维码 data URL 或外链。null 时该卡片应当不显示 QR 按钮，dialog 也不会被打开。 */
    qrcodeUrl: string | null;
    /** 卡片对应的 Bot QQID，仅作 dialog 标题展示，不参与登录逻辑。 */
    botId: string;
}

export function QrCodeDialog({
    open,
    onOpenChange,
    qrcodeUrl,
    botId,
}: QrCodeDialogProps) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="max-w-sm">
                <DialogTitle>扫码登录 · {botId}</DialogTitle>
                <DialogDescription>
                    用 QQ 扫描下方二维码完成登录。二维码 30 秒过期会自动刷新。
                </DialogDescription>

                <div className="mt-4 flex items-center justify-center rounded-md bg-canvas p-6 ring-1 ring-border-subtle">
                    {qrcodeUrl ? (
                        <img
                            src={qrcodeUrl}
                            alt="WebUI 登录二维码"
                            className="h-[280px] w-[280px] select-none"
                            draggable={false}
                        />
                    ) : (
                        <div className="flex h-[280px] w-[280px] items-center justify-center text-sm text-text-tertiary">
                            二维码已失效，请稍候…
                        </div>
                    )}
                </div>
            </DialogContent>
        </Dialog>
    );
}
