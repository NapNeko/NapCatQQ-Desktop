// ListItem: GSAP 版列表项 wrapper。第二轮重写。
//
// stagger 由父级 useGSAP 调用,本组件只管 hover lift。换用 m.bindHover 让 hover
// 自动接 boxShadow / brightness 联动。

import {
    forwardRef,
    useEffect,
    useImperativeHandle,
    useRef,
    type HTMLAttributes,
} from 'react';
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
            if (!el || !hoverable || !m.enabled || m.preset.feel.cardLift === 0) return;
            return m.bindHover(el);
        }, [hoverable, m.enabled, m.level, m.speed, m.bindHover, m.preset.feel.cardLift]);

        return <div ref={localRef} {...rest} />;
    },
);
ListItem.displayName = 'ListItem';
