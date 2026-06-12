// 主题切换过渡动画 — html-to-image 截图 + GSAP 水波覆盖层。
//
// 原理：
//   1. 用 html-to-image 截取旧主题页面截图
//   2. 创建全屏覆盖层，背景 = 截图
//   3. 在覆盖层下方切换主题
//   4. GSAP 水波 clip-path 从顶部向底部扫过：
//      覆盖层（旧截图）从顶部开始逐步消失，露出新主题
//   5. 动画结束 → 移除覆盖层 → 主题切换完成

import { gsap } from 'gsap';
import { toPng } from 'html-to-image';

/** 主题过渡的动画配置。 */
export interface ThemeTransitionOptions {
    enabled: boolean;
    level: 'elegant' | 'standard' | 'rich';
    duration: number;
    easing: string;
}

let running = false;

export function playThemeTransition(
    changeTheme: () => void,
    opts: ThemeTransitionOptions,
): void {
    if (!opts.enabled || running) {
        changeTheme();
        return;
    }
    running = true;
    const done = () => { running = false; };

    if (opts.level === 'elegant') {
        changeTheme();
        done();
        return;
    }

    // ① 截取旧主题页面截图
    toPng(document.body, {
        cacheBust: true,
        style: { overflow: 'hidden' },
    })
        .then((dataUrl) => runWaveTransition(changeTheme, opts, dataUrl, done))
        .catch(() => runWaveTransition(changeTheme, opts, null, done));
}

/** 创建覆盖层并播放水波动画。 */
function runWaveTransition(
    changeTheme: () => void,
    opts: ThemeTransitionOptions,
    screenshot: string | null,
    done: () => void,
): void {
    const root = document.documentElement;
    const vh = innerHeight;
    const vw = innerWidth;

    // ─── 创建覆盖层 ────────────────────────────────────────────────
    const overlay = document.createElement('div');
    const bgColor = getComputedStyle(root)
        .getPropertyValue('--bg-base').trim() || '#1e1e2e';

    const styles: string[] = [
        'position:fixed',
        'inset:0',
        'z-index:2147483647',
        'pointer-events:none',
    ];

    if (screenshot) {
        // 用截图作为背景 → 覆盖层看起来就是旧主题
        styles.push(
            `background:url(${screenshot}) center/cover no-repeat`,
            `background-color:${bgColor}`,
        );
    } else {
        // 降级：纯色背景
        styles.push(`background:${bgColor}`);
    }

    overlay.style.cssText = styles.join(';');
    document.body.appendChild(overlay);

    // ─── 在覆盖层下方切换主题 ──────────────────────────────────────
    changeTheme();

    // ─── 波形参数 ──────────────────────────────────────────────────
    const amp = Math.max(18, vh * 0.03);   // 波幅
    const waves = 2.5;                       // 波数（非整数 → 边缘不对称，更自然）
    const samples = 48;                      // 采样点

    // ─── GSAP 驱动水波动画 ─────────────────────────────────────────
    // yBase 从顶部向底部移动：覆盖层从上方开始消失，新主题从上往下出现
    const state = { yBase: -amp - 50, phase: 0 };
    const dur = opts.duration;

    const updateClip = () => {
        const pts: string[] = [];
        for (let i = 0; i <= samples; i++) {
            const t = i / samples;
            const x = t * vw;
            // 双正弦波叠加 → 更复杂的波形，像真实水波
            const y = state.yBase
                + amp * Math.sin(t * Math.PI * 2 * waves + state.phase)
                + amp * 0.3 * Math.sin(t * Math.PI * 2 * waves * 1.7 + state.phase * 1.3);
            pts.push(`${x.toFixed(1)}px ${y.toFixed(1)}px`);
        }
        pts.push(`${vw}px ${vh + 200}px`, `0px ${vh + 200}px`);
        overlay.style.clipPath = `polygon(${pts.join(',')})`;
    };

    gsap.ticker.add(updateClip);

    const tl = gsap.timeline({
        onComplete: () => {
            gsap.ticker.remove(updateClip);
            overlay.remove();
            done();
        },
    });

    // phase 持续变化（波纹涌动）
    tl.to(state, {
        phase: Math.PI * 6,
        duration: dur,
        ease: 'none',
    }, 0);

    // 水波从顶部扫到底部（新主题从上往下出现）
    tl.to(state, {
        yBase: vh + amp + 50,
        duration: dur,
        ease: 'power1.inOut',
    }, 0);

    // rich 档：body 亮度脉冲
    if (opts.level === 'rich') {
        tl.fromTo(document.body,
            { filter: 'brightness(1.04)' },
            { filter: 'brightness(1)', duration: dur * 0.35, ease: 'power2.out' },
            dur * 0.4,
        );
    }

    // 安全超时清理（防止 GSAP timeline 卡住）
    setTimeout(() => {
        if (tl.isActive()) {
            tl.kill();
            gsap.ticker.remove(updateClip);
            overlay.remove();
            done();
        }
    }, dur * 1000 + 500);
}
