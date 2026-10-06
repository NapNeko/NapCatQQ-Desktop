// 页面切换（根路由、Bot 列表 ↔ 配置、应用端列表 ↔ 详情）。
//
// 约定和原来一样：visible 变 false 播退场，播完调 onExited，父级换内容再把 visible 放回 true，
// 这时播进场。
//
// 动画走 WAAPI（el.animate）而不是 GSAP：transform / opacity 的 WAAPI 动画由合成线程跑，
// 新页面挂载完、数据回来触发重渲时主线程被占满，动画照样顺；GSAP 每帧都要主线程来推，
// 恰好在最忙的那几百毫秒里卡住。位移、缩放、时长、缓动都沿用原来的数值。

import { useLayoutEffect, useRef, type ReactNode } from 'react';
import { useMotion, type MotionEnv } from '../../../hooks/preferences/useMotion';
import { cssEase } from '../../../core/design/cssEase';

interface PageTransitionProps {
    visible: boolean;
    children: ReactNode;
    className?: string;
    onExited?: () => void;
    direction?: -1 | 0 | 1;
}

const SHOWN: Keyframe = { opacity: 1, transform: 'none' };

function enterFrom(dir: number, m: MotionEnv): Keyframe {
    const rich = m.level === 'rich';
    const x = dir === 0 ? 0 : dir > 0 ? (rich ? 28 : 18) : rich ? -28 : -18;
    const y = rich ? 14 : 10;
    return { opacity: 0, transform: `translate(${x}px, ${y}px) scale(${rich ? 0.988 : 1})` };
}

function exitTo(dir: number, m: MotionEnv): Keyframe {
    const rich = m.level === 'rich';
    const x = dir === 0 ? 0 : dir > 0 ? (rich ? -16 : -10) : rich ? 16 : 10;
    const y = rich ? -10 : -6;
    return { opacity: 0, transform: `translate(${x}px, ${y}px) scale(${rich ? 0.992 : 1})` };
}

export function PageTransition({
    visible,
    children,
    className,
    onExited,
    direction = 0,
}: PageTransitionProps) {
    const m = useMotion();
    const ref = useRef<HTMLDivElement>(null);
    const animRef = useRef<Animation | null>(null);
    // 下面的 effect 只跟 visible 走；方向、档位、回调读最新值即可，变了不该重播
    const latest = useRef({ m, direction, onExited });
    latest.current = { m, direction, onExited };

    useLayoutEffect(() => {
        const el = ref.current;
        if (!el) return;
        const { m: env, direction: dir, onExited: done } = latest.current;

        // 上一段没播完就换方向（退场途中又点回原页），直接撤掉它，从当前内容重新进场
        const prev = animRef.current;
        animRef.current = null;
        if (prev) {
            prev.onfinish = null;
            prev.cancel();
        }

        const canAnimate = env.enabled && typeof el.animate === 'function';

        if (visible) {
            el.style.visibility = '';
            if (!canAnimate) return;
            // layout effect 在首帧绘制前执行，起点直接生效，新内容不会先亮一下
            animRef.current = el.animate([enterFrom(dir, env), SHOWN], {
                duration: env.duration('slow') * 1000,
                easing: cssEase(env.ease.enter),
                fill: 'backwards',
            });
            return;
        }

        if (!canAnimate) {
            done?.();
            return;
        }
        // fill: forwards 让页面停在淡出后的样子，等父级换好内容、这里再起进场时一起撤掉
        const anim = el.animate([SHOWN, exitTo(dir, env)], {
            duration: env.duration('fast') * 1000,
            easing: cssEase(env.ease.exit),
            fill: 'forwards',
        });
        animRef.current = anim;
        anim.onfinish = () => {
            if (animRef.current !== anim) return;
            // 淡完再藏起来，和原来 GSAP 的 autoAlpha 一样不再挡点击。visibility 不放进关键帧：
            // 一段动画里只要有一个属性上不了合成线程，整段都会退回主线程跑
            el.style.visibility = 'hidden';
            latest.current.onExited?.();
        };
    }, [visible]);

    useLayoutEffect(
        () => () => {
            animRef.current?.cancel();
            animRef.current = null;
        },
        [],
    );

    return (
        <div ref={ref} className={className}>
            {children}
        </div>
    );
}

export default PageTransition;
