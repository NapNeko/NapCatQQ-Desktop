// 动画体系核心 token + 三档 preset。GSAP 版,精细化第二轮。
//
// 跟第一版的差异(为什么重写):
//   1. preset 拆 timing/feel 两层。timing 控速度(duration/stagger/ease 字符串),
//      feel 控形态(scale/lift/shadow/brightness/overshoot)。这两层正交,需要时
//      可以单独覆盖,不再一改 ease 就连带 scale 跟着调。
//   2. ease 命名语义化:enter/exit/hover/press/release/pop/damped 七档,
//      不再让业务直接读 enterEase/bouncyEase 这种"它到底用在哪"的命名。
//   3. 注册 CustomEase 手画曲线:critical-damped spring(物理感的阻尼)、
//      aftershock(主反弹后还有一小段尾震,用于 rich 档 release)。仅在 standard/rich
//      档启用,elegant 档退化到 power 系列。
//   4. shadow / brightness 写进 feel,卡片/按钮悬停时 boxShadow + filter 一起动,
//      不再只动 transform。
//   5. speed 滑块同时 scale stagger(之前只影响 duration,导致快档下 stagger 还是
//      慢悠悠分批,跟 duration 对不上节奏)。
//
// 跟动画库耦合的部分仅集中在 ease 字符串(GSAP 解析)+ CustomEase 注册。其它都是
// 纯数字。业务通过 useMotion() 拿 helper,不直接读 motionPresets。

import gsap from 'gsap';
import { CustomEase } from 'gsap/CustomEase';
import { CustomBounce } from 'gsap/CustomBounce';
import { CustomWiggle } from 'gsap/CustomWiggle';

// CustomEase / CustomBounce / CustomWiggle 模块级注册。GSAP 对同插件重复 register
// 是幂等的,放模块顶层最稳。
gsap.registerPlugin(CustomEase, CustomBounce, CustomWiggle);

// 自定义曲线 ID。命名前缀 ndf 避免跟用户/插件冲突。
// critical-damped:物理 spring 临界阻尼,无超调直达,适合"立即归位"的 hover/press。
// 通过 5 段贝塞尔逼近 1 - exp(-t)*(1+t) 的解析形式。
CustomEase.create('ndf-critical', 'M0,0 C0.18,1 0.45,1 1,1');
// soft-spring:轻微超调(峰值 ~1.04),回归后无尾震。standard 档 release 默认。
CustomEase.create('ndf-spring', 'M0,0 C0.34,1.32 0.46,1.06 1,1');
// elastic-spring:中度超调 + 一次小回振(峰值 ~1.12),rich 档 release。
CustomEase.create('ndf-elastic', 'M0,0 C0.32,1.5 0.5,0.95 0.62,1.05 0.78,1 1,1 1,1');
// quick-out:小元素进场,比 power2.out 收得更急(适合 indicator / chip)。
CustomEase.create('ndf-quick', 'M0,0 C0.1,0.78 0.18,0.96 1,1');

// CustomBounce / CustomWiggle 由 GSAP 内部根据物理参数生成曲线,不需要手画。
// 名字带 .out 后缀,跟 GSAP 内置 ease 命名一致(power2.out / back.out 等)。
//
// ndf-aftershock:用 CustomBounce 生成。strength=0.4 = 弹性强度,squash=2 = 落地
// 时被压扁的程度。挑选这两个值让 Counter 数字 / 状态 pop 看起来像"硬糖落地"。
CustomBounce.create('ndf-aftershock', { strength: 0.4, squash: 2 });
// ndf-bounce:更强的弹簧,rich 档专用 release。strength=0.55 让两次明显反弹。
CustomBounce.create('ndf-bounce', { strength: 0.55, squash: 1 });
// ndf-wiggle:水平摇摆 ease,replace 之前 5 段手挂的 shake。type=anticipate 让
// 第一次摇摆方向预备,wiggles=4 = 四次反向摆动。
CustomWiggle.create('ndf-wiggle', { wiggles: 4, type: 'anticipate' });

export type MotionLevel = 'elegant' | 'standard' | 'rich';

/// 七档语义化 ease,值是 GSAP ease 字符串(内置或 CustomEase ID)。
/// 任意 preset 至少要填齐这七档,业务取 useMotion().ease.<kind> 即可。
export interface MotionTiming {
    /// 基础 duration(秒)。speed 滑块在这之上再除一次。
    durationFast: number;
    durationBase: number;
    durationSlow: number;
    /// 列表 stagger(秒)。0 = 关。speed 滑块也会 scale 这个值。
    stagger: number;
    ease: {
        /// 中大型元素进场(Dialog / Page / Card 列表项)
        enter: string;
        /// 中大型元素退场
        exit: string;
        /// 微小元素进场(indicator / chip / icon button)
        enterMicro: string;
        /// hover 上抬/缩放,要"听话不超调"
        hover: string;
        /// 按下瞬间(快、轻微 ease-out)
        press: string;
        /// 释放(可超调,标准档轻 spring,rich 档 elastic 一下)
        release: string;
        /// 状态变化反馈(数字 rolling / 状态徽章 pop)
        pop: string;
        /// 阻尼归位(取消按下时回到 1.0,不要 spring,要立即但不生硬)
        damped: string;
    };
}

