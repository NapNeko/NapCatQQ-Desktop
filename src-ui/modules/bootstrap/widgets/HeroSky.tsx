// Hello 卡背后的「此刻天色」：整卡天幕 + 太阳 / 月牙 + 白天飘云 + 夜里星星与流星。
// 文字区靠左，上面压一层 --surface-hero 的雾，天色再浓也不动可读性。

import React, { useMemo } from 'react';
import {
    celestialPosition,
    cloudField,
    starCountFor,
    starField,
    type DayPhase,
} from '../../../core/domain/overview/dayPhase';

interface HeroSkyProps {
    phase: DayPhase;
    hour: number;
    minute: number;
    motionEnabled: boolean;
}

export const HeroSky: React.FC<HeroSkyProps> = ({ phase, hour, minute, motionEnabled }) => {
    const body = useMemo(() => celestialPosition(hour, minute), [hour, minute]);
    const stars = useMemo(() => starField(starCountFor(phase)), [phase]);
    const clouds = useMemo(() => cloudField(phase), [phase]);
    const live = motionEnabled ? ' is-live' : '';

    return (
        <div
            aria-hidden
            className={`ndf-hero-sky ndf-hero-sky--${phase}${live} pointer-events-none absolute inset-0 overflow-hidden rounded-[inherit]`}
        >
            <div className="ndf-hero-sky__gradient absolute inset-0" />

            {body.kind === 'sun' ? (
                <div
                    className="ndf-hero-sun absolute"
                    style={{
                        left: `${body.x}%`,
                        top: `${body.y}%`,
                        // 中天最亮最大；贴地平线时缩一点、暗一点。
                        transform: `translate(-50%, -50%) scale(${0.85 + 0.25 * body.altitude})`,
                        opacity: 0.75 + 0.25 * body.altitude,
                    }}
                />
            ) : (
                <svg
                    className="ndf-hero-moon absolute"
                    viewBox="0 0 24 24"
                    style={{
                        left: `${body.x}%`,
                        top: `${body.y}%`,
                        transform: 'translate(-50%, -50%)',
                    }}
                >
                    <defs>
                        <mask id="ndf-hero-moon-mask">
                            <rect width="24" height="24" fill="#fff" />
                            <circle cx="16.5" cy="9" r="8" fill="#000" />
                        </mask>
                    </defs>
                    <circle cx="12" cy="12" r="9" mask="url(#ndf-hero-moon-mask)" />
                </svg>
            )}

            {clouds.map((c, i) => (
                <span
                    key={i}
                    className="ndf-hero-cloud absolute"
                    style={{
                        top: `${c.y}%`,
                        width: c.width,
                        height: c.width * 0.3,
                        opacity: 0.96 - 0.38 * c.depth,
                        animationDuration: `${c.duration}s`,
                        animationDelay: `${c.delay}s`,
                    }}
                />
            ))}

            <div className="ndf-hero-sky__haze absolute inset-0" />

            {stars.map((s, i) => (
                <span
                    key={i}
                    className="ndf-hero-star absolute rounded-full"
                    style={{
                        left: `${s.x}%`,
                        top: `${s.y}%`,
                        width: s.size,
                        height: s.size,
                        animationDuration: `${s.period}s`,
                        animationDelay: `${s.delay}s`,
                    }}
                />
            ))}

            {phase === 'night' && <span className="ndf-hero-shooting-star absolute" />}
        </div>
    );
};

export default HeroSky;
