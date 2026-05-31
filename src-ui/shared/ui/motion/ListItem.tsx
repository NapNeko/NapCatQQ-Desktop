// ListItem: GSAP 版列表项 wrapper。
//
// 跟 framer 版根本差异:GSAP 没有 Variants,stagger 由父级 useGSAP 调用
// gsap.from(items, { stagger, ... }) 实现,本组件不再承担 enter 动画自身。
//
// 留给业务的只有:hover 上抬 + 退场动画。退场用 GsapPresence 包(列表项被
// 删除时希望先淡出再 unmount)。
//
// 用法:
//   const containerRef = useRef(null);
//   useGSAP(() => {
//     gsap.from(containerRef.current.children, {
//       autoAlpha: 0, y: 6, scale: 0.985,
//       duration: m.duration('base'),
//       ease: m.preset.enterEase,
//       stagger: m.preset.stagger,
//     });
//   }, { scope: containerRef, dependencies: [items.length] });
//
//   <div ref={containerRef}>
//     {items.map(it => <ListItem key={it.id} hoverable>{...}</ListItem>)}
//   </div>

import {
    forwardRef,
    useEffect,
    useImperativeHandle,
    useRef,
    type HTMLAttributes,
} from 'react';
import gsap from 'gsap';
import { useMotion } from '../../../hooks/preferences/useMotion';

interface ListItemProps extends HTMLAttributes<HTMLDivElement> {
    /// hoverable=true 时启用 hover lift。
    hoverable?: boolean;
}

export const ListItem = forwardRef<HTMLDivElement, ListItemProps>(
    ({ hoverable, ...rest }, ref) => {
        const m = useMotion();
        const localRef = useRef<HTMLDivElement | null>(null);
        useImperativeHandle(ref, () => localRef.current!, []);

        useEffect(() => {
            const el = localRef.current;
            if (!el || !hoverable || !m.enabled) return;
            const lift = m.preset.cardLift;
            if (lift === 0) return;

            const onEnter = () => {
                gsap.to(el, {
                    y: -lift,
                    duration: m.duration('fast'),
                    ease: m.preset.hoverEase,
                });
            };
            const onLeave = () => {
                gsap.to(el, {
                    y: 0,
                    duration: m.duration('fast'),
                    ease: m.preset.hoverEase,
                });
            };
            el.addEventListener('mouseenter', onEnter);
            el.addEventListener('mouseleave', onLeave);
            return () => {
                el.removeEventListener('mouseenter', onEnter);
                el.removeEventListener('mouseleave', onLeave);
            };
        }, [hoverable, m.enabled, m.preset.cardLift, m.preset.hoverEase, m.speed]);

        return <div ref={localRef} {...rest} />;
    },
);
ListItem.displayName = 'ListItem';
