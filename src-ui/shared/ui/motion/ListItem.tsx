// ListItem: 列表项进退场原子件。
//
// 用法:
//   <motion.div /* 父容器 */ variants={listContainerVariants(stagger)} initial="initial" animate="animate">
//     {items.map(it => (
//       <ListItem key={it.id}>...</ListItem>
//     ))}
//   </motion.div>
//
// 父级负责 stagger,子项各自承担 fade+slide+scale。子项的 transition 走全局
// MotionConfig 默认值即可,不再单独传(否则 stagger 被覆盖会失效)。
//
// 注意:父容器外面通常还要 <AnimatePresence>,这样列表项被删除时也有 exit 动画。
//
// hoverable=true 时项目本身会承担 hover 上抬手感(rich/standard 档),
// 等同于在外层套 MotionCard 的效果但不需要侵入卡片组件本身。

import { forwardRef, type HTMLAttributes } from 'react';
import { motion } from 'framer-motion';
import { listItemVariants } from '../../../core/design/motion';
import { useMotion } from '../../../hooks/preferences/useMotion';

interface ListItemProps extends HTMLAttributes<HTMLDivElement> {
    /// layout=true 时被同辈插入/删除会平滑滑动到新位置。默认 false 避免不必要重布局。
    layout?: boolean;
    /// hoverable=true 时启用 hover lift。默认 false。
    hoverable?: boolean;
}

export const ListItem = forwardRef<HTMLDivElement, ListItemProps>(
    ({ layout, hoverable, ...rest }, ref) => {
        const m = useMotion();
        const lift = hoverable ? m.preset.cardLift : 0;
        const hoverProps = lift > 0 && m.enabled
            ? { whileHover: { y: -lift } }
            : {};
        return (
            <motion.div
                ref={ref}
                variants={listItemVariants}
                layout={layout}
                {...hoverProps}
                {...(rest as Parameters<typeof motion.div>[0])}
            />
        );
    },
);
ListItem.displayName = 'ListItem';
