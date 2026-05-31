// useMotion: 所有 motion 组件读偏好的统一入口。
//
// 整合 3 个来源:
//   1. preferencesStore 用户偏好(level / speed / enabled)
//   2. framer-motion useReducedMotion() 系统级 prefers-reduced-motion
//   3. 派生 effective 字段:任一关闭即整体禁用
//
// 设计取舍:不直接返回 transition 对象,而是返回原始字段 + 几个派生 helper。
// 业务自己挑要哪一种 transition(base/slow/spring/bouncy),避免一次取多份浪费。

import { useMemo } from 'react';
import { useReducedMotion, type Transition } from 'framer-motion';
import { usePreferences } from './preferencesStore';
import {
    getTransition,
    motionPresets,
    type MotionKind,
    type MotionLevel,
} from '../../core/design/motion';

export interface MotionEnv {
    /// 用户选的档位。系统 reduced-motion 也不修改本字段(展示用)。
    level: MotionLevel;
    /// 用户拖的速度倍率,clamp 后的有效值。
    speed: number;
    /// 系统 prefers-reduced-motion 是否命中。
    reduced: boolean;
    /// 综合后真正生效的 enabled:用户开关 && 非 reduced。
    enabled: boolean;
    /// 取一个 transition。enabled=false 时强制 duration 0 + 退化 spring 为瞬时 tween。
    /// 业务不要自己再判 enabled,直接调本方法。
    transition: (kind?: MotionKind) => Transition;
    /// 派生字段:当前档位 preset(列表 stagger / hover scale 之类的常量)。
    preset: typeof motionPresets[MotionLevel];
}

/// 关闭动画时的"瞬时"transition:duration 0 的 tween。
/// 用 const 引用让 React shallow compare 短路。
const ZERO_TRANSITION: Transition = { duration: 0 };

export function useMotion(): MotionEnv {
    const prefs = usePreferences();
    const reduced = useReducedMotion() ?? false;
    const enabled = prefs.motionEnabled && !reduced;

    return useMemo<MotionEnv>(() => {
        const level = prefs.motionLevel;
        const speed = prefs.motionSpeed;
        const preset = motionPresets[level];

        const transition = (kind: MotionKind = 'base'): Transition => {
            if (!enabled) return ZERO_TRANSITION;
            return getTransition(level, speed, kind);
        };

        return { level, speed, reduced, enabled, transition, preset };
    }, [prefs.motionLevel, prefs.motionSpeed, enabled, reduced]);
}
