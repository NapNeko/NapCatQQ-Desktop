// 主界面 spotlight：半透明遮罩 + 四块遮罩挖孔 + 气泡。
// 锚点：document.querySelector(`[data-tour-id="..."]`)。
// 量洞：先 onBeforeStep / scroll，再量；scroll 后再补量一次，减轻漂移。
// 步骤切换：挖孔几何用 GSAP 插值形变（直接写 DOM），文案用 DialogStepTransition。
// 形变期间 lock remeasure，避免 scroll/RO 把洞 snap 掉。

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import gsap from 'gsap';
import { ArrowRight, X } from 'lucide-react';
import { useMotion } from '../../../hooks/preferences/useMotion';
import { Button } from '../../ui';
import { DialogStepTransition } from '../../ui/motion';
import { cn } from '../../utils/cn';

export type SpotlightStep = {
    id: string;
    target: string;
    title: string;
    body: string;
};

type Hole = { top: number; left: number; width: number; height: number };

const PAD = 8;
const RADIUS = 10;
const TIP_W = 320;
const TIP_H_EST = 176;

/** 主锚点找不到时的备选（如空列表时 FAB 仍在，但也可指空态按钮） */
const TARGET_FALLBACKS: Readonly<Record<string, readonly string[]>> = {
    'bot-create-fab': ['bot-create-empty'],
    // 连接 Tab 内容未挂载时先指 Tab 触发器
    'bot-connections-body': ['bot-connections-tab'],
};

function measureOne(tourId: string): Hole | null {
    const el = document.querySelector<HTMLElement>(`[data-tour-id="${tourId}"]`);
    if (!el) return null;
    const r = el.getBoundingClientRect();
    if (r.width < 2 || r.height < 2) return null;
    return {
        top: Math.max(0, r.top - PAD),
        left: Math.max(0, r.left - PAD),
        width: r.width + PAD * 2,
        height: r.height + PAD * 2,
    };
}

function measureTarget(tourId: string): Hole | null {
    const primary = measureOne(tourId);
    if (primary) return primary;
    for (const alt of TARGET_FALLBACKS[tourId] ?? []) {
        const hole = measureOne(alt);
        if (hole) return hole;
    }
    return null;
}

function queryTourEl(tourId: string): HTMLElement | null {
    const primary = document.querySelector<HTMLElement>(`[data-tour-id="${tourId}"]`);
    if (primary) return primary;
    for (const alt of TARGET_FALLBACKS[tourId] ?? []) {
        const el = document.querySelector<HTMLElement>(`[data-tour-id="${alt}"]`);
        if (el) return el;
    }
    return null;
}

function waitForTarget(tourId: string, attempts = 36): Promise<Hole | null> {
    // 连接 Tab 内容随 forceTab 延迟挂载，多等几帧
    const max =
        tourId === 'bot-connections-body' || tourId === 'bot-connections-tab'
            ? Math.max(attempts, 56)
            : attempts;
    return new Promise((resolve) => {
        let n = 0;
        const tick = () => {
            const hole = measureTarget(tourId);
            if (hole) {
                resolve(hole);
                return;
            }
            n += 1;
            if (n >= max) {
                resolve(null);
                return;
            }
            requestAnimationFrame(() => requestAnimationFrame(tick));
        };
        tick();
    });
}

function sleep(ms: number) {
    return new Promise<void>((r) => setTimeout(r, ms));
}

function tipPosition(hole: Hole | null, vw: number, vh: number) {
    if (!hole) {
        return { top: 24, left: 24 };
    }
    const below = hole.top + hole.height + 12;
    const above = hole.top - TIP_H_EST - 12;
    const top = below + TIP_H_EST < vh - 16 ? below : Math.max(16, above);
    const left = Math.min(Math.max(16, hole.left), Math.max(16, vw - TIP_W));
    return { top, left };
}

export interface SpotlightTourProps {
    open: boolean;
    steps: readonly SpotlightStep[];
    stepIndex: number;
    onStepIndexChange: (index: number) => void;
    onClose: (reason: 'skip' | 'done') => void;
    onBeforeStep?: (step: SpotlightStep, index: number) => void | Promise<void>;
}

