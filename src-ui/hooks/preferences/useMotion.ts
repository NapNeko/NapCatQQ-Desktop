// useMotion: 所有 GSAP 动画读偏好的统一入口。
//
// 职责:
//   1. 整合 preferencesStore + 系统 prefers-reduced-motion
//   2. 提供 duration(kind) helper 让业务直接拿到秒数
//   3. 暴露 preset 让需要 ease/scale/stagger 的业务自取
//
// 关键差别于 framer 版:
//   - 没有 transition 对象返回,GSAP 用扁平的 {duration, ease, ...} 配置
//   - reduced 命中时 duration → 0,业务侧仍可放心调 gsap.to(),只是瞬时跳到终态
//   - GSAP 对 prefers-reduced-motion 的官方推荐是 gsap.matchMedia(),但那需要
//     在 effect 内部组织代码,跟 React state 驱动不太搭。我们这层用 React 监听
//     系统 media query,然后透传给 useGSAP 调用方,让它们自己按 reduced=true
//     直接 gsap.set() 跳过动画

import { useEffect, useState } from 'react';
import { usePreferences } from './preferencesStore';
import {
    motionPresets,
    scaleDuration,
    type MotionLevel,
} from '../../core/design/motion';

export type DurationKind = 'fast' | 'base' | 'slow';

export interface MotionEnv {
    /// 用户选的档位。系统 reduced-motion 也不修改本字段(展示用)。
    level: MotionLevel;
    /// 用户拖的速度倍率,clamp 后的有效值。
    speed: number;
    /// 系统 prefers-reduced-motion 是否命中。
    reduced: boolean;
    /// 综合后真正生效的 enabled:用户开关 && 非 reduced。
    enabled: boolean;
    /// 当前档位 preset(ease/scale/stagger 等)。
    preset: typeof motionPresets[MotionLevel];
    /// 取一个 duration(秒)。enabled=false 时返回 0,业务直接用即可,不用判 enabled。
    duration: (kind?: DurationKind) => number;
}

/// 监听系统 prefers-reduced-motion。useGSAP 也支持 matchMedia,但 React 状态
/// 驱动的动画(GsapPresence 这种)更适合从 hook 出口拿到布尔值后自己分支。
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
    const preset = motionPresets[level];

    const duration = (kind: DurationKind = 'base'): number => {
        if (!enabled) return 0;
        const base = kind === 'fast'
            ? preset.durationFast
            : kind === 'slow'
                ? preset.durationSlow
                : preset.durationBase;
        return scaleDuration(base, speed);
    };

    return { level, speed, reduced, enabled, preset, duration };
}
