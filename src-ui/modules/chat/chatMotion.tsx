import {
    cloneElement,
    useLayoutEffect,
    useRef,
    type HTMLAttributes,
    type ReactElement,
    type ReactNode,
    type RefObject,
} from 'react';
import { useGSAP } from '@gsap/react';
import gsap from 'gsap';
import { useMotion, type MotionEnv } from '../../hooks/preferences/useMotion';
import { GsapPresence } from '../../shared/ui/motion/GsapPresence';

gsap.registerPlugin(useGSAP);

function enterPanel(element: HTMLElement, motion: MotionEnv) {
    const distance = motion.level === 'elegant' ? 0 : motion.level === 'rich' ? 8 : 4;
    const compact = element.matches(
        '.native-chat-badge, .native-chat-latest, .native-chat-attachment',
    );
    return gsap.fromTo(
        element,
        { opacity: 0, y: distance, scale: compact ? motion.preset.feel.tapScale : 1 },
        {
            autoAlpha: 1,
            y: 0,
            scale: 1,
            duration: motion.duration('fast'),
            ease: compact ? motion.ease.pop : motion.ease.enterMicro,
        },
    );
}

function exitPanel(element: HTMLElement, motion: MotionEnv) {
    return gsap.to(element, {
        autoAlpha: 0,
        y: motion.level === 'elegant' ? 0 : -3,
        duration: motion.duration('fast') * 0.7,
        ease: motion.ease.exit,
    });
}

// 保留退出前的内容；引用取消后不能先变成空壳再退场。
export function ChatPresence({
    visible,
    children,
    onExited,
}: {
    visible: boolean;
    children: ReactElement<HTMLAttributes<HTMLElement>>;
    onExited?: () => void;
}) {
    const retained = useRef(children);
    if (visible) retained.current = children;
    const child = retained.current;
    return (
        <GsapPresence visible={visible} onEnter={enterPanel} onExit={exitPanel} onExited={onExited}>
            {cloneElement(child, {
                ...(!visible ? { inert: '' } : { inert: undefined }),
                'aria-hidden': !visible,
                // 透明而非 visibility:hidden，允许搜索框在挂载时接收键盘焦点。
                style: {
                    ...child.props.style,
                    opacity: 0,
                    pointerEvents: visible ? child.props.style?.pointerEvents : 'none',
                },
            })}
        </GsapPresence>
    );
}

export function useChatPaneMotion(
    scope: RefObject<HTMLDivElement>,
    token: string,
    selector: string,
) {
    const motion = useMotion();
    const previous = useRef(token);
    useGSAP(
        () => {
            if (previous.current === token) return;
            previous.current = token;
            const element = scope.current?.querySelector<HTMLElement>(selector);
            if (!motion.enabled || !element || element.clientWidth === 0) return;
            gsap.fromTo(
                element,
                {
                    opacity: 0.7,
                    x: motion.level === 'elegant' ? 0 : motion.level === 'rich' ? 10 : 5,
                },
                {
                    opacity: 1,
                    x: 0,
                    duration: motion.duration('fast'),
                    ease: motion.ease.damped,
                    clearProps: 'opacity,transform',
                },
            );
        },
        {
            scope,
            dependencies: [token, motion.enabled, motion.level, motion.speed],
            revertOnUpdate: true,
        },
    );
}

export function useChatControlReset(scope: RefObject<HTMLDivElement>) {
    const motion = useMotion();
    useLayoutEffect(() => {
        const reset = () => {
            const controls = scope.current?.querySelectorAll(
                '.native-chat-icon, .native-chat-send',
            );
            if (!controls?.length) return;
            gsap.killTweensOf(controls);
            gsap.set(controls, { clearProps: 'transform' });
        };
        reset();
        const observer = new MutationObserver((records) => {
            for (const { target } of records) {
                if (
                    !(target instanceof HTMLButtonElement) ||
                    !target.disabled ||
                    !target.matches('.native-chat-icon, .native-chat-send')
                )
                    continue;
                gsap.killTweensOf(target);
                gsap.set(target, { clearProps: 'transform' });
            }
        });
        if (scope.current)
            observer.observe(scope.current, {
                subtree: true,
                attributes: true,
                attributeFilter: ['disabled'],
            });
        return () => {
            observer.disconnect();
            reset();
        };
    }, [scope, motion.enabled, motion.level, motion.speed]);
}

