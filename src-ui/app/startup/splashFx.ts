// 启动屏动效小工具：副标题换词、退场开洞几何。
// swapText 引用 GSAP、IRIS_SUPPORTED 是模块加载时读浏览器 CSS 能力的探测常量，
// 都带运行时依赖，放不进 core/domain，留在 app 层给三个时间线 hook 共用。

import gsap from 'gsap';

/// 退场用 clip-path 光圈揭示主界面；WebView2 支持 path(evenodd)，不支持则退回整层淡出。
export const IRIS_SUPPORTED =
    typeof CSS !== 'undefined' &&
    typeof CSS.supports === 'function' &&
    CSS.supports('clip-path', 'path(evenodd, "M0 0H1V1H0Z")');

/// 外矩形 + 内圆（evenodd）= 挖洞；洞越大露出的主界面越多。
export function irisClipPath(w: number, h: number, cx: number, cy: number, r: number): string {
    const rr = Math.max(0.5, r);
    const radius = rr.toFixed(1);
    const diameter = (rr * 2).toFixed(1);
    return `path(evenodd, "M0 0H${w}V${h}H0Z M${(cx - rr).toFixed(1)} ${cy.toFixed(1)}a${radius} ${radius} 0 1 0 ${diameter} 0a${radius} ${radius} 0 1 0 -${diameter} 0Z")`;
}

/// 副标题换词：旧词上飘淡出，新词从下方浮入；同词不重播。
export function swapText(el: HTMLElement, next: string, k: number): gsap.core.Timeline | null {
    if (el.textContent === next) return null;
    return gsap
        .timeline()
        .to(el, { autoAlpha: 0, y: -6, duration: 0.05 * k, ease: 'power2.in' })
        .add(() => {
            el.textContent = next;
        })
        .fromTo(
            el,
            { autoAlpha: 0, y: 6 },
            { autoAlpha: 1, y: 0, duration: 0.08 * k, ease: 'power2.out' },
        );
}
