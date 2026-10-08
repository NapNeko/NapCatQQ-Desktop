import type { Virtualizer } from '@tanstack/react-virtual';
import type { RefObject } from 'react';

export function preserveTimelineReading(
    virtual: Virtualizer<HTMLDivElement, Element>,
    scroll: RefObject<HTMLDivElement>,
) {
    // 首测也会移动后续正文，跳过它只稳定 scrollTop，反而让阅读中的消息跳动。
    // 收缩后移出顶部的图片也要补偿，否则它下面的正文会在缓动途中突然反跳。
    virtual.shouldAdjustScrollPositionOnItemSizeChange = (item, delta) =>
        Math.min(item.end, item.end + delta) <=
        (scroll.current?.scrollTop ?? virtual.scrollOffset ?? 0);
}
