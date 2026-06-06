// 开屏结束后一次性彩带/纸屑。纯 Canvas，不引第三方库。

import { useEffect, useRef } from 'react';
import { useMotion } from '../../../hooks/preferences/useMotion';

const BRAND_COLORS_FALLBACK = ['#ff6b3d', '#f58fb6', '#ffb586', '#4fb477', '#f2b43a', '#ffe9cf'];

function readThemeColors(): string[] {
    if (typeof document === 'undefined') return BRAND_COLORS_FALLBACK;
    const root = document.documentElement;
    const s = getComputedStyle(root);
    const pick = (v: string) => s.getPropertyValue(v).trim();
    const colors = [
        pick('--brand-500'),
        pick('--accent-500'),
        pick('--brand-300'),
        pick('--accent-300'),
        pick('--green-500'),
        pick('--amber-500'),
    ].filter((c) => c.length > 0);
    return colors.length > 0 ? colors : BRAND_COLORS_FALLBACK;
}

interface Particle {
    x: number;
    y: number;
    vx: number;
    vy: number;
    w: number;
    h: number;
    rot: number;
    vr: number;
    color: string;
    life: number;
    maxLife: number;
}

function spawnFromCorner(
    particles: Particle[],
    corner: 'bottom-left' | 'bottom-right',
    canvasW: number,
    canvasH: number,
    count: number,
    power: number,
    colors: string[],
) {
    const originX = corner === 'bottom-left' ? 32 : canvasW - 32;
    const originY = canvasH + 16;
    // 更竖直、更猛：角度贴近正上方，扇形略收
    const centerAngle =
        corner === 'bottom-left' ? -Math.PI / 2 + 0.22 : -Math.PI / 2 - 0.22;
    const spread = 0.72;

    for (let i = 0; i < count; i++) {
        const angle = centerAngle + (Math.random() - 0.5) * spread;
        const speed = power * (0.72 + Math.random() * 0.55);
        const life = 3.2 + Math.random() * 1.8;
        const ribbon = Math.random() < 0.38;
        particles.push({
            x: originX + (Math.random() - 0.5) * 20,
            y: originY,
            vx: Math.cos(angle) * speed,
            vy: Math.sin(angle) * speed,
            w: ribbon ? 11 + Math.random() * 9 : 5 + Math.random() * 5,
            h: ribbon ? 3 + Math.random() * 2 : 3 + Math.random() * 4,
            rot: Math.random() * Math.PI,
            vr: (Math.random() - 0.5) * 0.42,
            color: colors[Math.floor(Math.random() * colors.length)] ?? colors[0],
            life,
            maxLife: life,
        });
    }
}

export interface SplashConfettiProps {
    onDone: () => void;
}

export function SplashConfetti({ onDone }: SplashConfettiProps) {
    const canvasRef = useRef<HTMLCanvasElement>(null);
    const { enabled, level } = useMotion();
    const doneRef = useRef(false);

    useEffect(() => {
        if (!enabled || level === 'elegant') {
            onDone();
            return;
        }

        const canvas = canvasRef.current;
        if (!canvas) {
            onDone();
            return;
        }

        const ctx = canvas.getContext('2d');
        if (!ctx) {
            onDone();
            return;
        }

        const colors = readThemeColors();
        const particles: Particle[] = [];
        const dpr = Math.min(window.devicePixelRatio || 1, 2);

        const resize = () => {
            const w = window.innerWidth;
            const h = window.innerHeight;
            canvas.width = Math.floor(w * dpr);
            canvas.height = Math.floor(h * dpr);
            canvas.style.width = `${w}px`;
            canvas.style.height = `${h}px`;
            ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        };
        resize();
        window.addEventListener('resize', resize);

        const w = window.innerWidth;
        const h = window.innerHeight;

        if (level === 'rich') {
            spawnFromCorner(particles, 'bottom-left', w, h, 90, 26, colors);
            spawnFromCorner(particles, 'bottom-right', w, h, 90, 26, colors);
            window.setTimeout(() => {
                spawnFromCorner(particles, 'bottom-left', w, h, 40, 22, colors);
                spawnFromCorner(particles, 'bottom-right', w, h, 40, 22, colors);
            }, 200);
        } else {
            spawnFromCorner(particles, 'bottom-left', w, h, 62, 22, colors);
            spawnFromCorner(particles, 'bottom-right', w, h, 62, 22, colors);
        }

        const gravity = 0.15;
        const drag = 0.994;
        const start = performance.now();
        const maxMs = level === 'rich' ? 4200 : 3200;
        let raf = 0;

        const tick = (now: number) => {
            const elapsed = now - start;
            ctx.clearRect(0, 0, w, h);

            for (let i = particles.length - 1; i >= 0; i--) {
                const p = particles[i];
                p.vy += gravity;
                p.vx *= drag;
                p.vy *= drag;
                p.x += p.vx;
                p.y += p.vy;
                p.rot += p.vr;
                p.life -= 1 / 60;

                if (p.life <= 0 || p.y > h + 40) {
                    particles.splice(i, 1);
                    continue;
                }

                const alpha = Math.min(1, (p.life / p.maxLife) * 1.15);
                ctx.save();
                ctx.translate(p.x, p.y);
                ctx.rotate(p.rot);
                ctx.globalAlpha = alpha;
                ctx.fillStyle = p.color;
                ctx.fillRect(-p.w / 2, -p.h / 2, p.w, p.h);
                ctx.restore();
            }

            if (elapsed < maxMs && particles.length > 0) {
                raf = requestAnimationFrame(tick);
            } else {
                finish();
            }
        };

        const finish = () => {
            if (doneRef.current) return;
            doneRef.current = true;
            cancelAnimationFrame(raf);
            window.removeEventListener('resize', resize);
            onDone();
        };

        raf = requestAnimationFrame(tick);
        const safety = window.setTimeout(finish, maxMs + 400);

        return () => {
            window.clearTimeout(safety);
            cancelAnimationFrame(raf);
            window.removeEventListener('resize', resize);
        };
    }, [enabled, level, onDone]);

    if (!enabled || level === 'elegant') return null;

    return (
        <canvas
            ref={canvasRef}
            className="pointer-events-none fixed inset-0 z-[190]"
            aria-hidden
        />
    );
}

export default SplashConfetti;