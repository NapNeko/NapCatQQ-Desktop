import type { Virtualizer } from '@tanstack/react-virtual';
import type { RefObject } from 'react';

// 默认虚拟列表在上滚时跳过行高补偿。聊天媒体会异步加载，因此即使仍在
// 上滚，也要抵消视口上方的高度变化；读取实际 scrollTop 避免多张图同帧加载时滞后。
export function preserveTimelineReading(virtual: Virtualizer<HTMLDivElement, Element>, scroll: RefObject<HTMLDivElement>) {
    virtual.shouldAdjustScrollPositionOnItemSizeChange = item => item.start < (scroll.current?.scrollTop ?? virtual.scrollOffset ?? 0);
}