// 外层 translateY 属于虚拟列表；只动画内层，不改变测量高度。
export function ChatMessageEntrance({
    messageKey,
    mine,
    enter,
    takeEnter,
    order = 0,
    children,
}: {
    messageKey: string;
    mine: boolean;
    enter: boolean;
    takeEnter: (key: string) => boolean;
    order?: number;
    children: ReactNode;
}) {
    const motion = useMotion();
    const body = useRef<HTMLDivElement>(null);
    const eligible = useRef<boolean | null>(null);
    const animation = useRef<gsap.core.Tween | null>(null);
    const settings = useRef({ level: motion.level, speed: motion.speed });
    useGSAP(
        () => {
            const pending = takeEnter(messageKey);
            eligible.current ??= enter || pending;
            const element = body.current;
            if (
                !eligible.current ||
                !motion.enabled ||
                !element ||
                element.getClientRects().length === 0
            )
                return;
            const distance = motion.level === 'elegant' ? 0 : motion.level === 'rich' ? 12 : 6;
            const delay = Math.min(3, Math.max(0, order)) * motion.stagger() * 0.5;
            animation.current = gsap.fromTo(
                element,
                { opacity: 0, x: mine ? distance : -distance, y: distance / 2 },
                {
                    opacity: 1,
                    x: 0,
                    y: 0,
                    duration: motion.duration('base'),
                    delay,
                    // 正文始终单调归位，不沿用丰富挡的 back/elastic，避免文字抖动。
                    ease: motion.ease.damped,
                    clearProps: 'opacity,transform',
                },
            );
            const avatar = element.querySelector('.native-chat-avatar');
            if (avatar && !avatar.closest('.invisible') && motion.level !== 'elegant') {
                gsap.fromTo(
                    avatar,
                    { scale: motion.preset.feel.tapScale },
                    {
                        scale: 1,
                        duration: motion.duration('fast'),
                        delay,
                        ease: motion.ease.pop,
                        clearProps: 'transform',
                    },
                );
            }
        },
        { scope: body },
    );
    useLayoutEffect(() => {
        const changed =
            settings.current.level !== motion.level || settings.current.speed !== motion.speed;
        settings.current = { level: motion.level, speed: motion.speed };
        if ((motion.enabled && !changed) || !body.current) return;
        animation.current?.kill();
        gsap.set(body.current, { clearProps: 'opacity,transform' });
        const avatar = body.current.querySelector('.native-chat-avatar');
        if (avatar) {
            gsap.killTweensOf(avatar);
            gsap.set(avatar, { clearProps: 'transform' });
        }
    }, [motion.enabled, motion.level, motion.speed]);
    return (
        <div ref={body} className="native-chat-message-motion">
            {children}
        </div>
    );
}

export function useChatTabMotion(scope: RefObject<HTMLDivElement>, token: string) {
    const motion = useMotion();
    const previous = useRef<{ token: string; x: number; width: number } | null>(null);
    useGSAP(
        () => {
            const list = scope.current?.querySelector<HTMLElement>('[role=tablist]');
            const button = list?.querySelector<HTMLElement>('[aria-selected=true]');
            const indicator = list?.querySelector<HTMLElement>('.native-chat-tab-indicator');
            if (!list || !button || !indicator) return;
            const align = (animate: boolean) => {
                if (!button.clientWidth) return;
                const next = { token, x: button.offsetLeft, width: button.offsetWidth };
                const last = previous.current;
                previous.current = next;
                gsap.killTweensOf(indicator);
                gsap.set(indicator, { autoAlpha: 1 });
                if (animate && motion.enabled && last && last.token !== token) {
                    if (motion.level === 'elegant') {
                        gsap.set(indicator, { x: next.x, scaleX: next.width });
                        gsap.fromTo(
                            indicator,
                            { opacity: 0.4 },
                            {
                                opacity: 1,
                                duration: motion.duration('fast'),
                                ease: motion.ease.damped,
                            },
                        );
                    } else {
                        gsap.fromTo(
                            indicator,
                            { x: last.x, scaleX: last.width },
                            {
                                x: next.x,
                                scaleX: next.width,
                                duration: motion.duration('base'),
                                ease: motion.ease.damped,
                            },
                        );
                    }
                } else gsap.set(indicator, { x: next.x, scaleX: next.width });
            };
            align(true);
            let width = list.clientWidth;
            const observer = new ResizeObserver(() => {
                if (list.clientWidth === width) return;
                width = list.clientWidth;
                align(false);
            });
            observer.observe(list);
            return () => observer.disconnect();
        },
        {
            scope,
            dependencies: [token, motion.enabled, motion.level, motion.speed],
            revertOnUpdate: true,
        },
    );
}

