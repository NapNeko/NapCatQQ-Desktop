// 动态图标：Lucide SVG + 描边绘制 / 弹入 / 循环动效（零额外依赖）。

import { useEffect, useRef, useState, type ComponentType } from 'react';
import gsap from 'gsap';
import type { LucideProps } from 'lucide-react';
import { useMotion } from '../../../hooks/preferences/useMotion';
import { cn } from '../../utils/cn';

export type MotionIconPreset =
    | 'none'
    | 'pulse'
    | 'breathe'
    | 'wiggle'
    | 'spin'
    | 'spin-slow'
    | 'nudge'
    | 'bob';

export interface MotionIconProps extends LucideProps {
    icon: ComponentType<LucideProps>;
    motion?: MotionIconPreset;
    /// 选中时播一次描边绘制 + 轻弹入（侧栏切换）。
    playEnter?: boolean;
    /// 变化时重播进场（传路由 id）。
    enterKey?: string;
    /// 悬停时短暂 pop，适合工具栏图标按钮。
    hoverAccent?: boolean;
    className?: string;
}

function collectStrokedNodes(svg: SVGSVGElement): SVGGeometryElement[] {
    return Array.from(
        svg.querySelectorAll<SVGGeometryElement>(
            'path, line, circle, rect, polyline, ellipse',
        ),
    ).filter((el) => {
        const stroke = el.getAttribute('stroke');
        return stroke !== 'none' && stroke !== null;
    });
}

function resetStrokeDash(nodes: SVGGeometryElement[]) {
    nodes.forEach((p) => {
        gsap.set(p, { clearProps: 'strokeDasharray,strokeDashoffset' });
    });
}

