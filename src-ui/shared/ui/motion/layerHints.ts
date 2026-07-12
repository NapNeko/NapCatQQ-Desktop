// 动画期间临时提升合成层，结束后立刻放下，避免 will-change 常驻占内存。

export function armTransformLayer(el: HTMLElement): void {
    el.style.willChange = 'opacity, transform';
}

export function disarmTransformLayer(el: HTMLElement): void {
    el.style.willChange = 'auto';
}
