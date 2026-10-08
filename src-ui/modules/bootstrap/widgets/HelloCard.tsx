// 概览顶部 Hello 卡：天色背景 + 问候语 + 状态内联行 + 吉祥物。
// 每分钟重取一次时间让问候语跨时段换；点天空炸星星、实例状态突变时吉祥物主动说话。

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import gsap from 'gsap';
import { AlertTriangle, Server, ThumbsUp } from 'lucide-react';
import { Card } from '../../../shared/ui';
import { Counter } from '../../../shared/ui/motion';
import { usePreferences } from '../../../hooks/preferences/usePreferences';
import { useMotion, type MotionEnv } from '../../../hooks/preferences/useMotion';
import { useOpenExternal } from '../../../hooks/useOpenExternal';
import {
    getDayPhase,
    getGreeting,
    greetingSeed,
    type DayPhase,
} from '../../../core/domain/overview/dayPhase';
import type { BotFleetStats } from '../../../core/domain/overview/glance';
import {
    fleetHint,
    pick,
    ALARM_LINES,
    RECOVERED_LINES,
    ONLINE_LINES,
    HALTED_LINES,
    mascotQuips,
} from '../../../core/domain/bootstrap/heroCopy';
import { HeroSky } from './HeroSky';
import { HeroTitle } from './HeroTitle';
import { HeroMascot, type MascotReaction } from './HeroMascot';
import type { AppRoute } from '../../../shared/components/next/Sidebar';

export interface HelloCardProps {
    fleet: BotFleetStats;
    /** 实例快照至少拿到过一次；之前的数字变化不算「发生了事」。 */
    fleetReady: boolean;
    actionableCount: number;
    serverCount: number;
    onNavigate: (route: AppRoute) => void;
}

export const HelloCard: React.FC<HelloCardProps> = ({
    fleet,
    fleetReady,
    actionableCount,
    serverCount,
    onNavigate,
}) => {
    const { showMascot } = usePreferences();
    const m = useMotion();
    const openExternal = useOpenExternal();
    const stageRef = useRef<HTMLDivElement>(null);
    const now = useMinuteClock();
    const phase = getDayPhase(now.getHours());
    const { title, hint: greetingHint } = getGreeting(phase, greetingSeed(now));
    const hint = fleetHint(fleet) ?? greetingHint;
    const runningCount = fleet.running;
    const quips = useMemo(() => mascotQuips(fleet, actionableCount), [fleet, actionableCount]);
    const reaction = useFleetReaction(fleet, actionableCount, fleetReady);
    useDayPhaseOnRoot(phase);
    const burstSparks = useSkySparks(stageRef, m);

    return (
        <Card
            ref={stageRef}
            variant="hero"
            className="relative overflow-visible py-7 px-6 sm:px-7 min-h-[212px]"
            onClick={burstSparks}
        >
            <HeroSky
                phase={phase}
                hour={now.getHours()}
                minute={now.getMinutes()}
                motionEnabled={m.enabled}
            />

            <div className="relative z-10 max-w-[340px] pr-2 sm:pr-0">
                <HeroTitle
                    title={title}
                    className="font-display text-[36px] font-extrabold leading-none tracking-tight text-[var(--text-hero-title)]"
                />
                <p className="mt-3 text-[14px] leading-relaxed text-text-secondary">{hint}</p>

                {/* 状态与导航内联行：纯文字排版与细致微标 */}
                <div className="mt-5 flex flex-wrap items-center gap-x-3 gap-y-1.5 text-xs text-text-secondary">
                    <button
                        type="button"
                        onClick={() => onNavigate('bots')}
                        className="inline-flex items-center gap-1.5 transition-colors hover:text-text cursor-pointer select-none"
                    >
                        <span
                            className={`h-1.5 w-1.5 rounded-full ${
                                runningCount > 0
                                    ? 'bg-success shadow-glow-success'
                                    : 'bg-text-disabled'
                            }`}
                        />
                        <span>
                            <strong className="font-mono font-semibold text-text tabular-nums">
                                <Counter value={runningCount} />
                            </strong>{' '}
                            个实例运行中
                        </span>
                    </button>

                    {actionableCount > 0 && (
                        <>
                            <span className="text-border-subtle select-none" aria-hidden>
                                ·
                            </span>
                            <button
                                type="button"
                                onClick={() => onNavigate('bots')}
                                className="inline-flex items-center gap-1.5 text-danger transition-colors hover:text-danger/80 cursor-pointer select-none"
                            >
                                <AlertTriangle size={12} strokeWidth={2} />
                                <span>
                                    <strong className="font-mono font-semibold tabular-nums">
                                        <Counter value={actionableCount} />
                                    </strong>{' '}
                                    个异常
                                </span>
                            </button>
                        </>
                    )}

                    <span className="text-border-subtle select-none" aria-hidden>
                        ·
                    </span>

                    <button
                        type="button"
                        onClick={() => onNavigate('remote')}
                        className="inline-flex items-center gap-1.5 transition-colors hover:text-text cursor-pointer select-none"
                    >
                        <Server size={12} className="text-info opacity-90" />
                        <span>
                            <strong className="font-mono font-semibold text-text tabular-nums">
                                <Counter value={serverCount} />
                            </strong>{' '}
                            台主机
                        </span>
                    </button>

                    <span className="text-border-subtle select-none" aria-hidden>
                        ·
                    </span>

                    <button
                        type="button"
                        onClick={() => openExternal('https://github.com/NapNeko/NapCatQQ-Desktop')}
                        className="inline-flex items-center gap-1 text-[var(--text-hero-accent)] hover:underline cursor-pointer select-none"
                    >
                        <ThumbsUp size={12} strokeWidth={2} />
                        <span>GitHub Star</span>
                    </button>
                </div>
            </div>

            {/* mascot：破圈悬浮，站在天色前面 */}
            {showMascot && (
                <HeroMascot
                    stageRef={stageRef}
                    quips={quips}
                    reaction={reaction}
                    className="absolute -top-9 right-2 z-20 hidden md:block lg:right-6"
                />
            )}
        </Card>
    );
};