export function MotionIcon({
    icon: Icon,
    motion: preset = 'none',
    playEnter = true,
    enterKey,
    hoverAccent = false,
    className,
    size = 18,
    strokeWidth = 1.75,
    ...rest
}: MotionIconProps) {
    const wrapRef = useRef<HTMLSpanElement>(null);
    const lastEnterKeyRef = useRef<string | null>(null);
    const [enterSettled, setEnterSettled] = useState(preset === 'none');
    const m = useMotion();
    const active = preset !== 'none' && m.enabled;

    useEffect(() => {
        if (preset === 'none') {
            setEnterSettled(true);
            lastEnterKeyRef.current = null;
            return;
        }
        // 无进场动画时直接允许循环动效（如 Loader2 spin），否则 enterSettled 会一直为 false
        if (!playEnter) {
            setEnterSettled(true);
        }
    }, [preset, playEnter]);

    // 选中瞬间：轻弹入 + 描边绘制
    useEffect(() => {
        const wrap = wrapRef.current;
        if (!wrap || !m.enabled || preset === 'none' || !playEnter || !enterKey) {
            if (wrap && preset === 'none') {
                const svg = wrap.querySelector('svg');
                if (svg) resetStrokeDash(collectStrokedNodes(svg));
                gsap.set(wrap, { scale: 1, rotation: 0, opacity: 1, y: 0 });
            }
            return;
        }

        if (lastEnterKeyRef.current === enterKey) return;
        lastEnterKeyRef.current = enterKey;
        setEnterSettled(false);

        const svg = wrap.querySelector('svg');
        const paths = svg ? collectStrokedNodes(svg) : [];
        const speed = Math.max(0.5, m.speed);
        const popEase = m.preset.timing.ease.pop;
        const enterTl = gsap.timeline({
            onComplete: () => setEnterSettled(true),
        });

        enterTl.fromTo(
            wrap,
            { scale: 0.82, y: 3, opacity: 0.5 },
            {
                scale: 1,
                y: 0,
                opacity: 1,
                duration: m.duration('base'),
                ease: popEase,
            },
        );

        if (paths.length > 0) {
            const drawDur = (m.duration('fast') * 1.1) / speed;
            paths.forEach((p, i) => {
                const len =
                    typeof p.getTotalLength === 'function'
                        ? Math.max(p.getTotalLength(), 6)
                        : 24;
                gsap.set(p, { strokeDasharray: len, strokeDashoffset: len });
                enterTl.to(
                    p,
                    { strokeDashoffset: 0, duration: drawDur, ease: 'power2.out' },
                    0.04 + i * 0.022,
                );
            });
        } else {
            enterTl.call(() => setEnterSettled(true), [], '+=0.02');
        }

        return () => {
            enterTl.kill();
        };
    }, [preset, playEnter, enterKey, m.enabled, m.speed, m.preset.timing.ease.pop]);

    // 选中态持续动效（进场结束后再开，避免和弹入打架）
    useEffect(() => {
        const el = wrapRef.current;
        const waitEnter = playEnter && enterKey != null && enterKey !== '';
        if (!el || !active || (waitEnter && !enterSettled)) {
            if (el && !active) {
                gsap.killTweensOf(el);
                gsap.set(el, { rotation: 0, scale: 1, y: 0, opacity: 1 });
            }
            return;
        }

        const speed = Math.max(0.5, m.speed);
        const f = m.preset.feel;
        const easeHover = m.preset.timing.ease.hover;
        let tl: gsap.core.Timeline | gsap.core.Tween | null = null;

        switch (preset) {
            case 'pulse':
                tl = gsap.timeline({ repeat: -1, yoyo: true }).to(el, {
                    scale: f.overshoot ? 1.1 : 1.05,
                    duration: (f.breathDuration * 0.55) / speed,
                    ease: 'sine.inOut',
                });
                break;
            case 'breathe':
                tl = gsap.timeline({ repeat: -1, yoyo: true }).to(el, {
                    opacity: f.overshoot ? 0.72 : 0.82,
                    duration: (f.breathDuration * 1.05) / speed,
                    ease: 'sine.inOut',
                });
                break;
            case 'wiggle':
                if (m.level === 'elegant') {
                    tl = gsap.timeline({ repeat: -1, yoyo: true }).to(el, {
                        scale: 1.04,
                        duration: f.breathDuration / speed,
                        ease: 'sine.inOut',
                    });
                } else {
                    tl = gsap.timeline({ repeat: -1, repeatDelay: 1.4 / speed }).to(el, {
                        rotation: 6,
                        duration: 0.09 / speed,
                        yoyo: true,
                        repeat: 3,
                        ease: 'power1.inOut',
                    });
                }
                break;
            case 'spin':
                tl = gsap.to(el, {
                    rotation: 360,
                    duration: 2.4 / speed,
                    ease: 'none',
                    repeat: -1,
                });
                break;
            case 'spin-slow':
                tl = gsap.to(el, {
                    rotation: 360,
                    duration: 4.5 / speed,
                    ease: 'none',
                    repeat: -1,
                });
                break;
            case 'nudge':
                tl = gsap.timeline({ repeat: -1, repeatDelay: 2.2 / speed }).to(el, {
                    y: -2,
                    duration: 0.22 / speed,
                    yoyo: true,
                    repeat: 1,
                    ease: easeHover,
                });
                break;
            case 'bob':
                tl = gsap.timeline({ repeat: -1, yoyo: true }).to(el, {
                    y: -2,
                    duration: 0.65 / speed,
                    ease: 'sine.inOut',
                });
                break;
            default:
                break;
        }

        return () => {
            tl?.kill();
            // 同一 DOM 在 spin → 静止图标间复用时，必须清零 rotation，否则会「歪着」停住
            if (el) {
                gsap.set(el, { rotation: 0 });
            }
        };
    }, [
        active,
        enterSettled,
        playEnter,
        enterKey,
        preset,
        m.enabled,
        m.speed,
        m.level,
        m.preset.feel.overshoot,
        m.preset.feel.breathDuration,
        m.preset.timing.ease.hover,
    ]);

    useEffect(() => {
        const wrap = wrapRef.current;
        if (!wrap || !hoverAccent || !m.enabled) return;
        const onEnter = () => {
            m.pop(wrap, { peak: 1 + (m.preset.feel.popPeak - 1) * 0.45 });
        };
        wrap.addEventListener('mouseenter', onEnter);
        return () => wrap.removeEventListener('mouseenter', onEnter);
    }, [hoverAccent, m.enabled, m.level, m.speed, m.pop, m.preset.feel.popPeak]);

    return (
        <span
            ref={wrapRef}
            className={cn(
                'inline-flex shrink-0 items-center justify-center transition-[opacity] duration-200',
                !active && m.enabled && preset === 'none' && 'opacity-80',
                className,
            )}
            style={{ transformOrigin: '50% 50%' }}
        >
            <Icon size={size} strokeWidth={strokeWidth} aria-hidden {...rest} />
        </span>
    );
}

export default MotionIcon;