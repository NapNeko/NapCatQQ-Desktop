// 收起只改变布局与可交互性，编辑器和异步附件任务保持挂载。
import { useLayoutEffect, useRef, type RefObject } from 'react';
import gsap from 'gsap';
import { useMotion } from '../../hooks/preferences/useMotion';

export function useComposerCollapse(
    composer: RefObject<HTMLDivElement>,
    collapsed: boolean,
    onExpanded: () => void,
) {
    const motion = useMotion();
    const initialized = useRef(false);
    useLayoutEffect(() => {
        const element = composer.current;
        if (!element) return;
        gsap.killTweensOf(element);
        delete element.dataset.collapseAnimating;
        const clear = 'height,paddingTop,paddingBottom,overflow,opacity,transform';
        if (!initialized.current || !motion.enabled) {
            initialized.current = true;
            if (collapsed)
                gsap.set(element, {
                    height: 0,
                    paddingTop: 0,
                    paddingBottom: 0,
                    overflow: 'hidden',
                    opacity: 0,
                });
            else {
                gsap.set(element, { clearProps: clear });
                onExpanded();
            }
            return () => {
                gsap.killTweensOf(element);
                delete element.dataset.collapseAnimating;
            };
        }
        if (collapsed) {
            element.dataset.collapseAnimating = 'true';
            gsap.set(element, {
                height: element.getBoundingClientRect().height,
                overflow: 'hidden',
            });
            gsap.to(element, {
                height: 0,
                paddingTop: 0,
                paddingBottom: 0,
                opacity: 0,
                y: 8,
                duration: motion.duration('base'),
                ease: motion.ease.exit,
                onComplete: () => {
                    delete element.dataset.collapseAnimating;
                },
            });
        } else {
            const height = element.getBoundingClientRect().height;
            const before = getComputedStyle(element);
            const paddingTop = before.paddingTop;
            const paddingBottom = before.paddingBottom;
            gsap.set(element, { clearProps: 'height,paddingTop,paddingBottom' });
            const naturalHeight = element.getBoundingClientRect().height;
            const natural = getComputedStyle(element);
            const naturalTop = natural.paddingTop;
            const naturalBottom = natural.paddingBottom;
            gsap.set(element, { height, paddingTop, paddingBottom, overflow: 'hidden' });
            element.dataset.collapseAnimating = 'true';
            gsap.to(element, {
                height: naturalHeight,
                paddingTop: naturalTop,
                paddingBottom: naturalBottom,
                opacity: 1,
                y: 0,
                duration: motion.duration('base'),
                ease: motion.ease.release,
                onComplete: () => {
                    delete element.dataset.collapseAnimating;
                    gsap.set(element, { clearProps: clear });
                    onExpanded();
                },
            });
        }
        return () => {
            gsap.killTweensOf(element);
            delete element.dataset.collapseAnimating;
        };
    }, [composer, collapsed, onExpanded, motion.enabled, motion.level, motion.speed]);
}
