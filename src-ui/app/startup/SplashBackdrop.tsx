// 启动屏背景层：底色光晕、极光层、漂浮星点、以 logo 为圆心的特效环。
// 从 StartupSplash 主件原样搬移的纯展示 JSX，作为 fragment 嵌在主件根容器里；
// DOM 顺序与原来一致，特效环位置由进场时间线按 logo 实测中心写入。

import React from 'react';
import { RING_FX_SIZE, SPLASH_PARTICLES } from '../../core/domain/bootstrap/splashSpec';
import type { SplashRefs } from './splashRefs';

export const SplashBackdrop: React.FC<{
    refs: SplashRefs;
    motionEnabled: boolean;
}> = ({ refs, motionEnabled }) => {
    const ringFxStyle = { marginLeft: -RING_FX_SIZE / 2, marginTop: -RING_FX_SIZE / 2 };
    return (
        <>
            <div
                ref={refs.glowRef}
                className="ndf-canvas-glow pointer-events-none absolute inset-0 opacity-0"
                aria-hidden
            />
            <div ref={refs.auroraRef} className="ndf-splash-aurora-layer opacity-0" aria-hidden>
                {(['a', 'b', 'c'] as const).map((key) => (
                    <div
                        key={key}
                        className={
                            `ndf-splash-aurora ndf-splash-aurora--${key}` +
                            (motionEnabled ? ' is-live' : '')
                        }
                    />
                ))}
            </div>
            {SPLASH_PARTICLES.map((p, i) => (
                <span
                    key={i}
                    ref={(el) => {
                        refs.particleRefs.current[i] = el;
                    }}
                    className={
                        'ndf-splash-particle opacity-0' +
                        (p.accent ? ' ndf-splash-particle--accent' : '')
                    }
                    style={{
                        left: `${p.left}%`,
                        top: `${p.top}%`,
                        width: p.size,
                        height: p.size,
                    }}
                    aria-hidden
                />
            ))}

            {/* 以 logo 为圆心的特效挂在根层：位置由 GSAP 按 logo 实测中心写入，
                不放进 logoWrap 是因为 logoWrap 自己会整体缩放/淡出 */}
            <div ref={refs.sparkRef} className="ndf-splash-spark z-20 opacity-0" aria-hidden />
            <div
                ref={refs.focusRingRef}
                className="ndf-splash-shockwave ndf-splash-shockwave--thin z-20 opacity-0"
                style={ringFxStyle}
                aria-hidden
            />
            {/* 退场冲击波垫在 stage（z-10）之下，从 logo 背后扩开 */}
            <div
                ref={refs.shockRef}
                className="ndf-splash-shockwave z-0 opacity-0"
                style={ringFxStyle}
                aria-hidden
            />
            <div
                ref={refs.rimRef}
                className="ndf-splash-shockwave ndf-splash-shockwave--rim z-20 opacity-0"
                style={ringFxStyle}
                aria-hidden
            />
        </>
    );
};
