// Dialog 宽度档位。业务用 size= 选档，避免各处手写 max-w-* 不一致。
// 高度由 DialogContent 内 ResizeObserver + GSAP 在内容变化时自动过渡。

export const DIALOG_SIZE_CLASS = {
    sm: 'max-w-sm',
    md: 'max-w-md',
    lg: 'max-w-2xl',
    xl: 'max-w-3xl',
    sheet: 'max-w-3xl max-h-[85vh] flex flex-col',
    taskQueue:
        'max-w-5xl w-[min(96vw,1120px)] h-[min(92dvh,900px)] min-h-[min(52dvh,480px)] max-h-[min(92dvh,900px)] flex flex-col overflow-hidden p-0',
} as const;

export type DialogSize = keyof typeof DIALOG_SIZE_CLASS;