export function useChatSelectionMotion(
    scope: RefObject<HTMLDivElement>,
    index: number,
    active: string | null,
) {
    const motion = useMotion();
    const previous = useRef({ index, active });
    useGSAP(
        () => {
            const last = previous.current;
            previous.current = { index, active };
            const indicator = scope.current?.querySelector('.native-chat-selection-indicator');
            if (!indicator) return;
            // 行高 68，指示条高 22，居中偏移 23。
            const y = Math.max(0, index) * 68 + 23;
            if (index < 0 || !motion.enabled || (last.index === index && last.active === active)) {
                gsap.set(indicator, { autoAlpha: index < 0 ? 0 : 1, y, scaleY: 1 });
                return;
            }
            const near = last.index >= 0 && Math.abs(last.index - index) <= 3;
            gsap.fromTo(
                indicator,
                {
                    autoAlpha: 0.4,
                    y: motion.level !== 'elegant' && near ? last.index * 68 + 23 : y,
                    scaleY: motion.preset.feel.tapScale,
                },
                {
                    autoAlpha: 1,
                    y,
                    scaleY: 1,
                    duration: motion.duration('fast'),
                    ease: motion.ease.damped,
                },
            );
            const avatar = scope.current?.querySelector('[data-active=true] .native-chat-avatar');
            if (avatar && last.active !== active && motion.level !== 'elegant') {
                gsap.fromTo(
                    avatar,
                    { scale: motion.preset.feel.tapScale },
                    {
                        scale: 1,
                        duration: motion.duration('fast'),
                        ease: motion.ease.pop,
                        clearProps: 'transform',
                    },
                );
            }
        },
        {
            scope,
            dependencies: [index, active, motion.enabled, motion.level, motion.speed],
            revertOnUpdate: true,
        },
    );
}

export function ChatCount({ value, cap = false }: { value: number; cap?: boolean }) {
    const motion = useMotion();
    const digit = useRef<HTMLSpanElement>(null);
    const previous = useRef(value);
    useGSAP(
        () => {
            const changed = previous.current !== value;
            previous.current = value;
            if (!changed || !motion.enabled || !digit.current || value < 1) return;
            gsap.fromTo(
                digit.current,
                {
                    opacity: 0.4,
                    y: motion.level === 'elegant' ? 0 : motion.level === 'rich' ? 5 : 3,
                    scale: motion.preset.feel.popPeak,
                },
                {
                    opacity: 1,
                    y: 0,
                    scale: 1,
                    duration: motion.duration('fast'),
                    ease: motion.ease.pop,
                    clearProps: 'opacity,transform',
                },
            );
        },
        {
            scope: digit,
            dependencies: [value, motion.enabled, motion.level, motion.speed],
            revertOnUpdate: true,
        },
    );
    return (
        <span ref={digit} className="native-chat-count">
            {cap && value > 99 ? '99+' : value}
        </span>
    );
}

export function ChatUnreadBadge({ count }: { count: number }) {
    return (
        <ChatPresence visible={count > 0}>
            <span className="native-chat-badge" aria-label={`${count} 条未读`}>
                <ChatCount value={count} cap />
            </span>
        </ChatPresence>
    );
}

export function useChatComposerMotion(
    scope: RefObject<HTMLDivElement>,
    ready: boolean,
    error: string,
) {
    const motion = useMotion();
    const previous = useRef({ ready, error });
    useGSAP(
        () => {
            const last = previous.current;
            previous.current = { ready, error };
            if (!motion.enabled) return;
            const icon = scope.current?.querySelector('.native-chat-send svg');
            if (ready && !last.ready && icon) {
                gsap.fromTo(
                    icon,
                    {
                        opacity: 0.4,
                        y: motion.level === 'elegant' ? 0 : 4,
                        scale: motion.preset.feel.tapScale,
                    },
                    {
                        opacity: 1,
                        y: 0,
                        scale: 1,
                        duration: motion.duration('fast'),
                        ease: motion.ease.pop,
                        clearProps: 'opacity,transform',
                    },
                );
            }
            const send = scope.current?.querySelector<HTMLElement>('.native-chat-send');
            if (error && error !== last.error && send) motion.shake(send);
        },
        {
            scope,
            dependencies: [ready, error, motion.enabled, motion.level, motion.speed],
            revertOnUpdate: true,
        },
    );
}