export function SpotlightTour({
    open,
    steps,
    stepIndex,
    onStepIndexChange,
    onClose,
    onBeforeStep,
}: SpotlightTourProps) {
    const m = useMotion();
    const [missing, setMissing] = useState(false);
    // 仅驱动 tip 文案区；挖孔几何不走 React state，避免每帧 setState 打断形变
    const [tipHole, setTipHole] = useState<Hole | null>(null);
    const step = steps[stepIndex] ?? null;
    const isLast = stepIndex >= steps.length - 1;

    const genRef = useRef(0);
    const onBeforeStepRef = useRef(onBeforeStep);
    onBeforeStepRef.current = onBeforeStep;

    const holeRef = useRef<Hole | null>(null);
    const proxyRef = useRef({ top: 0, left: 0, width: 0, height: 0 });
    const holeTweenRef = useRef<gsap.core.Tween | null>(null);
    /** 形变 / 准备下一步时禁止 scroll·RO 把洞 snap 掉 */
    const lockRemeasureRef = useRef(false);

    const topRef = useRef<HTMLDivElement>(null);
    const bottomRef = useRef<HTMLDivElement>(null);
    const leftRef = useRef<HTMLDivElement>(null);
    const rightRef = useRef<HTMLDivElement>(null);
    const ringRef = useRef<HTMLDivElement>(null);
    const tipRef = useRef<HTMLDivElement>(null);

    const killHoleTween = useCallback(() => {
        holeTweenRef.current?.kill();
        holeTweenRef.current = null;
    }, []);

    /** 把洞几何写到四块遮罩 + 高亮环 + 气泡位置（同步 DOM） */
    const paintHole = useCallback((hole: Hole | null) => {
        const top = topRef.current;
        const bottom = bottomRef.current;
        const left = leftRef.current;
        const right = rightRef.current;
        const ring = ringRef.current;
        const tip = tipRef.current;
        if (!top || !bottom || !left || !right || !ring) return;

        if (!hole) {
            top.style.top = '0px';
            top.style.left = '0px';
            top.style.right = '0px';
            top.style.width = 'auto';
            top.style.height = '100%';
            bottom.style.top = '0px';
            bottom.style.left = '0px';
            bottom.style.right = '0px';
            bottom.style.bottom = 'auto';
            bottom.style.height = '0px';
            left.style.top = '0px';
            left.style.left = '0px';
            left.style.width = '0px';
            left.style.height = '0px';
            right.style.top = '0px';
            right.style.left = '0px';
            right.style.right = 'auto';
            right.style.width = '0px';
            right.style.height = '0px';
            ring.style.opacity = '0';
            ring.style.top = '0px';
            ring.style.left = '0px';
            ring.style.width = '0px';
            ring.style.height = '0px';
        } else {
            top.style.top = '0px';
            top.style.left = '0px';
            top.style.right = '0px';
            top.style.width = 'auto';
            top.style.height = `${Math.max(0, hole.top)}px`;

            bottom.style.top = `${hole.top + hole.height}px`;
            bottom.style.left = '0px';
            bottom.style.right = '0px';
            bottom.style.bottom = '0px';
            bottom.style.height = 'auto';

            left.style.top = `${hole.top}px`;
            left.style.left = '0px';
            left.style.width = `${Math.max(0, hole.left)}px`;
            left.style.height = `${hole.height}px`;

            right.style.top = `${hole.top}px`;
            right.style.left = `${hole.left + hole.width}px`;
            right.style.right = '0px';
            right.style.width = 'auto';
            right.style.height = `${hole.height}px`;

            ring.style.opacity = '1';
            ring.style.top = `${hole.top}px`;
            ring.style.left = `${hole.left}px`;
            ring.style.width = `${hole.width}px`;
            ring.style.height = `${hole.height}px`;
            ring.style.borderRadius = `${RADIUS}px`;
        }

        if (tip && typeof window !== 'undefined') {
            const pos = tipPosition(hole, window.innerWidth, window.innerHeight);
            tip.style.top = `${pos.top}px`;
            tip.style.left = `${pos.left}px`;
        }
    }, []);

    const snapHole = useCallback(
        (next: Hole | null) => {
            killHoleTween();
            lockRemeasureRef.current = false;
            holeRef.current = next;
            if (next) {
                proxyRef.current = { ...next };
            }
            paintHole(next);
            setTipHole(next);
            setMissing(!next);
        },
        [killHoleTween, paintHole],
    );

    /** 从上一个洞形变到 next；无起点或关动画则直接贴 */
    const morphHoleTo = useCallback(
        (next: Hole | null) => {
            setMissing(!next);

            if (!next) {
                snapHole(null);
                return;
            }

            const from = holeRef.current;
            if (!m.enabled || !from) {
                snapHole(next);
                return;
            }

            const same =
                Math.abs(from.top - next.top) < 0.5 &&
                Math.abs(from.left - next.left) < 0.5 &&
                Math.abs(from.width - next.width) < 0.5 &&
                Math.abs(from.height - next.height) < 0.5;
            if (same) {
                snapHole(next);
                return;
            }

            killHoleTween();
            lockRemeasureRef.current = true;

            const proxy = proxyRef.current;
            proxy.top = from.top;
            proxy.left = from.left;
            proxy.width = from.width;
            proxy.height = from.height;

            const duration = m.duration('base') || 0.32;
            const ease = m.ease.enter;

            holeTweenRef.current = gsap.fromTo(
                proxy,
                {
                    top: from.top,
                    left: from.left,
                    width: from.width,
                    height: from.height,
                },
                {
                    top: next.top,
                    left: next.left,
                    width: next.width,
                    height: next.height,
                    duration,
                    ease,
                    onUpdate: () => {
                        const h: Hole = {
                            top: proxy.top,
                            left: proxy.left,
                            width: proxy.width,
                            height: proxy.height,
                        };
                        holeRef.current = h;
                        paintHole(h);
                    },
                    onComplete: () => {
                        holeRef.current = next;
                        proxyRef.current = { ...next };
                        paintHole(next);
                        setTipHole(next);
                        holeTweenRef.current = null;
                        lockRemeasureRef.current = false;
                    },
                },
            );

            // 文案可先跟新步；洞继续形变
            setTipHole(next);
        },
        [killHoleTween, m.enabled, m.duration, m.ease.enter, paintHole, snapHole],
    );

    const remeasureOnly = useCallback(() => {
        if (!open || !step) return;
        // 形变 / 切步准备中：绝不 snap，否则洞会瞬移
        if (lockRemeasureRef.current || holeTweenRef.current) return;

        const next = measureTarget(step.target);
        if (!next) {
            // 目标暂时没了：保留当前洞，只标 missing，避免闪全屏
            setMissing(true);
            return;
        }
        setMissing(false);
        holeRef.current = next;
        proxyRef.current = { ...next };
        paintHole(next);
        setTipHole(next);
    }, [open, paintHole, step]);

    const prepareStep = useCallback(async () => {
        if (!open || !step) {
            snapHole(null);
            return;
        }

        const gen = ++genRef.current;
        // 切步期间锁 remeasure；保留上一洞作为 morph 起点
        lockRemeasureRef.current = true;

        if (onBeforeStepRef.current) {
            await onBeforeStepRef.current(step, stepIndex);
        }
        if (gen !== genRef.current) return;

        let next = await waitForTarget(step.target);
        if (gen !== genRef.current) return;

        if (next) {
            const el = queryTourEl(step.target);
            // instant：少触发中间 layout；即便 scroll 冒泡也被 lock 挡住
            el?.scrollIntoView({
                block: 'nearest',
                inline: 'nearest',
                behavior: 'instant',
            });
            await sleep(32);
            await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
            if (gen !== genRef.current) return;
            next = measureTarget(step.target) ?? next;
        }
        if (gen !== genRef.current) return;

        morphHoleTo(next);
        // morph 自己的 onComplete 会解锁；无动画 snap 已解锁
        if (!holeTweenRef.current) {
            lockRemeasureRef.current = false;
        }
    }, [morphHoleTo, open, snapHole, step, stepIndex]);

    useLayoutEffect(() => {
        void prepareStep();
    }, [prepareStep]);

    useEffect(() => {
        if (!open) {
            killHoleTween();
            lockRemeasureRef.current = false;
            holeRef.current = null;
            setTipHole(null);
            setMissing(false);
        }
    }, [open, killHoleTween]);

    // refs 就绪后补画一次
    useLayoutEffect(() => {
        if (!open) return;
        paintHole(holeRef.current);
    }, [open, paintHole]);

    useEffect(() => {
        if (!open || !step) return;
        const onWin = () => remeasureOnly();
        window.addEventListener('resize', onWin);
        window.addEventListener('scroll', onWin, true);
        const ro =
            typeof ResizeObserver !== 'undefined'
                ? new ResizeObserver(() => remeasureOnly())
                : null;
        const el = queryTourEl(step.target);
        if (el && ro) ro.observe(el);
        return () => {
            window.removeEventListener('resize', onWin);
            window.removeEventListener('scroll', onWin, true);
            ro?.disconnect();
        };
    }, [open, step, remeasureOnly]);

    useEffect(() => {
        if (!open) return;
        const onKey = (e: KeyboardEvent) => {
            if (e.key === 'Escape') {
                e.preventDefault();
                onClose('skip');
            }
        };
        window.addEventListener('keydown', onKey);
        return () => window.removeEventListener('keydown', onKey);
    }, [open, onClose]);

    // 文案区淡入（与洞形变并行）
    useEffect(() => {
        const tip = tipRef.current;
        if (!open || !tip) return;
        if (!m.enabled) {
            gsap.set(tip, { autoAlpha: 1, y: 0 });
            return;
        }
        gsap.fromTo(
            tip,
            { autoAlpha: 0.75, y: 6 },
            {
                autoAlpha: 1,
                y: 0,
                duration: m.duration('fast') || 0.18,
                ease: m.ease.enterMicro,
                clearProps: 'transform',
            },
        );
    }, [step?.id, open, m.enabled, m.duration, m.ease.enterMicro]);

    if (!open || !step || typeof document === 'undefined') return null;

    const vw = typeof window !== 'undefined' ? window.innerWidth : 0;
    const vh = typeof window !== 'undefined' ? window.innerHeight : 0;
    const initialTip = tipPosition(tipHole, vw, vh);

    return createPortal(
        <div
            className="fixed inset-0 z-[180]"
            role="dialog"
            aria-modal="true"
            aria-label="框架引导"
        >
            {/* 四块遮罩常驻；几何由 paintHole / GSAP 写 style，不靠 React 重渲 */}
            <div
                ref={topRef}
                className="absolute bg-black/55"
                style={{ top: 0, left: 0, right: 0, height: '100%' }}
            />
            <div ref={bottomRef} className="absolute bg-black/55" style={{ height: 0 }} />
            <div ref={leftRef} className="absolute bg-black/55" style={{ width: 0, height: 0 }} />
            <div ref={rightRef} className="absolute bg-black/55" style={{ width: 0, height: 0 }} />
            <div
                ref={ringRef}
                className="pointer-events-none absolute rounded-md ring-2 ring-brand ring-offset-2 ring-offset-transparent"
                style={{ opacity: 0, borderRadius: RADIUS }}
            />

            <div
                ref={tipRef}
                className={cn(
                    'absolute z-10 w-[min(100%-2rem,20rem)] rounded-lg border border-border-subtle',
                    'bg-elevated p-4 shadow-popover',
                )}
                style={{ top: initialTip.top, left: initialTip.left }}
            >
                <div className="mb-2 flex items-start justify-between gap-2">
                    <p className="text-[11px] font-medium tabular-nums text-text-tertiary">
                        {stepIndex + 1} / {steps.length}
                    </p>
                    <button
                        type="button"
                        className="rounded-xs p-0.5 text-text-tertiary hover:bg-inset hover:text-text"
                        aria-label="跳过引导"
                        onClick={() => onClose('skip')}
                    >
                        <X size={14} strokeWidth={2} />
                    </button>
                </div>
                <DialogStepTransition stepKey={step.id}>
                    <h3 className="font-display text-[15px] font-semibold text-text">
                        {step.title}
                    </h3>
                    <p className="mt-1.5 text-[12.5px] leading-relaxed text-text-secondary">
                        {step.body}
                    </p>
                    {missing ? (
                        <p className="mt-2 text-[11px] text-warning">
                            没找到对应界面元素，可点下一步或跳过。
                        </p>
                    ) : null}
                </DialogStepTransition>
                <div className="mt-3 flex items-center justify-between gap-2">
                    <Button
                        variant="ghost"
                        size="sm"
                        disabled={stepIndex === 0}
                        onClick={() => onStepIndexChange(Math.max(0, stepIndex - 1))}
                    >
                        上一步
                    </Button>
                    <div className="flex gap-2">
                        <Button variant="ghost" size="sm" onClick={() => onClose('skip')}>
                            跳过
                        </Button>
                        <Button
                            variant="primary"
                            size="sm"
                            onClick={() => {
                                if (isLast) onClose('done');
                                else onStepIndexChange(stepIndex + 1);
                            }}
                        >
                            {isLast ? '完成' : '下一步'}
                            {!isLast ? (
                                <ArrowRight size={14} strokeWidth={2} />
                            ) : null}
                        </Button>
                    </div>
                </div>
            </div>
        </div>,
        document.body,
    );
}

