// 动画体系核心 token + 三档 preset。GSAP 版。
//
// 职责:
//   1. motionPresets: 三档(elegant/standard/rich)的 ease/duration/stagger/缩放配置
//   2. clampSpeed: 速度滑块边界
//   3. 常量 MOTION_SPEED_MIN/MAX/DEFAULT
//
// 跟动画库(GSAP)耦合的部分只有 ease 字符串(GSAP 解析"power3.out""back.out(1.7)"
// 这种格式),其它都是纯数字。这层保持纯 TS 不 import 任何运行时,业务通过
// useMotion() 拿 preset 后,自己组装 gsap.to/timeline 调用。

export type MotionLevel = 'elegant' | 'standard' | 'rich';

interface PresetEntry {
    /// 基础 duration(秒)。speed 滑块在这个上面再除一次。
    durationFast: number;
    durationBase: number;
    durationSlow: number;
    /// 入场 ease(GSAP 字符串)。
    enterEase: string;
    /// 退场 ease。
    exitEase: string;
    /// 按钮 hover/tap 用的 ease(spring 替代品)。
    hoverEase: string;
    /// rich 档专属 QQ 弹 ease。standard 档复用 hoverEase,elegant 档为 power2.out 即可。
    bouncyEase: string;
    /// 列表 stagger 错位(秒)。0 = 关闭 stagger。
    stagger: number;
    /// hover/tap 缩放幅度。1 = 不缩放(优雅档)。
    tapScale: number;
    hoverScale: number;
    /// 卡片 hover 上抬距离(px)。0 = 不抬。
    cardLift: number;
    /// rich 档独享:enter 时是否走 elastic/back overshoot。
    overshoot: boolean;
    /// 状态点呼吸单轮时长(秒)。
    breathDuration: number;
}

/// 三档预设。GSAP 内置 ease:
///   power1~power4 .in/.out/.inOut - 越大越急促
///   back.out(overshoot) - 超调反弹,QQ 弹首选
///   elastic.out(amplitude, period) - 弹簧反弹,过头再回
///   bounce.out - 落地反弹(像球)
export const motionPresets: Record<MotionLevel, PresetEntry> = {
    elegant: {
        durationFast: 0.12,
        durationBase: 0.16,
        durationSlow: 0.22,
        enterEase: 'power2.out',
        exitEase: 'power2.in',
        hoverEase: 'power2.out',
        bouncyEase: 'power2.out',
        stagger: 0,
        tapScale: 1,
        hoverScale: 1,
        cardLift: 0,
        overshoot: false,
        breathDuration: 1.8,
    },
    standard: {
        durationFast: 0.14,
        durationBase: 0.2,
        durationSlow: 0.28,
        enterEase: 'power3.out',
        exitEase: 'power3.in',
        hoverEase: 'back.out(1.4)',
        bouncyEase: 'back.out(1.6)',
        stagger: 0.035,
        tapScale: 0.96,
        hoverScale: 1.02,
        cardLift: 1,
        overshoot: false,
        breathDuration: 1.6,
    },
    rich: {
        durationFast: 0.16,
        durationBase: 0.24,
        durationSlow: 0.32,
        // 入场用 back overshoot,有"啪嗒落位"的弹性
        enterEase: 'back.out(1.7)',
        exitEase: 'power3.in',
        // hover 仍用 back,tap 用 elastic 弹得更明显
        hoverEase: 'back.out(2)',
        bouncyEase: 'elastic.out(1, 0.4)',
        stagger: 0.045,
        tapScale: 0.92,
        hoverScale: 1.04,
        cardLift: 2,
        overshoot: true,
        breathDuration: 1.4,
    },
};

/// 速度滑块上下界。1.0 = 默认,>1 越快,<1 越慢。
export const MOTION_SPEED_MIN = 0.5;
export const MOTION_SPEED_MAX = 1.5;
export const MOTION_SPEED_DEFAULT = 1.0;

export function clampSpeed(speed: number): number {
    if (!Number.isFinite(speed)) return MOTION_SPEED_DEFAULT;
    return Math.max(MOTION_SPEED_MIN, Math.min(MOTION_SPEED_MAX, speed));
}

/// 把 baseline duration 按用户速度滑块换算成最终值。
export function scaleDuration(base: number, speed: number): number {
    return base / clampSpeed(speed);
}
