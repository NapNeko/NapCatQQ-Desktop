// 启动屏吉祥物件：logo 外框 + 呼吸光斑 + 两条轨道环 + 四角星芒 + 高光扫过条。
// 从 StartupSplash 主件原样搬移的纯展示 JSX；这些元素被进场/待机/退场三个时间线
// 分别驱动缩放、旋转与透明度，ref 袋由主件持有，这里只负责挂回 DOM。

import React from 'react';
import logoSplash from '../../assets/logo-72.png?inline';
import { BRAND_PULSE_SIZE, ORBIT_RINGS, SPARKLES } from '../../core/domain/bootstrap/splashSpec';
import type { SplashRefs } from './splashRefs';

export const SplashMascot: React.FC<{ refs: SplashRefs }> = ({ refs }) => (
    <div
        ref={refs.logoWrapRef}
        className="relative flex shrink-0 items-center justify-center opacity-0"
    >
        <div
            ref={refs.brandPulseRef}
            className="pointer-events-none absolute left-1/2 top-1/2 rounded-full opacity-0"
            style={{
                width: BRAND_PULSE_SIZE,
                height: BRAND_PULSE_SIZE,
                marginLeft: -BRAND_PULSE_SIZE / 2,
                marginTop: -BRAND_PULSE_SIZE / 2,
                background:
                    'radial-gradient(circle, color-mix(in srgb, var(--brand-400) 18%, transparent) 0%, color-mix(in srgb, var(--accent-400) 8%, transparent) 42%, transparent 70%)',
                filter: 'blur(32px)',
            }}
            aria-hidden
        />

        {ORBIT_RINGS.map((ring, i) => (
            <div
                key={i}
                ref={(el) => {
                    refs.ringRefs.current[i] = el;
                }}
                className={
                    'ndf-splash-orbit-ring opacity-0' +
                    (ring.solid ? ' ndf-splash-orbit-ring--solid' : '')
                }
                style={{
                    width: ring.size,
                    height: ring.size,
                    marginLeft: -ring.size / 2,
                    marginTop: -ring.size / 2,
                }}
                aria-hidden
            >
                <span
                    className={
                        'ndf-splash-orbit-dot' +
                        (ring.accentDot ? ' ndf-splash-orbit-dot--accent' : '')
                    }
                    style={ring.dotAtTop ? { top: -3 } : { bottom: -3 }}
                />
            </div>
        ))}

        {SPARKLES.map((sp, idx) => (
            <span
                key={idx}
                ref={(el) => {
                    refs.sparkleRefs.current[idx] = el;
                }}
                className="pointer-events-none absolute z-20 select-none font-bold opacity-0"
                style={{
                    top: sp.top,
                    bottom: sp.bottom,
                    left: sp.left,
                    right: sp.right,
                    fontSize: `${sp.size}px`,
                    color: sp.color,
                    filter: 'drop-shadow(0 0 6px currentColor)',
                }}
                aria-hidden
            >
                {sp.char}
            </span>
        ))}

        <div
            ref={refs.logoBoxRef}
            className="ndf-splash-logo-shine z-10 rounded-2xl ring-1 ring-border-subtle shadow-[0_12px_32px_-4px_rgba(0,0,0,0.35),0_0_28px_-4px_color-mix(in_srgb,var(--brand-400)_25%,transparent)] opacity-0"
        >
            <img
                ref={refs.logoRef}
                src={logoSplash}
                alt=""
                width={72}
                height={72}
                className="block h-[72px] w-[72px] rounded-2xl"
                draggable={false}
            />
            <div
                ref={refs.shineRef}
                className="ndf-splash-shine-stripe z-[1] mix-blend-overlay"
                aria-hidden
            />
        </div>
    </div>
);
