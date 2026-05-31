// useMotion: GSAP 动画统一入口。
//
// 第二轮重写要点:
//   1. 七档 ease 直接暴露:m.ease.enter/exit/hover/press/release/pop/damped,
//      业务不再读底层 enterEase/bouncyEase。
//   2. 提供高阶 helper:
//        - m.tween(el, vars, opts?)     gsap.to + 自动注入 ease/duration
//        - m.fromTo(el, from, to, opts?)
//        - m.bindHover(el, scale?)      返回 cleanup,自动挂 mouseenter/leave + brightness/shadow
//        - m.bindPress(el)              返回 cleanup,挂 mousedown/up,带 release spring
//        - m.pop(el, opts?)             一次性 pop 反馈 1 → popPeak → 1
//        - m.shake(el)                  错误 shake,rich/standard 档启用
//      helper 引用稳定(ref-backed),可放 useEffect 依赖,组件不会因 useMotion 调用
//      就被强迫重渲。
//   3. enabled=false / reduced 命中时,helper 全部短路:set 终态、不挂事件、不返
//      gsap.Tween。业务侧不需要再写 if (!m.enabled) gsap.set(...) 逻辑。
//   4. duration helper 仍是 m.duration('fast'|'base'|'slow') 字面量;新增
//      m.stagger() 跟 speed 走。
//
// 跟系统 prefers-reduced-motion 的关系:命中时强制 enabled=false,所有 helper
// 退化为静态。useGSAP / matchMedia 的官方推荐没用,因为 React state 驱动的动画
// 更适合在 hook 出口拿 enabled 布尔自己分支。

import { useEffect, useMemo, useRef, useState } from 'react';
import gsap from 'gsap';
import { usePreferences } from './preferencesStore';
import {
    motionPresets,
    scaleDuration,
    scaleStagger,
    type MotionLevel,
    type MotionPreset,
} from '../../core/design/motion';

export type DurationKind = 'fast' | 'base' | 'slow';
export type EaseKind =
    | 'enter'
    | 'exit'
    | 'enterMicro'
    | 'hover'
    | 'press'
    | 'release'
    | 'pop'
    | 'damped';

/// 业务可传入的 tween 选项,所有字段都是可选,缺什么就用 preset 默认值。
export interface TweenOptions {
    /// duration 档位,默认 'base'。
    kind?: DurationKind;
    /// ease 档位,默认 'enter'(用于 fromTo)或 'hover'(tween)。
    ease?: EaseKind;
    /// 显式 duration(秒),覆盖 kind。
    duration?: number;
    /// 显式 ease 字符串,覆盖 ease。
    easeStr?: string;
    /// 完成回调。
    onComplete?: () => void;
}

/// hover 绑定的可选项。
export interface HoverOptions {
    /// 自定义 scale,默认走 preset.feel.hoverScale。传 1 = 不缩放(行式大卡片用)。
    scale?: number;
    /// 是否让 GSAP 接管 boxShadow,默认 false。大部分卡片已经在 Tailwind 写了
    /// hover:shadow-popover,GSAP 再叠一份会扩散过深。需要 GSAP 控 shadow
    /// (跟 lift 联动到很重的层级)时显式 true。
    shadow?: boolean;
    /// 是否同步动 brightness,默认 true(rich 档才显著)。
    brightness?: boolean;
    /// 卡片 lift 距离(px),覆盖 preset.feel.cardLift。null = 不动 y。
    lift?: number | null;
}

export interface PopOptions {
    /// 自定义峰值,默认 preset.feel.popPeak。
    peak?: number;
    /// 自定义 ease,默认 preset.ease.pop。
    ease?: EaseKind;
}

/// useMotion 的返回值。env 字段是"当前快照",helper 是稳定引用。
/// 组件用 m.enabled / m.level 等做 useEffect 依赖时,引用变更带来的重跑是预期的。
export interface MotionEnv {
    /// 用户选的档位。
    level: MotionLevel;
    /// 速度滑块的有效值(已 clamp)。
    speed: number;
    /// 系统 prefers-reduced-motion 命中。
    reduced: boolean;
    /// 综合 enabled = 用户开关 && 非 reduced。helper 都依赖此判断短路。
    enabled: boolean;
    /// 当前档位 preset。timing/feel 两层。
    preset: MotionPreset;

