import { Virtualizer, type VirtualizerOptions } from '@tanstack/react-virtual';
import { describe, expect, it } from 'vitest';
import { preserveTimelineReading } from './timelineReadingAnchor';

function timeline() {
    const element = document.createElement('div');
    let observeOffset!: (offset: number, isScrolling: boolean) => void;
    const options: VirtualizerOptions<HTMLDivElement, Element> = {
        count: 20, getScrollElement: () => element, estimateSize: () => 100, getItemKey: index => `message:${index}`,
        initialRect: { width: 400, height: 300 }, initialOffset: 1000, anchorTo: 'end',
        observeElementRect: (_, callback) => { callback({ width: 400, height: 300 }); },
        observeElementOffset: (_, callback) => { observeOffset = callback; callback(1000, false); },
        scrollToFn: (offset, { adjustments = 0 }) => { element.scrollTop = offset + adjustments; },
    };
    const virtual = new Virtualizer(options);
    preserveTimelineReading(virtual, { current: element });
    virtual._willUpdate(); virtual.getVirtualItems();
    element.scrollTop = 600; observeOffset(600, true);
    return { virtual, element, cleanup: virtual._didMount() };
}

describe('asynchronous media while scrolling upward', () => {
    it.each([300, 40])('keeps the visible message fixed when an image above it becomes %i pixels high', size => {
        const { virtual, element, cleanup } = timeline();
        expect(virtual.scrollDirection).toBe('backward');
        const before = virtual.getMeasurements()[6].start - element.scrollTop;
        virtual.resizeItem(1, size);
        const after = virtual.getMeasurements()[6].start - element.scrollTop;
        expect(after).toBe(before);
        expect(element.scrollTop).toBe(600 + size - 100);
        cleanup();
    });
    it('does not scroll when an image below the viewport changes size', () => {
        const { virtual, element, cleanup } = timeline();
        virtual.resizeItem(15, 300);
        expect(element.scrollTop).toBe(600);
        cleanup();
    });
    it('accounts for multiple images resizing before the next scroll event', () => {
        const { virtual, element, cleanup } = timeline();
        const before = virtual.getMeasurements()[6].start - element.scrollTop;
        virtual.resizeItem(1, 300); virtual.resizeItem(3, 160);
        expect(virtual.getMeasurements()[6].start - element.scrollTop).toBe(before);
        cleanup();
    });
});
