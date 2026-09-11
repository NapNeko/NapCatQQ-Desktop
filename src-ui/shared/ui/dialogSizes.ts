// Dialog 宽度档位。业务用 size= 选档，避免各处手写 max-w-* 不一致。
// sm–xl 高度由 DialogContent 内 ResizeObserver + GSAP 过渡；sheet 用 max-h + flex。

export const DIALOG_SIZE_CLASS = {
    sm: 'max-w-sm',
    md: 'max-w-md',
    lg: 'max-w-2xl',
    xl: 'max-w-3xl',
    // 预留 portal 上下 p-6；高度用 max-h + 子级 flex 分区滚，不整页卷走 footer。
    sheet: 'max-w-3xl max-h-[calc(100dvh-3rem)] flex min-h-0 flex-col overflow-hidden',
    // 通道编辑：左连接 + 中字段 + 右 JSON。
    sheetWide: 'max-w-5xl max-h-[calc(100dvh-3rem)] flex min-h-0 flex-col overflow-hidden',
    // 新手引导：图文并茂宽卡（欢迎合成预览 + 路径三列需要宽度）。
    onboarding:
        'max-w-[820px] w-[min(96vw,820px)] max-h-[calc(100dvh-2.5rem)] flex flex-col overflow-hidden p-0',
    taskQueue:
        'max-w-5xl w-[min(96vw,1120px)] h-[min(92dvh,900px)] min-h-[min(52dvh,480px)] max-h-[min(92dvh,900px)] flex flex-col overflow-hidden p-0',
} as const;

export type DialogSize = keyof typeof DIALOG_SIZE_CLASS;