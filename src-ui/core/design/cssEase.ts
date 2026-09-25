// GSAP 缓动名 → CSS 缓动函数。给 WAAPI / CSS 动画用，和仍由 GSAP 驱动的部分观感对齐。
//
// power 系列按 GSAP 的定义换算：power1 = quad，power2 = cubic，power3 = quart，power4 = quint。
// 自定义曲线只收单段三次贝塞尔的那几条（和 motion.ts 里 CustomEase.create 的路径一字不差）；
// ndf-elastic / ndf-bounce / ndf-aftershock 是多段曲线，CSS 表达不了，走 fallback。

const TABLE: Readonly<Record<string, string>> = {
    none: 'linear',
    linear: 'linear',
    'power1.in': 'cubic-bezier(0.11, 0, 0.5, 0)',
    'power1.out': 'cubic-bezier(0.5, 1, 0.89, 1)',
    'power1.inOut': 'cubic-bezier(0.45, 0, 0.55, 1)',
    'power2.in': 'cubic-bezier(0.32, 0, 0.67, 0)',
    'power2.out': 'cubic-bezier(0.33, 1, 0.68, 1)',
    'power2.inOut': 'cubic-bezier(0.65, 0, 0.35, 1)',
    'power3.in': 'cubic-bezier(0.5, 0, 0.75, 0)',
    'power3.out': 'cubic-bezier(0.25, 1, 0.5, 1)',
    'power3.inOut': 'cubic-bezier(0.76, 0, 0.24, 1)',
    'power4.in': 'cubic-bezier(0.64, 0, 0.78, 0)',
    'power4.out': 'cubic-bezier(0.22, 1, 0.36, 1)',
    'sine.inOut': 'cubic-bezier(0.37, 0, 0.63, 1)',
    'back.out(1.7)': 'cubic-bezier(0.34, 1.56, 0.64, 1)',
    'ndf-critical': 'cubic-bezier(0.18, 1, 0.45, 1)',
    'ndf-spring': 'cubic-bezier(0.34, 1.32, 0.46, 1.06)',
    'ndf-quick': 'cubic-bezier(0.1, 0.78, 0.18, 0.96)',
};

export function cssEase(gsapEase: string, fallback = 'ease-out'): string {
    return TABLE[gsapEase] ?? fallback;
}
