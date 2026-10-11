import { useCallback, useEffect, useLayoutEffect, useRef, type RefObject } from 'react';
import { useMotion } from '../../hooks/preferences/useMotion';
import { cssEase } from '../../core/design/cssEase';
import { startConversationMotionProbe } from './conversationMotionProbe';
import { createConversationSnapshot } from './conversationSnapshot';

// 旧画面只留一份惰性 DOM 快照；合成器播放过渡，React 不必等退场才挂新会话。
export function useConversationTransition(scope: RefObject<HTMLDivElement>, token: string | null) {
    const motion = useMotion();
    const settings = useRef(motion);
    settings.current = motion;
    const current = useRef(token);
    current.current = token;
    const previous = useRef(token);
    const pending = useRef<{ token: string; commit: () => void } | null>(null);
    const incoming = useRef<Animation | null>(null);
    const outgoing = useRef<Animation | null>(null);
    const snapshot = useRef<HTMLElement | null>(null);
    const scheduled = useRef(0);
    const alive = useRef(true);
    const probe = useRef<ReturnType<typeof startConversationMotionProbe> | null>(null);

    const surface = useCallback(() => {
        const frame = scope.current?.querySelector<HTMLElement>('.native-chat-timeline-frame');
        const body = frame?.querySelector<HTMLElement>(
            '.native-chat-timeline-wrap:not([data-chat-snapshot])',
        );
        return frame && body ? { frame, body } : null;
    }, [scope]);

    const unlock = useCallback(() => {
        const elements = surface();
        if (!elements) return;
        elements.frame.inert = false;
        delete elements.frame.dataset.switching;
        elements.body.style.removeProperty('opacity');
        elements.body.style.removeProperty('visibility');
        elements.body.style.removeProperty('will-change');
    }, [surface]);

    const clear = useCallback(() => {
        cancelAnimationFrame(scheduled.current);
        scheduled.current = 0;
        incoming.current?.cancel();
        incoming.current = null;
        outgoing.current?.cancel();
        outgoing.current = null;
        snapshot.current?.remove();
        snapshot.current = null;
        probe.current?.finish();
        probe.current = null;
        unlock();
    }, [unlock]);

    const commitPending = useCallback(() => {
        scheduled.current = 0;
        const request = pending.current;
        pending.current = null;
        if (alive.current) request?.commit();
    }, []);

    const cancelPending = useCallback(() => {
        pending.current = null;
        clear();
    }, [clear]);

    const switchTo = useCallback(
        (next: string, commit: () => void) => {
            if (!alive.current) return;
            if (next === current.current) {
                if (pending.current) cancelPending();
                commit();
                return;
            }
            pending.current = { token: next, commit };
            const elements = surface();
            const env = settings.current;
            if (
                !env.enabled ||
                document.hidden ||
                !elements?.frame.getClientRects().length ||
                typeof elements.body.animate !== 'function'
            ) {
                clear();
                commitPending();
                return;
            }
            if (scheduled.current) return;
            scheduled.current = requestAnimationFrame(() => {
                scheduled.current = 0;
                if (!alive.current || !pending.current) return;
                const source = surface();
                if (!source) {
                    commitPending();
                    return;
                }
                if (!snapshot.current) {
                    probe.current = startConversationMotionProbe(source.frame, 0);
                    const frozen = createConversationSnapshot(source.body, source.frame);
                    const oldScroll =
                        source.body.querySelector<HTMLElement>('.native-chat-timeline');
                    source.frame.append(frozen);
                    const frozenScroll = frozen.querySelector<HTMLElement>('.native-chat-timeline');
                    if (oldScroll && frozenScroll) frozenScroll.scrollTop = oldScroll.scrollTop;
                    snapshot.current = frozen;
                    probe.current.captured(frozen.querySelectorAll('*').length);
                    const height = source.frame.clientHeight;
                    const animation = frozen.animate(
                        [
                            { transform: 'translate3d(0,0,0)', opacity: 1 },
                            {
                                transform: `translate3d(0,${env.level === 'elegant' ? 0 : -height}px,0)`,
                                opacity: 0,
                            },
                        ],
                        {
                            duration: env.duration('base') * 750,
                            easing: cssEase(env.ease.damped),
                            fill: 'both',
                        },
                    );
                    outgoing.current = animation;
                    void animation.finished.then(
                        () => {
                            if (outgoing.current !== animation) return;
                            frozen.remove();
                            snapshot.current = null;
                            outgoing.current = null;
                            animation.cancel();
                        },
                        () => {},
                    );
                }
                source.frame.inert = true;
                source.frame.dataset.switching = 'true';
                source.body.style.visibility = 'hidden';
                // 先提交快照合成层，下一拍的会话挂载不会截断旧画面。
                scheduled.current = requestAnimationFrame(commitPending);
            });
        },
        [clear, cancelPending, commitPending, surface],
    );

    useLayoutEffect(() => {
        if (previous.current === token) return;
        previous.current = token;
        // 关闭会话或搜索直接跳转优先于尚未提交的列表点击。
        if (pending.current) cancelPending();
        incoming.current?.cancel();
        incoming.current = null;
        const env = settings.current;
        const elements = surface();
        if (
            !token ||
            !env.enabled ||
            document.hidden ||
            !elements?.frame.getClientRects().length ||
            typeof elements.body.animate !== 'function'
        ) {
            clear();
            return;
        }
        probe.current?.committed();
        elements.frame.inert = true;
        elements.frame.dataset.switching = 'true';
        elements.body.style.visibility = 'visible';
        const animation = elements.body.animate(
            [
                {
                    transform: `translate3d(0,${env.level === 'elegant' ? 0 : elements.frame.clientHeight}px,0)`,
                    opacity: 0,
                },
                { transform: 'translate3d(0,0,0)', opacity: 1 },
            ],
            {
                duration: env.duration('base') * 850,
                easing: cssEase(env.ease.damped),
                fill: 'both',
            },
        );
        incoming.current = animation;
        void animation.finished.then(
            () => {
                if (incoming.current !== animation) return;
                incoming.current = null;
                animation.cancel();
                unlock();
                probe.current?.finish();
                probe.current = null;
            },
            () => {},
        );
    }, [token, clear, cancelPending, surface, unlock, motion.enabled, motion.level, motion.speed]);

    useEffect(() => {
        if (!motion.enabled) {
            clear();
            commitPending();
        }
    }, [motion.enabled, clear, commitPending]);

    useEffect(() => {
        alive.current = true;
        const onVisibility = () => {
            if (document.hidden) {
                clear();
                commitPending();
            }
        };
        document.addEventListener('visibilitychange', onVisibility);
        return () => {
            alive.current = false;
            pending.current = null;
            clear();
            document.removeEventListener('visibilitychange', onVisibility);
        };
    }, [clear, commitPending]);
    return Object.assign(switchTo, { cancel: cancelPending });
}
