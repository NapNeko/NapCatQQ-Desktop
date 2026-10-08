// 启动屏三个时间线 hook（进场 / 待机 / 退场）共用的元素 ref 袋与上下文类型。
// 集中定义一份，避免三个 hook 各写一遍二十多个参数的签名；ref 由组件持有，
// hook 只在 effect 里读 current，不进依赖数组。

import type { MutableRefObject, RefObject } from 'react';
import type { MotionEnv } from '../../hooks/preferences/useMotion';

export interface SplashRefs {
    rootRef: RefObject<HTMLDivElement>;
    stageRef: RefObject<HTMLDivElement>;
    logoRef: RefObject<HTMLImageElement>;
    logoWrapRef: RefObject<HTMLDivElement>;
    logoBoxRef: RefObject<HTMLDivElement>;
    brandPulseRef: RefObject<HTMLDivElement>;
    sparkRef: RefObject<HTMLDivElement>;
    focusRingRef: RefObject<HTMLDivElement>;
    shockRef: RefObject<HTMLDivElement>;
    rimRef: RefObject<HTMLDivElement>;
    titleRef: RefObject<HTMLHeadingElement>;
    subRef: RefObject<HTMLDivElement>;
    subTextRef: RefObject<HTMLSpanElement>;
    barRef: RefObject<HTMLDivElement>;
    barTrackRef: RefObject<HTMLDivElement>;
    barShineRef: RefObject<HTMLDivElement>;
    glowRef: RefObject<HTMLDivElement>;
    auroraRef: RefObject<HTMLDivElement>;
    versionRef: RefObject<HTMLParagraphElement>;
    shineRef: RefObject<HTMLDivElement>;
    particleRefs: MutableRefObject<(HTMLSpanElement | null)[]>;
    ringRefs: MutableRefObject<(HTMLDivElement | null)[]>;
    sparkleRefs: MutableRefObject<(HTMLSpanElement | null)[]>;
}

/// 三个时间线 hook 都需要的共享输入：元素 ref + 动效环境快照 + 两档降级开关。
export interface SplashFxContext {
    refs: SplashRefs;
    motion: MotionEnv;
    isRich: boolean;
    flourish: boolean;
}
