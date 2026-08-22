// 主题切换过渡 — View Transitions API 圆形扩散揭示。
//
// 原理：document.startViewTransition 让合成器截住旧帧，changeTheme 同步换掉
// data-theme，新旧两层快照叠放。给「新主题」层播放 clip-path circle 关键帧，
// 从触发点（默认屏幕中心）扩张到覆盖全屏，旧主题层静止垫底被逐渐替换。
// 这是 View Transitions 主题切换的标准玩法：JS 只负责写两个 CSS 变量，
// 动画全程跑在合成器侧，无 DOM 覆盖层、无逐帧计算。

import './themeTransition.css';

/** 主题过渡的动画配置。 */
export interface ThemeTransitionOptions {
    enabled: boolean;
    level: 'elegant' | 'standard' | 'rich';
    duration: number;
    easing: string;
    /** 扩散圆心的视口坐标（px）。缺省取屏幕中心。 */
    originX?: number;
    originY?: number;
}

// 当前 DOM lib 未收录 View Transitions，这里补最小声明；
// 只覆盖本项目用到的同步回调形态。
interface ViewTransition {
    readonly ready: Promise<void>;
    readonly finished: Promise<void>;
}

declare global {
    interface Document {
        startViewTransition?(update: () => void): ViewTransition;
    }
}

const SUPPORTS_VIEW_TRANSITION =
    typeof document !== 'undefined' &&
    typeof document.startViewTransition === 'function';

// 上一次过渡没跑完时直接瞬时切换：叠两个 View Transition 会互相抢伪元素。
let active = false;

// 最近一次指针按下位置，用来把扩散圆心对准触发点击。键盘触发的保存
// 没有近期 pointerdown，会自然回落到屏幕中心。
let lastPointerX = Number.NaN;
let lastPointerY = Number.NaN;
let lastPointerAt = 0;

function ensurePointerTracking(): void {
    if (typeof window === 'undefined' || lastPointerAt !== 0) return;
    window.addEventListener('pointerdown', (e) => {
        lastPointerX = e.clientX;
        lastPointerY = e.clientY;
        lastPointerAt = Date.now();
    }, { capture: true, passive: true });
}

export function playThemeTransition(
    changeTheme: () => void,
    opts: ThemeTransitionOptions,
): Promise<void> {
    // elegant / 禁用动画 / 过渡进行中走瞬时切换；不支持时也直接切。
    if (!opts.enabled || opts.level === 'elegant' || !SUPPORTS_VIEW_TRANSITION || active) {
        changeTheme();
        return Promise.resolve();
    }

    const rootEl = document.documentElement;

    // 圆心：优先调用方显式传入，其次 2s 内的指针按下位置（即触发点击），
    // 都没有则取屏幕中心。
    ensurePointerTracking();
    const recentClick = Date.now() - lastPointerAt < 2000;
    const cx = Number.isFinite(opts.originX)
        ? (opts.originX as number)
        : recentClick ? lastPointerX : innerWidth / 2;
    const cy = Number.isFinite(opts.originY)
        ? (opts.originY as number)
        : recentClick ? lastPointerY : innerHeight / 2;
    // 终态半径要盖住最远的视口角。
    const endR = Math.hypot(Math.max(cx, innerWidth - cx), Math.max(cy, innerHeight - cy));

    // 档位与时长必须在 startViewTransition 之前写入 DOM：
    // ::view-transition-* 伪元素在过渡开始那一刻按当前样式解析，
    // 事后补属性会有一帧竞态（表现为闪一下 UA 默认交叉淡入）。
    // duration 沿用 motion 体系的秒单位（GSAP 约定），CSS 动画要 ms。
    const durMs = Math.max(0, Math.round(opts.duration * 1000));
    rootEl.style.setProperty('--theme-reveal-dur', `${durMs}ms`);
    rootEl.style.setProperty('--theme-reveal-x', `${Math.round(cx)}px`);
    rootEl.style.setProperty('--theme-reveal-y', `${Math.round(cy)}px`);
    rootEl.style.setProperty('--theme-reveal-r', `${Math.ceil(endR)}px`);
    rootEl.dataset.themeReveal = opts.level === 'rich' ? 'rich' : 'standard';
    active = true;

    const cleanup = () => {
        delete rootEl.dataset.themeReveal;
        for (const name of ['--theme-reveal-dur', '--theme-reveal-x', '--theme-reveal-y', '--theme-reveal-r']) {
            rootEl.style.removeProperty(name);
        }
        active = false;
    };

    const vt = document.startViewTransition!(changeTheme);

    // finished 在跳过 / 出错时也会 reject，统一吞掉保证清理必然执行。
    return vt.finished.catch(() => undefined).then(cleanup);
}
