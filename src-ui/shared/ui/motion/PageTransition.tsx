// PageTransition: 路由级页面过渡。
//
// 走 fade + slide-y + 微缩放的克制动画。退场旧页 -y8 + scale 0.99,新页从 y10 + scale 0.985
// 进入。配合 AppNext.tsx 的 <AnimatePresence mode="wait">,确保旧页退完新页才进,
// 不会"上下页同时存在导致布局抖动"。
//
// rich 档进场走 spring 让落位带轻微弹性,标准/优雅档走 tween。

import { motion } from 'framer-motion';
import { type ReactNode } from 'react';
import { useMotion } from '../../../hooks/preferences/useMotion';
import { pageVariants } from '../../../core/design/motion';

interface PageTransitionProps {
    children: ReactNode;
    className?: string;
}

export function PageTransition({ children, className }: PageTransitionProps) {
    const m = useMotion();
    // rich 档(bouncyOvershoot > 1)进场走 spring,其它档 tween。退场永远 tween
    // (variants 内已经写死 exit transition.duration,无需在这里区分)。
    const enterTransition = m.preset.bouncyOvershoot > 1
        ? m.transition('spring')
        : m.transition('slow');

    return (
        <motion.div
            className={className}
            variants={pageVariants}
            initial="initial"
            animate="animate"
            exit="exit"
            transition={enterTransition}
        >
            {children}
        </motion.div>
    );
}
