// Dialog 宽度档位。业务用 size= 选档，避免各处手写 max-w-* 不一致。
// 高度由 DialogContent 内 ResizeObserver + GSAP 在内容变化时自动过渡。

export const DIALOG_SIZE_CLASS = {
    sm: 'max-w-sm',
    md: 'max-w-md',
    lg: 'max-w-2xl',
    xl: 'max-w-3xl',
    sheet: 'max-w-3xl max-h-[85vh] flex flex-col',
} as const;

export type DialogSize = keyof typeof DIALOG_SIZE_CLASS;