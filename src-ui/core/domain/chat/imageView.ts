// 图片缩放与光标锚点计算。
export interface ImageSize { width: number; height: number }
export type ImageFit = 'auto' | 'fit' | 'width';
export const limitImageScale = (value: number) => Math.max(0.01, Math.min(8, value));
export function imageScale(image: ImageSize, viewport: ImageSize, mode: ImageFit): number {
    if (!image.width || !image.height || !viewport.width || !viewport.height) return 1;
    const width = Math.min(1, (viewport.width - 48) / image.width);
    const height = (viewport.height - 48) / image.height;
    const long = image.height / image.width > 2.2;
    return limitImageScale(mode === 'width' || (mode === 'auto' && long) ? width : Math.min(width, height));
}
export function zoomScroll(scroll: number, cursor: number, viewport: number, image: number, before: number, after: number): number {
    const oldInset = Math.max(24, (viewport - image * before) / 2);
    const newInset = Math.max(24, (viewport - image * after) / 2);
    const next = (scroll + cursor - oldInset) / before * after + newInset - cursor;
    return Math.max(0, Math.min(Math.max(viewport, image * after + 48) - viewport, next));
}
