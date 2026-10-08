import { useLayoutEffect, useState } from 'react';

/** 量一个元素的内容宽度（不含边框）；还没量到是 null */
export function useElementWidth(el: HTMLElement | null): number | null {
    const [width, setWidth] = useState<number | null>(null);
    useLayoutEffect(() => {
        if (!el) {
            setWidth(null);
            return;
        }
        // 用 clientWidth 而不是 getBoundingClientRect：进场动画里的 scale 不该算进来
        setWidth(el.clientWidth || null);
        if (typeof ResizeObserver === 'undefined') return;
        const ro = new ResizeObserver((entries) => {
            const w = entries[0]?.contentRect.width;
            if (w) setWidth(Math.round(w));
        });
        ro.observe(el);
        return () => ro.disconnect();
    }, [el]);
    return width;
}