    // ===== 直读快捷 =====
    /// duration(秒);enabled=false 时返回 0。
    duration: (kind?: DurationKind) => number;
    /// 列表 stagger 间隔(秒);enabled=false 时返回 0。
    stagger: () => number;
    /// 七档 ease,统一访问点。
    ease: Record<EaseKind, string>;

    // ===== gsap 工厂 =====
    /// gsap.to + 自动 ease/duration。返回 Tween 或 null(enabled=false 时直接 set 并返 null)。
    tween: (
        target: gsap.TweenTarget,
        vars: gsap.TweenVars,
        opts?: TweenOptions,
    ) => gsap.core.Tween | null;
    /// gsap.fromTo + 自动 ease/duration。
    fromTo: (
        target: gsap.TweenTarget,
        from: gsap.TweenVars,
        to: gsap.TweenVars,
        opts?: TweenOptions,
    ) => gsap.core.Tween | null;

    // ===== 交互绑定 =====
    /// 在 el 上挂 mouseenter/leave,自动跑 hover lift / scale / shadow / brightness。
    /// 返回 cleanup。enabled=false 时返回 noop。
    bindHover: (el: HTMLElement, opts?: HoverOptions) => () => void;
    /// 在 el 上挂 mousedown/up + mouseleave(防止鼠标按下后移开卡死),
    /// 自动跑 press/release 弹性。返回 cleanup。
    bindPress: (el: HTMLElement) => () => void;

    // ===== 反馈 =====
    /// 一次性 pop:scale 1 → popPeak → 1。enabled=false 时跳过。
    pop: (el: HTMLElement, opts?: PopOptions) => void;
    /// 一次性 shake:水平抖动 ±amplitude。
    shake: (el: HTMLElement) => void;
}

function useReducedMotion(): boolean {
    const [reduced, setReduced] = useState<boolean>(() => {
        if (typeof window === 'undefined' || !window.matchMedia) return false;
        return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    });
    useEffect(() => {
        if (typeof window === 'undefined' || !window.matchMedia) return;
        const mql = window.matchMedia('(prefers-reduced-motion: reduce)');
        const handler = (e: MediaQueryListEvent) => setReduced(e.matches);
        mql.addEventListener('change', handler);
        return () => mql.removeEventListener('change', handler);
    }, []);
    return reduced;
}

