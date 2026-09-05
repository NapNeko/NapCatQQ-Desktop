// 主题切换前的蓄力：点击点周围火花螺旋汇聚、细环收拢、核心亮起并闪一下，然后交棒给 View Transition 光波。
// 纯 canvas、按时间归一化推进；结束时自行摘掉画布，不留任何 DOM。

const FALLBACK = {
    brand50: '#fff5ec',
    brand300: '#ffb586',
    brand400: '#ff9a6b',
    accent400: '#f9a3c5',
};

function readHexToken(name: string, fallback: string): string {
    if (typeof document === 'undefined') return fallback;
    const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
    return /^#[0-9a-f]{6}$/i.test(v) ? v : fallback;
}

function rgba(hex: string, alpha: number): string {
    const n = parseInt(hex.slice(1), 16);
    return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha.toFixed(3)})`;
}

function clamp01(v: number): number {
    return v < 0 ? 0 : v > 1 ? 1 : v;
}

export function runThemeCharge(
    cx: number,
    cy: number,
    durationMs: number,
    level: 'standard' | 'rich',
): Promise<void> {
    return new Promise((resolve) => {
        if (typeof document === 'undefined' || durationMs <= 0 || document.hidden) {
            resolve();
            return;
        }
        const canvas = document.createElement('canvas');
        const ctx = canvas.getContext('2d');
        if (!ctx) {
            resolve();
            return;
        }

        const dpr = Math.min(window.devicePixelRatio || 1, 2);
        const w = window.innerWidth;
        const h = window.innerHeight;
        canvas.width = Math.floor(w * dpr);
        canvas.height = Math.floor(h * dpr);
        canvas.className = 'ndf-theme-charge-canvas';
        canvas.setAttribute('aria-hidden', 'true');
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        document.body.appendChild(canvas);

        // 用的是旧主题的品牌色：火花是旧主题的余温，光波才换成新主题的颜色
        const brand50 = readHexToken('--brand-50', FALLBACK.brand50);
        const brand300 = readHexToken('--brand-300', FALLBACK.brand300);
        const brand400 = readHexToken('--brand-400', FALLBACK.brand400);
        const accent400 = readHexToken('--accent-400', FALLBACK.accent400);
        const palette = [brand300, brand400, accent400, brand50];

        const rich = level === 'rich';
        const count = rich ? 40 : 24;
        const sparks = Array.from({ length: count }, (_, i) => ({
            angle: (i / count) * Math.PI * 2 + (Math.random() - 0.5) * 0.5,
            r0: (rich ? 120 : 96) + Math.random() * (rich ? 110 : 80),
            size: 1.2 + Math.random() * 1.8,
            color: palette[i % palette.length],
            delay: Math.random() * 0.28,
            spin: (Math.random() - 0.5) * (rich ? 2.2 : 1.4),
        }));

        const ringR0 = rich ? 180 : 140;
        const start = performance.now();
        let raf = 0;
        let done = false;

        const finish = () => {
            if (done) return;
            done = true;
            cancelAnimationFrame(raf);
            canvas.remove();
            resolve();
        };

        const tick = (now: number) => {
            const p = clamp01((now - start) / durationMs);
            ctx.clearRect(0, 0, w, h);

            // 收拢环：越收越亮，最后 15% 随闪光一起消失
            const ringP = clamp01(p / 0.85);
            const ringE = ringP * ringP;
            const ringR = ringR0 * (1 - ringE) + 10;
            const ringAlpha = (0.15 + 0.8 * ringP) * (1 - clamp01((p - 0.85) / 0.15));
            if (ringAlpha > 0.01) {
                ctx.beginPath();
                ctx.arc(cx, cy, ringR, 0, Math.PI * 2);
                ctx.strokeStyle = rgba(brand400, ringAlpha);
                ctx.lineWidth = 1 + 1.5 * ringP;
                ctx.stroke();
            }

            // 火花：三次方 ease-in 向心，带一点螺旋；拖一条短尾巴
            ctx.lineCap = 'round';
            for (const s of sparks) {
                const lp = clamp01((p - s.delay) / (0.88 - s.delay));
                if (lp <= 0) continue;
                const e = lp * lp * lp;
                const eTail = Math.pow(clamp01(lp - 0.1), 3);
                const r = s.r0 * (1 - e) + 4;
                const rTail = s.r0 * (1 - eTail) + 4;
                const a = s.angle + s.spin * e;
                const aTail = s.angle + s.spin * eTail;
                const fade = 1 - clamp01((lp - 0.9) / 0.1);
                ctx.globalAlpha = Math.min(1, lp * 1.8) * fade;
                ctx.strokeStyle = s.color;
                ctx.lineWidth = s.size;
                ctx.beginPath();
                ctx.moveTo(cx + Math.cos(aTail) * rTail, cy + Math.sin(aTail) * rTail);
                ctx.lineTo(cx + Math.cos(a) * r, cy + Math.sin(a) * r);
                ctx.stroke();
            }
            ctx.globalAlpha = 1;

            // 核心：先攒亮，最后 15% 炸成一团大光晕，交给 View Transition 那边的起点光团接着淡
            const flashP = clamp01((p - 0.85) / 0.15);
            const coreR = p < 0.85 ? 6 + 30 * (p / 0.85) : 36 + (rich ? 220 : 160) * flashP;
            const coreAlpha = p < 0.85 ? 0.25 + 0.7 * (p / 0.85) : 0.95 - 0.45 * flashP;
            const g = ctx.createRadialGradient(cx, cy, 0, cx, cy, coreR);
            g.addColorStop(0, rgba(brand50, coreAlpha));
            g.addColorStop(0.35, rgba(brand300, coreAlpha * 0.85));
            g.addColorStop(0.7, rgba(brand400, coreAlpha * 0.35));
            g.addColorStop(1, rgba(brand400, 0));
            ctx.fillStyle = g;
            ctx.beginPath();
            ctx.arc(cx, cy, coreR, 0, Math.PI * 2);
            ctx.fill();

            if (p < 1) {
                raf = requestAnimationFrame(tick);
            } else {
                finish();
            }
        };

        raf = requestAnimationFrame(tick);
        window.setTimeout(finish, durationMs + 250);
    });
}
