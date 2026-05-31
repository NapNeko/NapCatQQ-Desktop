// 动画体系核心 token + variants。
//
// 三个职责:
//   1. motionPresets: 三档(elegant/standard/rich) duration + spring 配置
//   2. getTransition(level, speed, kind): 选择器,业务直接用
//   3. variants 常量: 路由/列表/Dialog/InfoBar/按钮等场景的 framer variants
//
// 分层:这是纯 TS,零 React/tauri 依赖,跟 design tokens 同级。
// 业务代码不要直接写 spring 数值,要么从这里取 variants,要么调 getTransition()。

import type { Transition, Variants } from 'framer-motion';

export type MotionLevel = 'elegant' | 'standard' | 'rich';

/// 动画"种类"。不同种类基线 duration 不同,例如 dialog 略长于按钮。
export type MotionKind =
    | 'fast'      // 90ms 系:按钮按下/微 hover
    | 'base'      // 180ms 系:路由/列表/InfoBar
    | 'slow'      // 280ms 系:Dialog/Tabs 大块切换
    | 'spring'    // spring,配 stiffness/damping
    | 'bouncy';   // rich 档专用:按钮 QQ 弹

interface PresetEntry {
    /// 基础 duration(秒)。speed 滑块在这个上面再乘。
    fast: number;
    base: number;
    slow: number;
    /// spring 配置。reduce-motion / 优雅档退化为 tween。
    spring: { stiffness: number; damping: number; mass?: number };
    bouncy: { stiffness: number; damping: number; mass?: number };
    /// 列表 stagger 错位(秒)。0 = 关闭 stagger。
    stagger: number;
    /// hover/tap 缩放幅度。0 = 不缩放(优雅档)。
    tapScale: number;
    hoverScale: number;
    /// 卡片 hover 上抬距离(px)。0 = 不抬。
    cardLift: number;
    /// rich 档独享:按钮松手反弹超调到这个倍率,再回 1。standard 档为 1(无超调)。
    bouncyOvershoot: number;
}

/// 三档预设。曲线选 spring stiff/damp 而不是 cubic-bezier,因为 reduced-motion
/// 时直接换 transition 类型即可,无需替换关键帧。
export const motionPresets: Record<MotionLevel, PresetEntry> = {
    elegant: {
        fast: 0.12,
        base: 0.16,
        slow: 0.22,
        spring: { stiffness: 180, damping: 22 },
        bouncy: { stiffness: 220, damping: 24 },
        stagger: 0,
        tapScale: 1,
        hoverScale: 1,
        cardLift: 0,
        bouncyOvershoot: 1,
    },
    standard: {
        fast: 0.14,
        base: 0.2,
        slow: 0.28,
        spring: { stiffness: 320, damping: 26 },
        bouncy: { stiffness: 380, damping: 22 },
        stagger: 0.03,
        tapScale: 0.96,
        hoverScale: 1.02,
        cardLift: 1,
        bouncyOvershoot: 1,
    },
    rich: {
        fast: 0.16,
        base: 0.24,
        slow: 0.32,
        spring: { stiffness: 420, damping: 18 },
        bouncy: { stiffness: 600, damping: 14, mass: 0.9 },
        stagger: 0.04,
        tapScale: 0.92,
        hoverScale: 1.04,
        cardLift: 2,
        bouncyOvershoot: 1.04,
    },
};

/// 速度滑块上下界。1.0 = 默认,>1 越快,<1 越慢。
export const MOTION_SPEED_MIN = 0.5;
export const MOTION_SPEED_MAX = 1.5;
export const MOTION_SPEED_DEFAULT = 1.0;

