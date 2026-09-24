// 动态图标：Lucide SVG + 描边绘制 / 弹入 / 循环动效（零额外依赖）。
//
// 进场（描边 + 弹入）是一次性的，交给 GSAP；常驻循环走 CSS keyframes（见 index.css
// 的 .ndf-icon-loop）。循环图标同屏十几个，用 JS 就是每帧在主线程改十几处内联 style，
// 换成 CSS 之后这些 transform / opacity 直接跑在合成线程上。

import { useEffect, useRef, useState, type ComponentType, type CSSProperties } from 'react';
import gsap from 'gsap';
import type { LucideProps } from 'lucide-react';
import { useMotion, type MotionEnv } from '../../../hooks/preferences/useMotion';
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

/**
 * 每种循环对应的 CSS 类和一轮时长（秒）。往复型（pulse / breathe / swell / bob）用
 * animation-direction: alternate，时长是单程；nudge / wiggle 的停顿写在关键帧里，时长是整轮。
 * 数值沿用原来的 GSAP 参数，观感不变。
 */
function loopStyle(preset: MotionIconPreset, m: MotionEnv): { cls: string; style: CSSProperties } | null {
    const speed = Math.max(0.5, m.speed);
    const f = m.preset.feel;
    const sec = (v: number) => `${(v / speed).toFixed(3)}s`;
    switch (preset) {
        case 'pulse':
            return {
                cls: 'ndf-icon-loop--pulse',
                style: {
                    '--ndf-icon-dur': sec(f.breathDuration * 0.55),
                    '--ndf-icon-peak': f.overshoot ? 1.1 : 1.05,
                } as CSSProperties,
            };
        case 'breathe':
            return {
                cls: 'ndf-icon-loop--breathe',
                style: {
                    '--ndf-icon-dur': sec(f.breathDuration * 1.05),
                    '--ndf-icon-dim': f.overshoot ? 0.72 : 0.82,
                } as CSSProperties,
            };
        // elegant 档不做旋转，退化成轻微缩放
        case 'wiggle':
            return m.level === 'elegant'
                ? { cls: 'ndf-icon-loop--swell', style: { '--ndf-icon-dur': sec(f.breathDuration) } as CSSProperties }
                : { cls: 'ndf-icon-loop--wiggle', style: { '--ndf-icon-dur': sec(1.76) } as CSSProperties };
        case 'spin':
            return { cls: 'ndf-icon-loop--spin', style: { '--ndf-icon-dur': sec(2.4) } as CSSProperties };
        case 'spin-slow':
            return { cls: 'ndf-icon-loop--spin', style: { '--ndf-icon-dur': sec(4.5) } as CSSProperties };
        case 'nudge':
            return { cls: 'ndf-icon-loop--nudge', style: { '--ndf-icon-dur': sec(2.64) } as CSSProperties };
        case 'bob':
            return { cls: 'ndf-icon-loop--bob', style: { '--ndf-icon-dur': sec(0.65) } as CSSProperties };
        default:
            return null;
    }
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

    // 循环动效等进场跑完再挂，避免和弹入抢同一个 transform
    const waitEnter = playEnter && enterKey != null && enterKey !== '';
    const loop = active && (!waitEnter || enterSettled) ? loopStyle(preset, m) : null;

    // 进场结束后 GSAP 会在内联 style 上留下 transform，CSS 动画虽然优先级更高，
    // 但停掉循环时那份残留会露出来，所以挂循环前先清掉。
    useEffect(() => {
        const el = wrapRef.current;
        if (el && loop) gsap.set(el, { clearProps: 'transform,opacity' });
    }, [loop?.cls]);

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
                'inline-flex shrink-0 items-center justify-center',
                // breathe 循环本身在改 opacity，再挂过渡等于每轮都重建一次过渡
                !loop && 'transition-[opacity] duration-200',
                !active && m.enabled && preset === 'none' && 'opacity-80',
                loop && `ndf-icon-loop ${loop.cls}`,
                className,
            )}
            style={{ transformOrigin: '50% 50%', ...loop?.style }}
        >
            <Icon size={size} strokeWidth={strokeWidth} aria-hidden {...rest} />
        </span>
    );
}

export default MotionIcon;