export function useMotion(): MotionEnv {
    const prefs = usePreferences();
    const reduced = useReducedMotion();
    const enabled = prefs.motionEnabled && !reduced;
    const level = prefs.motionLevel;
    const speed = prefs.motionSpeed;

    // 把"当前生效值"装在 ref 里,helper 读 ref 而不是闭包快照。这样 helper
    // 工厂在 useMemo([]) 下保持引用稳定,业务把它放 useEffect 依赖也不会因为
    // useMotion() 重渲染就触发 cleanup+resubscribe。
    //
    // !!! 重要 !!! 之前写法每次 render 都新建 duration/stagger 闭包并最终
    // return 新对象,导致业务 useEffect 写 [..., m] 时每帧重跑(BotCard hover
    // 监听器反复 add/remove,TextField focus 监听器反复挂,react-query 触发
    // 列表刷新时叠加放大成卡顿)。修法:整个返回值 useMemo 锁住,依赖只看
    // primitive(enabled/level/speed),业务依赖也只放这三个 primitive 不要放 m。
    const envRef = useRef<{ enabled: boolean; speed: number; preset: MotionPreset }>({
        enabled,
        speed,
        preset: motionPresets[level],
    });
    envRef.current = { enabled, speed, preset: motionPresets[level] };

    // helper 工厂:空依赖让函数引用永久稳定。读 envRef.current 拿最新值。
    const helpers = useMemo(() => {
        function resolveTweenVars(
            vars: gsap.TweenVars,
            opts: TweenOptions | undefined,
            defaultEase: EaseKind,
        ): gsap.TweenVars {
            const env = envRef.current;
            const t = env.preset.timing;
            const dur =
                opts?.duration !== undefined
                    ? opts.duration / Math.max(0.5, env.speed)
                    : (() => {
                          const k = opts?.kind ?? 'base';
                          const base =
                              k === 'fast'
                                  ? t.durationFast
                                  : k === 'slow'
                                      ? t.durationSlow
                                      : t.durationBase;
                          return scaleDuration(base, env.speed);
                      })();
            const easeStr =
                opts?.easeStr ?? t.ease[opts?.ease ?? defaultEase];
            return {
                ...vars,
                duration: dur,
                ease: easeStr,
                onComplete: opts?.onComplete ?? vars.onComplete,
            };
        }

        const tween: MotionEnv['tween'] = (target, vars, opts) => {
            const env = envRef.current;
            if (!env.enabled) {
                gsap.set(target, vars);
                opts?.onComplete?.();
                return null;
            }
            return gsap.to(target, resolveTweenVars(vars, opts, 'hover'));
        };

        const fromTo: MotionEnv['fromTo'] = (target, from, to, opts) => {
            const env = envRef.current;
            if (!env.enabled) {
                gsap.set(target, to);
                opts?.onComplete?.();
                return null;
            }
            return gsap.fromTo(target, from, resolveTweenVars(to, opts, 'enter'));
        };

        const bindHover: MotionEnv['bindHover'] = (el, opts) => {
            const onEnter = () => {
                const env = envRef.current;
                if (!env.enabled) return;
                const f = env.preset.feel;
                const t = env.preset.timing;
                const dur = scaleDuration(t.durationFast, env.speed);
                const vars: gsap.TweenVars = {
                    duration: dur,
                    ease: t.ease.hover,
                };
                const targetScale = opts?.scale ?? f.hoverScale;
                if (targetScale !== 1) vars.scale = targetScale;
                const liftPx = opts?.lift !== undefined ? opts.lift : -f.cardLift;
                if (liftPx !== 0 && liftPx !== null) vars.y = liftPx;
                // boxShadow 默认 opt-in:大部分卡片元素已经在 Tailwind 写了
                // hover:shadow-popover,我们再叠一份 GSAP boxShadow 会糊出"扩散一大圈"
                // 的视觉。需要 GSAP 接管 shadow 时显式传 { shadow: true }。
                if (opts?.shadow === true && f.shadowBoost > 0) {
                    const blur = 24 + 16 * f.shadowBoost;
                    const yOff = 8 + 6 * f.shadowBoost;
                    const alpha = (0.08 + 0.06 * f.shadowBoost).toFixed(3);
                    vars.boxShadow = `0 ${yOff}px ${blur}px rgba(0,0,0,${alpha})`;
                }
                if ((opts?.brightness ?? true) && f.brightness !== 1) {
                    vars.filter = `brightness(${f.brightness})`;
                }
                gsap.to(el, vars);
            };
            const onLeave = () => {
                const env = envRef.current;
                if (!env.enabled) return;
                const t = env.preset.timing;
                const dur = scaleDuration(t.durationFast, env.speed);
                // 只清 onEnter 真正可能设过的属性。boxShadow/filter 没开就不动,
                // 否则一个空字符串覆盖会冲掉 Tailwind 类设的 shadow。
                const vars: gsap.TweenVars = {
                    scale: 1,
                    y: 0,
                    duration: dur,
                    ease: t.ease.damped,
                };
                if (opts?.shadow === true) vars.boxShadow = '';
                if ((opts?.brightness ?? true)) vars.filter = '';
                gsap.to(el, vars);
            };
            const env = envRef.current;
            if (!env.enabled) return () => {};
            el.addEventListener('mouseenter', onEnter);
            el.addEventListener('mouseleave', onLeave);
            return () => {
                el.removeEventListener('mouseenter', onEnter);
                el.removeEventListener('mouseleave', onLeave);
            };
        };

        const bindPress: MotionEnv['bindPress'] = (el) => {
            // press 流程:
            //   mousedown → 立即压扁到 tapScale,无 spring(power2.out 短促)
            //   mouseup → 走 release ease 弹回 hoverScale(已悬停)或 1(未悬停)
            //   mouseleave 在按下中也要触发释放,避免按住后拖出去卡在压扁状态
            let pressed = false;
            let hovered = false;
            const onEnter = () => {
                hovered = true;
            };
            const onLeave = () => {
                hovered = false;
                if (pressed) {
                    pressed = false;
                    releaseTo(1);
                }
            };
            const onDown = () => {
                const env = envRef.current;
                if (!env.enabled) return;
                pressed = true;
                const f = env.preset.feel;
                const t = env.preset.timing;
                gsap.to(el, {
                    scale: f.tapScale,
                    duration: scaleDuration(t.durationFast, env.speed) * 0.55,
                    ease: t.ease.press,
                });
            };
            const releaseTo = (fallback: number) => {
                const env = envRef.current;
                if (!env.enabled) return;
                const f = env.preset.feel;
                const t = env.preset.timing;
                const target = hovered ? f.hoverScale : fallback;
                gsap.to(el, {
                    scale: target,
                    duration: scaleDuration(t.durationBase, env.speed),
                    ease: t.ease.release,
                });
            };
            const onUp = () => {
                if (!pressed) return;
                pressed = false;
                releaseTo(1);
            };
            const env = envRef.current;
            if (!env.enabled) return () => {};
            el.addEventListener('mouseenter', onEnter);
            el.addEventListener('mouseleave', onLeave);
            el.addEventListener('mousedown', onDown);
            el.addEventListener('mouseup', onUp);
            return () => {
                el.removeEventListener('mouseenter', onEnter);
                el.removeEventListener('mouseleave', onLeave);
                el.removeEventListener('mousedown', onDown);
                el.removeEventListener('mouseup', onUp);
            };
        };

        const pop: MotionEnv['pop'] = (el, opts) => {
            const env = envRef.current;
            if (!env.enabled) return;
            const f = env.preset.feel;
            const t = env.preset.timing;
            const peak = opts?.peak ?? f.popPeak;
            if (peak === 1) return;
            const easeStr = t.ease[opts?.ease ?? 'pop'];
            // 用 timeline 而不是 keyframes:控起点为当前 scale,避免叠加 hover 时
            // 起点不是 1 时硬跳。
            const tl = gsap.timeline();
            tl.to(el, {
                scale: peak,
                duration: scaleDuration(t.durationFast, env.speed) * 0.6,
                ease: 'power2.out',
            });
            tl.to(el, {
                scale: 1,
                duration: scaleDuration(t.durationBase, env.speed),
                ease: easeStr,
            });
        };

        const shake: MotionEnv['shake'] = (el) => {
            const env = envRef.current;
            if (!env.enabled) return;
            const f = env.preset.feel;
            const t = env.preset.timing;
            const a = f.shakeAmplitude;
            if (a === 0) return;
            const dur = scaleDuration(t.durationBase, env.speed) * 1.4;
            // CustomWiggle 生成的 ndf-wiggle 让 x 在 0 附近做四次衰减摇摆,
            // 起点和终点都为 0,无须额外归位 tween。比手挂 5 段更平滑。
            gsap.fromTo(el, { x: -a }, { x: 0, duration: dur, ease: 'ndf-wiggle' });
        };

        return { tween, fromTo, bindHover, bindPress, pop, shake };
    }, []);

    // 整体返回值按 primitive 缓存。档位/速度/总开关变化时才重建,
    // 同档位下的父级 setState/重渲都拿到同一个 m 引用,业务依赖 [m] 也稳。
    return useMemo<MotionEnv>(() => {
        const preset = motionPresets[level];
        const ease = preset.timing.ease as Record<EaseKind, string>;
        const duration = (kind: DurationKind = 'base'): number => {
            if (!enabled) return 0;
            const t = preset.timing;
            const base =
                kind === 'fast' ? t.durationFast : kind === 'slow' ? t.durationSlow : t.durationBase;
            return scaleDuration(base, speed);
        };
        const stagger = (): number => {
            if (!enabled) return 0;
            return scaleStagger(preset.timing.stagger, speed);
        };
        return {
            level,
            speed,
            reduced,
            enabled,
            preset,
            duration,
            stagger,
            ease,
            ...helpers,
        };
    }, [level, speed, enabled, reduced, helpers]);
}