/// 主选择器:根据 level + speed 给出可直接用于 framer transition prop 的对象。
/// 业务通常不直接调,而是通过 useMotionTransition() hook 取到当前生效值。
/// reduced/disabled 命中时调用方应自己短路返回 { duration: 0 }(useMotion 已封装)。
export function getTransition(
    level: MotionLevel,
    speed: number,
    kind: MotionKind = 'base',
): Transition {
    const p = motionPresets[level];
    const s = clampSpeed(speed);
    if (kind === 'spring') {
        return { type: 'spring', ...p.spring };
    }
    if (kind === 'bouncy') {
        return { type: 'spring', ...p.bouncy };
    }
    return {
        type: 'tween',
        ease: [0.16, 1, 0.3, 1] as const,
        duration: p[kind] / s,
    };
}

export function clampSpeed(speed: number): number {
    if (!Number.isFinite(speed)) return MOTION_SPEED_DEFAULT;
    return Math.max(MOTION_SPEED_MIN, Math.min(MOTION_SPEED_MAX, speed));
}

// ---- variants ----
//
// 这些 variants 不带 transition 字段,业务在 motion.div 上单独传 transition,
// 拿 useMotionTransition('base'|'slow'|...) 的返回值。这样档位/速度变化时
// 不用换 variants,只 transition prop 重新求值即可。

/// 路由切换:fade + slide-y + 微缩放。退场用反向 y 让人感觉旧页"被推出",新页从下方 + 略缩放滑入。
/// 视觉幅度比之前更明显:y 12px(rich)/8px(standard)/4px(elegant),scale 0.985 起跳。
/// rich 档由 RouteOutlet 自己用 useMotion 选 spring 让进场带轻微弹性。
export const pageVariants: Variants = {
    initial: { opacity: 0, y: 10, scale: 0.985 },
    animate: { opacity: 1, y: 0, scale: 1 },
    exit: { opacity: 0, y: -8, scale: 0.99, transition: { duration: 0.16 } },
};

/// 列表项进出。父级 motion.ul 用 listContainerVariants 控 stagger,
/// 子项用 listItemVariants 自身进退场。
export const listContainerVariants = (stagger: number): Variants => ({
    initial: {},
    animate: { transition: { staggerChildren: stagger } },
    exit: {},
});

export const listItemVariants: Variants = {
    initial: { opacity: 0, y: 6, scale: 0.985 },
    animate: { opacity: 1, y: 0, scale: 1 },
    exit: { opacity: 0, y: -6, scale: 0.985, transition: { duration: 0.12 } },
};

/// Dialog 进退场。overlay 仅 fade;content 在 fade 外加 scale 0.96→1。
export const dialogOverlayVariants: Variants = {
    initial: { opacity: 0 },
    animate: { opacity: 1 },
    exit: { opacity: 0 },
};

export const dialogContentVariants: Variants = {
    initial: { opacity: 0, scale: 0.96, y: -2 },
    animate: { opacity: 1, scale: 1, y: 0 },
    exit: { opacity: 0, scale: 0.97, y: -2 },
};

/// InfoBar 从右侧滑入,rich 档进场会 scale 到 overshoot 再回 1。
export const infoBarVariants = (overshoot: number): Variants => ({
    initial: { opacity: 0, x: 16, scale: 0.985 },
    animate: {
        opacity: 1,
        x: 0,
        scale: overshoot > 1 ? [1, overshoot, 1] : 1,
    },
    exit: { opacity: 0, x: 12, scale: 0.985, transition: { duration: 0.16 } },
});

/// 按钮 hover/tap variants(由 MotionButton 直接读 useMotion 后选 hover/tap 系数,
/// 这里只是占位语义,真实数值在组件内现拼)。
export interface ButtonInteract {
    whileHover: { scale: number };
    whileTap: { scale: number };
}

export function buttonInteract(level: MotionLevel): ButtonInteract {
    const p = motionPresets[level];
    return {
        whileHover: { scale: p.hoverScale },
        whileTap: { scale: p.tapScale },
    };
}

/// Tab 指示器(走 layoutId 跟随,这里给 transition 即可)。
export function tabIndicatorTransition(level: MotionLevel, speed: number): Transition {
    const p = motionPresets[level];
    return { type: 'spring', ...p.spring, mass: 0.6, ...{ duration: p.base / clampSpeed(speed) } };
}
