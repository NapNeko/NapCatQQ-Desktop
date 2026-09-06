// Hello 卡上的吉祥物：弹簧入场 → 站着呼吸；鼠标在卡上移动时倾身、眼睛跟着看；
// 戳一下随机来一段身体动作（蹦 / 歪头 / 点头 / 举猫）加眯眼、一句话和一个表情；
// 久不理她会塌下来打瞌睡。所有动作都过 useMotion，档位关闭 / reduced 时静止。

import React, { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import gsap from 'gsap';
import { GsapPresence } from '../../../shared/ui/motion';
import { useMotion } from '../../../hooks/preferences/useMotion';
import { RiggedMascot } from './RiggedMascot';
import { useMascotFace } from './useMascotFace';
import { useMascotBody } from './useMascotBody';
import { MascotEmote, type Emote, type EmoteKind } from './MascotEmote';

export interface MascotReaction {
    /** 每次新反应换一个 key，相同 key 不重播。 */
    key: number;
    text: string;
    /** true = 出事了：抖一下而不是摇摆。 */
    alarm?: boolean;
}

interface HeroMascotProps {
    /** 鼠标倾身的感应区，通常是整张 Hello 卡。 */
    stageRef: React.RefObject<HTMLElement>;
    /** 台词，按顺序循环；第一条建议放当前状态。 */
    quips: readonly string[];
    /** 父级推过来的主动发言。 */
    reaction?: MascotReaction | null;
    className?: string;
}

const BUBBLE_MS = 3200;
const DIZZY_AT = 6;
const DIZZY_WINDOW_MS = 1500;
const DOZE_AFTER_MS = 45_000;

export const HeroMascot: React.FC<HeroMascotProps> = ({ stageRef, quips, reaction, className }) => {
    const m = useMotion();
    const figureRef = useRef<HTMLDivElement>(null);
    const bodyRef = useRef<HTMLDivElement>(null);
    const quipIndexRef = useRef(0);
    const hideTimerRef = useRef<number | null>(null);
    const [bubbleOpen, setBubbleOpen] = useState(false);
    // 退场淡出期间还要显示上一句，所以文字和开关分开存。
    const [bubbleText, setBubbleText] = useState('');
    const [emote, setEmote] = useState<Emote | null>(null);
    const face = useMascotFace(bodyRef, m);
    const body = useMascotBody(bodyRef, m);

    const lively = m.enabled && m.level !== 'elegant';

    const showEmote = useCallback((kind: EmoteKind) => {
        setEmote({ kind, key: Date.now() });
    }, []);

    // 入场。layout effect 抢在首帧前把起点设成透明，避免闪一下再飞入。
    useLayoutEffect(() => {
        const el = figureRef.current;
        if (!el) return;
        // 只清自己管的属性，别把倾身用的 quickTo（x / rotate）一起杀掉。
        gsap.killTweensOf(el, 'y,autoAlpha,opacity,visibility');
        if (!m.enabled) {
            gsap.set(el, { autoAlpha: 1, x: 0, y: 0, rotate: 0, scale: 1 });
            return;
        }
        const tween = gsap.fromTo(
            el,
            { autoAlpha: 0, y: 44 },
            {
                autoAlpha: 1,
                y: 0,
                duration: m.duration('slow') * 1.6,
                ease: m.ease.enter,
                // 等标题的字先落定，她再冒出来，开场有先后。
                delay: 0.32,
            },
        );
        return () => {
            tween.kill();
        };
    }, [m.enabled, m.duration, m.ease]);

    // 倾身：跟着鼠标在卡上的横向位置，脚底为轴微微侧一点。
    useEffect(() => {
        const stage = stageRef.current;
        const el = figureRef.current;
        if (!stage || !el || !lively) return;
        const toX = gsap.quickTo(el, 'x', { duration: 0.6, ease: 'power3.out' });
        const toRot = gsap.quickTo(el, 'rotate', { duration: 0.6, ease: 'power3.out' });
        const onMove = (e: PointerEvent) => {
            const rect = stage.getBoundingClientRect();
            const nx = ((e.clientX - rect.left) / rect.width) * 2 - 1;
            const ny = ((e.clientY - rect.top) / rect.height) * 2 - 1;
            toX(nx * 5);
            toRot(nx * 2.5);
            face.lookAt(nx, ny);
        };
        const onLeave = () => {
            toX(0);
            toRot(0);
            face.lookAt(0, 0);
        };
        stage.addEventListener('pointermove', onMove);
        stage.addEventListener('pointerleave', onLeave);
        return () => {
            stage.removeEventListener('pointermove', onMove);
            stage.removeEventListener('pointerleave', onLeave);
        };
    }, [stageRef, lively, face]);

    // 久不动鼠标键盘就打瞌睡：闭眼、头垂下去、人塌一点、头顶冒 zz；一有动静就醒。
    useEffect(() => {
        if (!lively) return;
        let timer = 0;
        let dozing = false;
        const fallAsleep = () => {
            dozing = true;
            face.doze(true);
            body.slump(true);
            showEmote('zz');
        };
        const wake = () => {
            if (!dozing) return;
            dozing = false;
            face.doze(false);
            body.slump(false);
            showEmote('exclaim');
        };
        const arm = () => {
            window.clearTimeout(timer);
            wake();
            timer = window.setTimeout(fallAsleep, DOZE_AFTER_MS);
        };
        const events = ['pointermove', 'pointerdown', 'keydown', 'wheel'] as const;
        events.forEach((ev) => document.addEventListener(ev, arm, { passive: true }));
        arm();
        return () => {
            window.clearTimeout(timer);
            events.forEach((ev) => document.removeEventListener(ev, arm));
            if (dozing) {
                face.doze(false);
                body.slump(false);
            }
        };
    }, [lively, face, body, showEmote]);

    useEffect(
        () => () => {
            if (hideTimerRef.current !== null) window.clearTimeout(hideTimerRef.current);
        },
        [],
    );

    const say = useCallback((text: string, holdMs = BUBBLE_MS) => {
        setBubbleText(text);
        setBubbleOpen(true);
        if (hideTimerRef.current !== null) window.clearTimeout(hideTimerRef.current);
        hideTimerRef.current = window.setTimeout(() => setBubbleOpen(false), holdMs);
    }, []);

    // 外部事件（实例出事 / 上线）让她主动开口。key 变一次说一次。
    useEffect(() => {
        if (!reaction) return;
        say(reaction.text, 4200);
        if (reaction.alarm) {
            const el = bodyRef.current;
            if (el) m.shake(el);
            face.widen();
            showEmote('exclaim');
        } else {
            body.hop();
            face.squint();
            showEmote('sparkle');
        }
        // 只认 key：同一条反应不因为父级重渲就重复播。
    }, [reaction?.key]);

    // 连戳彩蛋：短时间内戳到第 DIZZY_AT 下，她转一圈晕掉。
    const pokeBurstRef = useRef<{ count: number; last: number }>({ count: 0, last: 0 });

    const poke = useCallback(() => {
        const now = Date.now();
        const burst = pokeBurstRef.current;
        burst.count = now - burst.last < DIZZY_WINDOW_MS ? burst.count + 1 : 1;
        burst.last = now;

        if (burst.count >= DIZZY_AT) {
            burst.count = 0;
            const el = bodyRef.current;
            if (el && m.enabled) {
                gsap.killTweensOf(el);
                gsap.fromTo(
                    el,
                    { rotate: 0 },
                    { rotate: 360, duration: 0.9 / Math.max(0.5, m.speed), ease: m.ease.release },
                );
            }
            face.dizzy();
            showEmote('sweat');
            say('转晕了……让我歇会儿。', 3600);
            return;
        }

        // 动作层没装上（档位关闭 / 素材缺层）时退回整体轻微弹一下，至少有个回应。
        if (body.move() === null && bodyRef.current) m.pop(bodyRef.current, { peak: 1.035 });
        face.squint();
        showEmote('heart');
        if (quips.length === 0) return;
        const text = quips[quipIndexRef.current % quips.length];
        quipIndexRef.current += 1;
        say(text);
    }, [m, quips, say, body, face, showEmote]);

    return (
        <div className={className}>
            <GsapPresence
                visible={bubbleOpen}
                onEnter={(el, env) =>
                    gsap.fromTo(
                        el,
                        { autoAlpha: 0, scale: 0.72, y: 8, transformOrigin: '100% 50%' },
                        { autoAlpha: 1, scale: 1, y: 0, duration: env.duration('base'), ease: env.ease.pop },
                    )
                }
                onExit={(el, env) =>
                    gsap.to(el, {
                        autoAlpha: 0,
                        scale: 0.9,
                        y: 4,
                        duration: env.duration('fast'),
                        ease: env.ease.exit,
                    })
                }
            >
                <div
                    role="status"
                    aria-live="polite"
                    className="ndf-mascot-bubble pointer-events-none absolute right-[calc(100%-14px)] top-9 z-30 whitespace-nowrap rounded-md border border-border-subtle bg-surface px-3 py-1.5 text-xs font-medium text-text shadow-popover"
                >
                    {bubbleText}
                </div>
            </GsapPresence>

            <div
                ref={figureRef}
                role="button"
                tabIndex={0}
                aria-label="戳一下吉祥物"
                onClick={poke}
                onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault();
                        poke();
                    }
                }}
                className="pointer-events-auto cursor-pointer rounded-xl outline-none focus-visible:ring-2 focus-visible:ring-brand"
                style={{ transformOrigin: '50% 100%', willChange: 'transform' }}
            >
                <MascotEmote emote={emote} className="absolute right-3 top-0 z-30" />
                <div ref={bodyRef} style={{ transformOrigin: '50% 100%' }}>
                    <RiggedMascot
                        primaryColor="var(--brand-500)"
                        secondaryColor="var(--brand-700)"
                        className="h-[225px] w-[150px] drop-shadow-md [&>svg]:h-full [&>svg]:w-full"
                    />
                </div>
            </div>
        </div>
    );
};

export default HeroMascot;
