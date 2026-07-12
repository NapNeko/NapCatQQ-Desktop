// GsapPresence: GSAP 版的 AnimatePresence 等价物。
//
// 痛点:framer 的 <AnimatePresence> 让组件在 unmount 前先跑退场动画再真删,
// GSAP 没有内置等价物。Dialog/InfoBar/路由切换/Tabs 内容切换都依赖这个。
//
// 设计:外部传受控 visible 信号,组件内部 mounted 控制 DOM 渲染:
//   visible=true:  立即 mount → useGSAP 在 children 节点上跑 enter
//   visible=false: 不立即 unmount → useGSAP 跑 exit → onComplete 后 setMounted(false)
//
// children 必须是单个 React element,接受一个 ref。GsapPresence 通过 cloneElement
// 把内部 ref 注入。enter/exit timeline 工厂由调用方提供;空时走默认 autoAlpha
// 0↔1。reduced/enabled=false 时直接 set 终态跳过动画。

import { useGSAP } from '@gsap/react';
import gsap from 'gsap';
import {
    cloneElement,
    isValidElement,
    useEffect,
    useLayoutEffect,
    useRef,
    useState,
    type ReactElement,
    type Ref,
} from 'react';
import { useMotion, type MotionEnv } from '../../../hooks/preferences/useMotion';

/// useGSAP 插件注册(模块级,只跑一次)。
gsap.registerPlugin(useGSAP);

export type EnterFn = (
    el: HTMLElement,
    env: MotionEnv,
) => gsap.core.Timeline | gsap.core.Tween;
export type ExitFn = EnterFn;

interface GsapPresenceProps {
    visible: boolean;
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

export function GsapPresence({
    visible,
    children,
    onEnter,
    onExit,
    onExited,
}: GsapPresenceProps) {
    const env = useMotion();
    const ref = useRef<HTMLElement | null>(null);
    const activeAnimRef = useRef<gsap.core.Tween | gsap.core.Timeline | null>(null);
    const [mounted, setMounted] = useState<boolean>(visible);
    /** ref 挂上 DOM 后再触发 useGSAP，避免首屏 PageTransition 一直 visibility:hidden */
    const [refReady, setRefReady] = useState(0);

    useLayoutEffect(() => {
        if (!mounted) return;
        setRefReady((n) => n + 1);
    }, [mounted, visible]);

    // visible→true:立即 mount。后面 useGSAP 会跑 enter。
    // visible→false:不立即 unmount,留给 useGSAP 跑 exit + 完成时 setMounted(false)。
    useEffect(() => {
        if (visible && !mounted) {
            setMounted(true);
        }
    }, [visible, mounted]);

    // 退场必须落到终态再 unmount：exit 被 kill（依赖抖动、快速连点状态）时若只
    // kill 不设 visibility，按钮会卡在半透明仍占位（日志/WebUI/VNC 不消失）。
    useGSAP(
        () => {
            const el = ref.current;
            if (!el) return;

            const finishExit = () => {
                gsap.set(el, {
                    autoAlpha: 0,
                    opacity: 0,
                    visibility: 'hidden',
                    clearProps: 'willChange',
                });
                setMounted(false);
                onExited?.();
            };

            activeAnimRef.current?.kill();
            activeAnimRef.current = null;

            if (visible) {
                if (!env.enabled || !onEnter) {
                    gsap.set(el, { autoAlpha: 1, opacity: 1, visibility: 'visible' });
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
                const anim = activeAnimRef.current;
                if (anim) {
                    anim.kill();
                    activeAnimRef.current = null;
                    if (!visible && mounted) {
                        finishExit();
                    }
                }
            };
        },
        { dependencies: [visible, mounted, env.enabled, refReady] },
    );

    if (!visible && !mounted) return null;

    if (!isValidElement(children)) return children;
    return cloneElement(children as ReactElement<{ ref?: Ref<HTMLElement> }>, {
        ref,
    });
}
