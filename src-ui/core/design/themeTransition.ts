// 主题切换过渡 — 蓄力 + View Transitions 光波揭示。
//
// 两段：先在 live DOM 上用 canvas 播一段蓄力（火花汇聚、细环收拢、核心闪光），
// 摘掉画布后再 startViewTransition。合成器截住旧帧，changeTheme 同步换掉 data-theme，
// 新旧两层快照叠放：新层从触发点撑开并从「发热」冷却回正常色，旧层在波前外侧让出
// 空隙露出光环并逐渐褪色，主波后面还拖两道回声环，点击点炸一团光晕加两圈错开的涟漪。
// 所有半径绑同一个注册属性所以天然同步；JS 只写几个变量、挂几个元素。

import './themeTransition.css';
import { runThemeCharge } from './themeCharge';

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

// 光环靠 @property 注册的 <length> 变量插值；没有它变量只能离散跳变，退回硬边圆。
const SUPPORTS_REGISTERED_PROPERTY =
    typeof CSS !== 'undefined' &&
    typeof (CSS as unknown as { registerProperty?: unknown }).registerProperty === 'function';

const ORIGIN_RING_CLASS = 'ndf-theme-origin-ring';
const ORIGIN_RING_COUNT = 3;
const HOLE_KEYFRAMES_ID = 'ndf-theme-reveal-hole-keyframes';
// 蓄力占总时长的比例；光波本身的时长仍由 opts.duration 决定
const CHARGE_RATIO = 0.24;

// 波前半径关键帧的终值用字面量写进 <style>，不在 @keyframes 里靠 var() 解析：
// 注册属性 + var() 关键帧在部分 Chromium 版本上会整条失效，表现为无动画硬切。
function writeHoleKeyframes(endPx: number): void {
    let style = document.getElementById(HOLE_KEYFRAMES_ID) as HTMLStyleElement | null;
    if (!style) {
        style = document.createElement('style');
        style.id = HOLE_KEYFRAMES_ID;
        document.head.appendChild(style);
    }
    style.textContent =
        `@keyframes theme-reveal-hole{from{--theme-reveal-hole:0px}to{--theme-reveal-hole:${endPx}px}}`;
}

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

export async function playThemeTransition(
    changeTheme: () => void,
    opts: ThemeTransitionOptions,
): Promise<void> {
    // elegant / 禁用动画 / 过渡进行中走瞬时切换；不支持时也直接切。
    if (!opts.enabled || opts.level === 'elegant' || !SUPPORTS_VIEW_TRANSITION || active) {
        changeTheme();
        return;
    }
    active = true;

    const rootEl = document.documentElement;
    const level: 'standard' | 'rich' = opts.level === 'rich' ? 'rich' : 'standard';

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

    // duration 沿用 motion 体系的秒单位（GSAP 约定），CSS 动画要 ms。
    const durMs = Math.max(0, Math.round(opts.duration * 1000));

    // 蓄力只在能画光环的档位播；它是 live DOM，必须在截旧帧之前摘干净，
    // 否则火花会被冻进旧快照里。蓄力失败不影响换主题。
    if (SUPPORTS_REGISTERED_PROPERTY) {
        try {
            await runThemeCharge(cx, cy, Math.round(durMs * CHARGE_RATIO), level);
        } catch (err) {
            console.warn('[themeTransition] 蓄力动画异常，跳过:', err);
        }
    }

    // 终态半径要盖住最远的视口角（蓄力期间窗口可能被拖过，这里才量）。
    const endR = Math.hypot(Math.max(cx, innerWidth - cx), Math.max(cy, innerHeight - cy));

    // 档位与时长必须在 startViewTransition 之前写入 DOM：
    // ::view-transition-* 伪元素在过渡开始那一刻按当前样式解析，
    // 事后补属性会有一帧竞态（表现为闪一下 UA 默认交叉淡入）。
    rootEl.style.setProperty('--theme-reveal-dur', `${durMs}ms`);
    rootEl.style.setProperty('--theme-reveal-x', `${Math.round(cx)}px`);
    rootEl.style.setProperty('--theme-reveal-y', `${Math.round(cy)}px`);
    // 主波要多跑一段，落后最远的回声环（rich 210px + 环宽）才能完全出屏，
    // 否则过渡结束那一帧回声环会在屏上硬消失
    const endPx = Math.ceil(endR) + 280;
    rootEl.style.setProperty('--theme-reveal-r', `${endPx}px`);
    if (SUPPORTS_REGISTERED_PROPERTY) writeHoleKeyframes(endPx);
    rootEl.dataset.themeReveal = SUPPORTS_REGISTERED_PROPERTY ? level : 'basic';

    // 起点光晕 / 涟漪环只在新状态存在：在换主题的同一同步回调里挂上，View Transition 只会
    // 给它们 new 快照，由 ::view-transition-new(theme-origin-N) 各自错开播炸开动画；过渡结束就摘掉。
    const originRings: HTMLDivElement[] = [];
    const mountOriginRings = () => {
        if (!SUPPORTS_REGISTERED_PROPERTY) return;
        for (let i = 1; i <= ORIGIN_RING_COUNT; i += 1) {
            const el = document.createElement('div');
            el.className = ORIGIN_RING_CLASS;
            el.dataset.ring = String(i);
            el.style.left = `${Math.round(cx)}px`;
            el.style.top = `${Math.round(cy)}px`;
            el.style.setProperty('view-transition-name', `theme-origin-${i}`);
            el.setAttribute('aria-hidden', 'true');
            document.body.appendChild(el);
            originRings.push(el);
        }
    };

    try {
        const vt = document.startViewTransition!(() => {
            changeTheme();
            mountOriginRings();
        });
        // finished 在跳过 / 出错时也会 reject，统一吞掉保证清理必然执行。
        await vt.finished.catch(() => undefined);
    } finally {
        delete rootEl.dataset.themeReveal;
        for (const name of ['--theme-reveal-dur', '--theme-reveal-x', '--theme-reveal-y', '--theme-reveal-r']) {
            rootEl.style.removeProperty(name);
        }
        for (const el of originRings) el.remove();
        active = false;
    }
}
