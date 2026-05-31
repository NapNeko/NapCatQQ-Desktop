// ListItem: GSAP 版列表项 wrapper。第二轮重写。
//
// stagger 由父级 useGSAP 调用,本组件只管 hover lift。
//
// 行式大卡片(如 BotCard / 容器卡)用 hover 默认值会出现:
//   - scale 1.04 在 px-2 容器里被 ring/shadow 裁切
//   - brightness 1.04 叠加 Tailwind hover:bg-elevated/90 看起来"突然变白"
// 所以这里显式 opt-out scale/brightness,只保留 lift + shadow,符合 IBM/Material
// "卡片悬停应抬起,不应放大"的设计共识。

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
            // 行式大卡:不 scale、不 brightness、保留 lift + shadow,避免大尺寸放大裁切。
            return m.bindHover(el, { scale: 1, brightness: false });
        }, [hoverable, m.enabled, m.level, m.speed]);

        return <div ref={localRef} {...rest} />;
    },
);
ListItem.displayName = 'ListItem';
