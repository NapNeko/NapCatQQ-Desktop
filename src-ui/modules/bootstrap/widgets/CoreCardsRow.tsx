// 概览 Core 双卡：NapCat 与 SnowLuma 的安装状态 + 可更新徽章，点卡跳组件页。
// 图标微交互：摸卡时猫耳抖一抖、雪花转一圈；连点雪花 5 次触发大旋转彩蛋。

import React, { useRef } from 'react';
import gsap from 'gsap';
import { Snowflake } from 'lucide-react';
import { Card, Badge } from '../../../shared/ui';
import { useMotion } from '../../../hooks/preferences/useMotion';
import napcatPng from '../../../assets/napcat-symbol-48.png?inline';
import type { UpdateAvailableItem } from '../../../core/domain/release/normalize';
import { formatVersion } from '../../../core/domain/bootstrap/overviewFormat';
import type { AppRoute } from '../../../shared/components/next/Sidebar';

export interface CoreCardsRowProps {
    napcatVersion: string | null;
    snowlumaVersion: string | null;
    updates: UpdateAvailableItem[];
    onNavigate: (route: AppRoute) => void;
}

export const CoreCardsRow: React.FC<CoreCardsRowProps> = ({
    napcatVersion,
    snowlumaVersion,
    updates,
    onNavigate,
}) => {
    const napcatUpdate = updates.find((u) => u.project === 'napcat');
    const snowlumaUpdate = updates.find((u) => u.project === 'snowluma');

    return (
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <CoreCard
                kind="napcat"
                label="NapCat"
                version={napcatVersion}
                update={napcatUpdate ?? null}
                onNavigate={onNavigate}
            />
            <CoreCard
                kind="snowluma"
                label="SnowLuma"
                version={snowlumaVersion}
                update={snowlumaUpdate ?? null}
                onNavigate={onNavigate}
            />
        </div>
    );
};

interface CoreCardProps {
    kind: 'napcat' | 'snowluma';
    label: string;
    version: string | null;
    update: UpdateAvailableItem | null;
    onNavigate: (route: AppRoute) => void;
}

const CoreCard: React.FC<CoreCardProps> = ({ kind, label, version, update, onNavigate }) => {
    const installed = version !== null;
    const hasUpdate = installed && update !== null;
    const dotClass = installed ? 'bg-success shadow-glow-success' : 'bg-text-disabled/80';
    const m = useMotion();
    const iconRef = useRef<HTMLDivElement>(null);
    const snowflakeClicksRef = useRef({ startedAt: 0, count: 0 });

    // 摸到卡片时图标动一下：猫耳朵抖一抖，雪花转一圈。
    const nudgeIcon = () => {
        const el = iconRef.current;
        if (!el || !m.enabled || m.preset.feel.popPeak === 1) return;
        gsap.killTweensOf(el);
        if (kind === 'napcat') {
            gsap.fromTo(
                el,
                { rotate: -8 },
                { rotate: 0, duration: 0.7 / Math.max(0.5, m.speed), ease: 'ndf-wiggle' },
            );
        } else {
            gsap.fromTo(
                el,
                { rotate: 0 },
                { rotate: 180, duration: m.duration('slow') * 2, ease: m.ease.release },
            );
        }
    };

    const handleSnowflakeClick = (event: React.MouseEvent<HTMLButtonElement>) => {
        const el = iconRef.current;
        if (!el) return;
        if (!m.enabled || m.preset.feel.popPeak === 1) {
            snowflakeClicksRef.current = { startedAt: 0, count: 0 };
            return;
        }

        event.stopPropagation();
        const now = performance.now();
        const clicks = snowflakeClicksRef.current;
        if (clicks.count === 0 || now - clicks.startedAt > 2_000) {
            snowflakeClicksRef.current = { startedAt: now, count: 1 };
            return;
        }
        if (clicks.count < 4) {
            clicks.count += 1;
            return;
        }

        snowflakeClicksRef.current = { startedAt: 0, count: 0 };
        gsap.killTweensOf(el);
        gsap.fromTo(
            el,
            { rotate: 0 },
            { rotate: 900, duration: m.duration('slow') * 2, ease: m.ease.release },
        );
    };

    return (
        <Card
            padding="md"
            hover="lift"
            onClick={() => onNavigate('components')}
            onMouseEnter={nudgeIcon}
            className="flex items-center gap-3.5 transition-all cursor-pointer hover:shadow-popover"
        >
            <div
                className={`grid h-10 w-10 shrink-0 place-items-center rounded-md border border-border-subtle/40 ${
                    kind === 'napcat' ? 'bg-brand-soft/80' : 'bg-info-soft/80'
                }`}
            >
                <div
                    ref={iconRef}
                    className="grid place-items-center"
                    style={{ transformOrigin: '50% 60%' }}
                >
                    {kind === 'napcat' ? (
                        <img
                            src={napcatPng}
                            alt=""
                            className="h-6 w-6 select-none"
                            draggable={false}
                        />
                    ) : (
                        <button
                            type="button"
                            aria-label="SnowLuma 雪花"
                            onClick={handleSnowflakeClick}
                            className="grid cursor-pointer place-items-center rounded-sm border-0 bg-transparent p-0 text-info focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-info"
                        >
                            <Snowflake size={18} strokeWidth={1.75} className="text-info" />
                        </button>
                    )}
                </div>
            </div>

            <div className="min-w-0 flex-1">
                <div className="flex items-center justify-between gap-1">
                    <div className="flex items-center gap-1.5">
                        <span
                            aria-hidden
                            className={`inline-block h-1.5 w-1.5 shrink-0 rounded-full ${dotClass}`}
                        />
                        <p className="truncate font-display text-sm font-semibold leading-none text-text">
                            {label}
                        </p>
                    </div>
                    {hasUpdate && (
                        <Badge
                            tone="warning"
                            appearance="soft"
                            className="text-[10px] px-1 py-0 font-normal"
                        >
                            可更新
                        </Badge>
                    )}
                </div>
                <p
                    className={`mt-1.5 truncate text-[11.5px] tabular-nums ${
                        installed ? 'font-mono text-text-secondary' : 'text-text-tertiary'
                    }`}
                >
                    {installed ? formatVersion(version) : '未安装'}
                </p>
            </div>
        </Card>
    );
};
