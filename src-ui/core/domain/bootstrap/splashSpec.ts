// 启动屏静态规格表：展示时长基准、文案、进度刻度、粒子/轨道环/星芒布局参数。
// 从 StartupSplash 上移：这些是与动效实现解耦的纯数据与纯函数，调节奏、改文案
// 不用碰组件文件，也不带任何 React / DOM / GSAP 依赖。

/// 壳已就绪后至少再展示这么久（含进场），让「正在准备界面」那一拍能被读到。
/// 基准值按默认速度（k=2）翻倍后约 1.8s；速度滑块拉快时整段随之缩短。
export const MIN_VISIBLE_BASE_MS = 900;
export const MAX_WAIT_MS = 12_000;

export const SUB_TEXT = {
    wake: '正在唤醒…',
    prepare: '正在准备界面…',
    ready: '启动就绪，欢迎！',
} as const;

/// 进度条三段：进场到一半、待机推到八成并呼吸、出发瞬间冲满。
export const BAR_ENTER = 0.45;
export const BAR_IDLE = 0.8;
export const BAR_IDLE_BREATH = 0.84;

export interface SplashParticle {
    left: number;
    top: number;
    size: number;
    accent: boolean;
    driftX: number;
    driftY: number;
    /// 基准秒，运行时再按速度滑块缩放。
    driftDur: number;
}

/// 固定种子 LCG：每次启动布局一致，又不用手写几十个坐标。
function buildParticles(count: number, seed: number): SplashParticle[] {
    let state = seed >>> 0;
    const rand = () => {
        state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
        return state / 4294967296;
    };
    const out: SplashParticle[] = [];
    while (out.length < count) {
        const left = 4 + rand() * 92;
        const top = 6 + rand() * 88;
        // 中央 logo / 标题区留空，星点别压在文字上
        if (left > 32 && left < 68 && top > 26 && top < 74) continue;
        out.push({
            left,
            top,
            size: 2 + Math.round(rand() * 4),
            accent: rand() < 0.4,
            driftX: (rand() - 0.5) * 18,
            driftY: -(6 + rand() * 14),
            driftDur: 1.3 + rand() * 1.2,
        });
    }
    return out;
}

export const SPLASH_PARTICLES = buildParticles(28, 1592707838);

export const ORBIT_RINGS: ReadonlyArray<{
    size: number;
    solid: boolean;
    dotAtTop: boolean;
    accentDot: boolean;
    spin: 1 | -1;
    /// 转一圈的基准秒。
    dur: number;
}> = [
    { size: 118, solid: false, dotAtTop: true, accentDot: false, spin: 1, dur: 7 },
    { size: 156, solid: true, dotAtTop: false, accentDot: true, spin: -1, dur: 10 },
];

export const SPARKLES: ReadonlyArray<{
    top?: string;
    bottom?: string;
    left?: string;
    right?: string;
    size: number;
    color: string;
    char: string;
}> = [
    { top: '-10px', right: '-12px', size: 18, color: 'var(--brand-300, #f59e0b)', char: '✦' },
    { top: '-8px', left: '-10px', size: 14, color: 'var(--brand-400, #a855f7)', char: '✧' },
    { bottom: '-8px', right: '-8px', size: 14, color: 'var(--accent-400, #ec4899)', char: '✧' },
    { bottom: '-10px', left: '-12px', size: 18, color: 'var(--green-400, #10b981)', char: '✦' },
];

export const BRAND_PULSE_SIZE = 360;
export const RING_FX_SIZE = 96;