// 每分钟醒一次：问候语跨时段要换，太阳月亮也要挪。
function useMinuteClock(): Date {
    const [now, setNow] = useState(() => new Date());
    useEffect(() => {
        const id = window.setInterval(() => setNow(new Date()), 60_000);
        return () => window.clearInterval(id);
    }, []);
    return now;
}

// 整页角落柔光跟着 Hello 卡的天色走；离开概览就摘掉。
function useDayPhaseOnRoot(phase: DayPhase): void {
    useEffect(() => {
        const root = document.documentElement;
        root.setAttribute('data-day-phase', phase);
        return () => root.removeAttribute('data-day-phase');
    }, [phase]);
}

// 实例出事 / 恢复 / 从零到有上线 / 全停时，吉祥物主动说一句。首屏拿到数据前的变化不算。
function useFleetReaction(
    fleet: BotFleetStats,
    actionableCount: number,
    ready: boolean,
): MascotReaction | null {
    const [reaction, setReaction] = useState<MascotReaction | null>(null);
    const prevRef = useRef<{ actionable: number; running: number } | null>(null);
    useEffect(() => {
        if (!ready) return;
        const prev = prevRef.current;
        prevRef.current = { actionable: actionableCount, running: fleet.running };
        if (!prev) return;
        if (actionableCount > prev.actionable) {
            setReaction({
                key: Date.now(),
                text: `${actionableCount} ${pick(ALARM_LINES)}`,
                alarm: true,
            });
        } else if (prev.actionable > 0 && actionableCount === 0) {
            setReaction({ key: Date.now(), text: pick(RECOVERED_LINES) });
        } else if (prev.running === 0 && fleet.running > 0) {
            setReaction({ key: Date.now(), text: pick(ONLINE_LINES) });
        } else if (prev.running > 0 && fleet.running === 0) {
            setReaction({ key: Date.now(), text: pick(HALTED_LINES) });
        }
    }, [ready, actionableCount, fleet.running]);
    return reaction;
}

// 点天空（不是按钮）炸出几颗小星星。星星挂在卡片上，跟 HeroSky 用同一套墨色。
function useSkySparks(
    stageRef: React.RefObject<HTMLDivElement>,
    m: MotionEnv,
): (e: React.MouseEvent<HTMLDivElement>) => void {
    return useCallback(
        (e) => {
            const stage = stageRef.current;
            if (!stage || !m.enabled || m.preset.feel.popPeak === 1) return;
            if ((e.target as HTMLElement).closest('button, a, [role="button"]')) return;
            const rect = stage.getBoundingClientRect();
            const x = e.clientX - rect.left;
            const y = e.clientY - rect.top;
            const sky = stage.querySelector('.ndf-hero-sky');
            if (!sky) return;
            const count = m.preset.feel.overshoot ? 8 : 6;
            for (let i = 0; i < count; i += 1) {
                const spark = document.createElement('span');
                spark.className = 'ndf-hero-spark';
                spark.style.left = `${x}px`;
                spark.style.top = `${y}px`;
                sky.appendChild(spark);
                const angle = (Math.PI * 2 * i) / count + (Math.random() - 0.5) * 0.6;
                const dist = 26 + Math.random() * 30;
                gsap.fromTo(
                    spark,
                    { scale: 0.4, autoAlpha: 1 },
                    {
                        x: Math.cos(angle) * dist,
                        y: Math.sin(angle) * dist - 8,
                        scale: 1,
                        autoAlpha: 0,
                        duration: m.duration('slow') * 2.2,
                        ease: 'power2.out',
                        onComplete: () => spark.remove(),
                    },
                );
            }
        },
        [stageRef, m],
    );
}
