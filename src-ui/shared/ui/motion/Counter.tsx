// Counter: 数字 rolling 切换。rich 档启用,其它档直接显示新值。
//
// 实现:框定固定字号 + 等宽数字,用 framer 的 motion.span 在 key 变化时
// 上一个数字向上推出 + 新数字从下方滑入。仅整数,不支持小数 / 千分号。

import { AnimatePresence, motion } from 'framer-motion';
import { useMotion } from '../../../hooks/preferences/useMotion';

interface CounterProps {
    value: number;
    className?: string;
}

export function Counter({ value, className }: CounterProps) {
    const m = useMotion();
    const enabled = m.enabled && m.preset.bouncyOvershoot > 1;

    if (!enabled) {
        return <span className={className}>{value}</span>;
    }

    return (
        <span
            className={`relative inline-block tabular-nums ${className ?? ''}`}
            style={{ overflow: 'hidden' }}
        >
            <AnimatePresence mode="popLayout" initial={false}>
                <motion.span
                    key={value}
                    initial={{ y: '100%', opacity: 0 }}
                    animate={{ y: 0, opacity: 1 }}
                    exit={{ y: '-100%', opacity: 0, position: 'absolute' }}
                    transition={m.transition('spring')}
                    style={{ display: 'inline-block', left: 0, right: 0 }}
                >
                    {value}
                </motion.span>
            </AnimatePresence>
        </span>
    );
}
