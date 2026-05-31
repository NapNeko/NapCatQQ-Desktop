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
    // mounted 控 DOM 是否渲染。visible=false 但 exit 进行中时仍 mounted=true。
    const [mounted, setMounted] = useState<boolean>(visible);

    // visible→true:立即 mount。后面 useGSAP 会跑 enter。
    // visible→false:不立即 unmount,留给 useGSAP 跑 exit + 完成时 setMounted(false)。
    useEffect(() => {
        if (visible && !mounted) {
            setMounted(true);
        }
    }, [visible, mounted]);

    // 关键:useGSAP 监听 visible 变化。每次 visible 反向变化时跑相应 timeline。
    // mounted 的依赖让 enter 在 DOM 真正挂上后才跑。
    useGSAP(
        () => {
            const el = ref.current;
            if (!el) return;
            if (visible) {
                // ENTER
                if (!env.enabled || !onEnter) {
                    gsap.set(el, { autoAlpha: 1 });
                    return;
                }
                onEnter(el, env);
            } else {
                // EXIT
                if (!mounted) return;
                if (!env.enabled || !onExit) {
                    setMounted(false);
                    onExited?.();
                    return;
                }
                const anim = onExit(el, env);
                anim.eventCallback('onComplete', () => {
                    setMounted(false);
                    onExited?.();
                });
            }
        },
        // env.enabled 切换也要重跑(用户切档位/速度/总开关时)。
        { dependencies: [visible, mounted, env.enabled] },
    );

    if (!mounted) return null;

    if (!isValidElement(children)) return children;
    return cloneElement(children as ReactElement<{ ref?: Ref<HTMLElement> }>, {
        ref,
    });
}
