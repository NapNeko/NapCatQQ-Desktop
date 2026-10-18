// GsapPresence: GSAP 版的 AnimatePresence 等价物。
//
// 痛点:framer 的 <AnimatePresence> 让组件在 unmount 前先跑退场动画再真删,
// GSAP 没有内置等价物。Dialog/InfoBar/路由切换/Tabs 内容切换都依赖这个。
//
// 设计:外部传受控 visible 信号,组件内部 mounted 控制 DOM 渲染:
//   visible=true:  立即 mount → 在 children 节点上跑 enter
//   visible=false: 不立即 unmount → 跑 exit → onComplete 后 setMounted(false)
//
// children 必须是单个 React element,接受一个 ref。GsapPresence 通过 cloneElement
// 把内部 ref 注入。enter/exit timeline 工厂由调用方提供;空时走默认 autoAlpha
// 0↔1。reduced/enabled=false 时直接 set 终态跳过动画。

import gsap from 'gsap';
import {
    cloneElement,
    forwardRef,
    isValidElement,
    useCallback,
    useLayoutEffect,
    useRef,
    useState,
    type ReactElement,
    type Ref,
} from 'react';
import { useMotion, type MotionEnv } from '../../../hooks/preferences/useMotion';

export type EnterFn = (el: HTMLElement, env: MotionEnv) => gsap.core.Timeline | gsap.core.Tween;
export type ExitFn = EnterFn;

interface GsapPresenceProps {
    visible: boolean;
    /** 收起后隐藏 DOM，保留编辑器等子组件的本地草稿。 */
    keepMounted?: boolean;
    /// 单个 React element,会被注入 ref。通常是一个 div / forwardRef 组件。
    children: ReactElement;
    /// enter 工厂。返回 timeline / tween,GsapPresence 会监听 onComplete 翻 phase。
    /// 不传 → 默认 autoAlpha 0→1,duration=base。
    onEnter?: EnterFn;
    /// exit 工厂。不传 → 默认 autoAlpha → 0。
    onExit?: ExitFn;
    /// exit 完成后回调。一般父级用不到,Dialog 这种受 RadixDialog Portal 控的场景
    /// 可以用它通知外部"现在可以真清掉 portal 了"。
    onExited?: () => void;
}

export const GsapPresence = forwardRef<HTMLElement, GsapPresenceProps>(function GsapPresence(
    { visible, keepMounted = false, children, onEnter, onExit, onExited },
    forwardedRef,
) {
    const env = useMotion();
    const latestMotion = useRef(env);
    latestMotion.current = env;
    const callbacks = useRef({ onEnter, onExit, onExited });
    callbacks.current = { onEnter, onExit, onExited };
    const ref = useRef<HTMLElement | null>(null);
    const setRef = useCallback(
        (node: HTMLElement | null) => {
            ref.current = node;
            if (typeof forwardedRef === 'function') forwardedRef(node);
            else if (forwardedRef) forwardedRef.current = node;
        },
        [forwardedRef],
    );
    const activeAnimRef = useRef<gsap.core.Tween | gsap.core.Timeline | null>(null);
    const [mounted, setMounted] = useState<boolean>(visible);
    // 提交前同步挂载状态，避免 ref 就绪后再次重播 enter、把反转中的高度清零。
    if (visible && !mounted) setMounted(true);
    const latestVisible = useRef(visible);
    latestVisible.current = visible;
    const hasMounted = useRef(visible);
    if (visible) hasMounted.current = true;

    // 退场必须落到终态再 unmount：exit 被 kill（依赖抖动、快速连点状态）时若只
    // kill 不设 visibility，按钮会卡在半透明仍占位（日志/WebUI/VNC 不消失）。
    useLayoutEffect(() => {
        const { onEnter, onExit, onExited } = callbacks.current;
        const el = ref.current;
        if (!el) return;
        let cancelled = false;
        let exited = false;

        const finishExit = () => {
            // 快速切回或设置变化会取消旧动画；迟到的回调不能再卸载新内容。
            if (cancelled || exited || latestVisible.current) return;
            exited = true;
            gsap.set(el, {
                autoAlpha: 0,
                opacity: 0,
                visibility: 'hidden',
                clearProps: 'willChange',
            });
            setMounted(false);
            onExited?.();
        };

        const prev = activeAnimRef.current;
        if (prev) {
            prev.eventCallback('onInterrupt', null);
            prev.kill();
            activeAnimRef.current = null;
        }

        if (visible) {
            gsap.set(el, { visibility: 'visible' });
            if (!env.enabled || !onEnter) {
                gsap.set(el, {
                    autoAlpha: 1,
                    opacity: 1,
                    visibility: 'visible',
                    ...(!env.enabled ? { clearProps: 'transform' } : {}),
                });
                return;
            }
            const anim = onEnter(el, env);
            activeAnimRef.current = anim;
            anim.eventCallback('onInterrupt', () => {
                gsap.set(el, { autoAlpha: 1, opacity: 1, visibility: 'visible' });
            });
        } else {
            if (!mounted) return;
            if (!env.enabled || !onExit) {
                finishExit();
                return;
            }
            const anim = onExit(el, env);
            activeAnimRef.current = anim;
            anim.eventCallback('onComplete', finishExit);
            anim.eventCallback('onInterrupt', finishExit);
        }

        return () => {
            cancelled = true;
            const anim = activeAnimRef.current;
            if (anim) {
                anim.eventCallback('onInterrupt', null);
                // 中途关闭动效时先交付入场终态，不能留下半缩放的弹窗或半高面板。
                if (visible && latestVisible.current && !latestMotion.current.enabled) {
                    anim.progress(1);
                }
                anim.eventCallback('onComplete', null);
                anim.kill();
                activeAnimRef.current = null;
            }
        };
    }, [visible, mounted, env]);

    if (!visible && !mounted && (!keepMounted || !hasMounted.current)) return null;

    if (!isValidElement(children)) return children;
    return cloneElement(children as ReactElement<{ ref?: Ref<HTMLElement>; hidden?: boolean }>, {
        ref: setRef,
        ...(keepMounted ? { hidden: !visible && !mounted } : {}),
    });
});