/// feel:静态形态值,描述每档动画的"空间幅度"。
export interface MotionFeel {
    /// hover 缩放比例。1 = 不缩放(优雅档)。
    hoverScale: number;
    /// tap 按下缩放比例。
    tapScale: number;
    /// 卡片 hover 上抬距离(px)。
    cardLift: number;
    /// hover 时 boxShadow 加深倍率(基于 token shadow-popover)。
    /// 1.0 = 原始 shadow,1.4 = y 偏移 + blur 都增 40%。0 = 不动。
    shadowBoost: number;
    /// hover 亮度提升(filter brightness)。1.0 = 不动。
    brightness: number;
    /// rich 档独享:状态变化是否走 aftershock(余震)而不是单 spring。
    overshoot: boolean;
    /// 状态点呼吸单轮时长(秒)。
    breathDuration: number;
    /// 错误 shake 总位移(px)。0 = 关闭 shake。
    shakeAmplitude: number;
    /// pop 反馈最大缩放峰值。1.0 = 关闭 pop(elegant 档)。
    popPeak: number;
}

interface PresetEntry {
    timing: MotionTiming;
    feel: MotionFeel;
}

/// 三档预设。
///
/// elegant:仅 fade + 微 slide,无 spring,无 pop,无 shake。视觉最安静。
/// standard:轻 spring + 卡片轻浮 + 状态点呼吸。日常默认。
/// rich:elastic release + aftershock pop + 强 lift + brightness/shadow 联动 + shake。
///
/// 三档之间 ease 也是分级的:elegant 用 GSAP 内置 power 系列(无超调),
/// standard 用 ndf-spring(轻),rich 用 ndf-elastic + ndf-aftershock(强且带余震)。
export const motionPresets: Record<MotionLevel, PresetEntry> = {
    elegant: {
        timing: {
            durationFast: 0.12,
            durationBase: 0.16,
            durationSlow: 0.22,
            stagger: 0,
            ease: {
                enter: 'power2.out',
                exit: 'power2.in',
                enterMicro: 'power2.out',
                hover: 'power2.out',
                press: 'power2.out',
                release: 'power2.out',
                pop: 'power2.out',
                damped: 'power2.out',
            },
        },
        feel: {
            hoverScale: 1,
            tapScale: 1,
            cardLift: 0,
            shadowBoost: 0,
            brightness: 1,
            overshoot: false,
            breathDuration: 1.8,
            shakeAmplitude: 0,
            popPeak: 1,
        },
    },
    standard: {
        timing: {
            durationFast: 0.14,
            durationBase: 0.2,
            durationSlow: 0.28,
            stagger: 0.035,
            ease: {
                enter: 'power3.out',
                exit: 'power3.in',
                enterMicro: 'ndf-quick',
                hover: 'ndf-critical',
                press: 'power2.out',
                release: 'ndf-spring',
                pop: 'ndf-spring',
                damped: 'ndf-critical',
            },
        },
        feel: {
            hoverScale: 1.02,
            tapScale: 0.96,
            cardLift: 1,
            shadowBoost: 0.5,
            brightness: 1.02,
            overshoot: false,
            breathDuration: 1.6,
            shakeAmplitude: 4,
            popPeak: 1.06,
        },
    },
    rich: {
        timing: {
            durationFast: 0.16,
            durationBase: 0.24,
            durationSlow: 0.32,
            stagger: 0.045,
            ease: {
                enter: 'back.out(1.7)',
                exit: 'power3.in',
                enterMicro: 'back.out(2)',
                hover: 'ndf-spring',
                press: 'power2.out',
                // 真实物理 bounce(CustomBounce 生成),让按钮 release 弹两次有"啪嗒"质感。
                release: 'ndf-bounce',
                // aftershock 由 CustomBounce 生成,比手画曲线更有"落地"感。
                pop: 'ndf-aftershock',
                damped: 'ndf-critical',
            },
        },
        feel: {
            hoverScale: 1.04,
            tapScale: 0.92,
            cardLift: 2,
            shadowBoost: 1.0,
            brightness: 1.04,
            overshoot: true,
            breathDuration: 1.4,
            shakeAmplitude: 6,
            popPeak: 1.12,
        },
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

/// stagger 也跟着 speed 走,不然快档时 duration 已经压短但项之间间隔仍慢悠悠,
/// 整列表节奏不一致。
export function scaleStagger(base: number, speed: number): number {
    return base / clampSpeed(speed);
}

/// 给 useMotion 用的浅型,导出方便 hooks 文件不必再 import 内部 PresetEntry。
export type MotionPreset = PresetEntry;
