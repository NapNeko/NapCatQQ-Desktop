import { Virtualizer, type VirtualizerOptions } from '@tanstack/react-virtual';
import { describe, expect, it } from 'vitest';
import { preserveTimelineReading } from './timelineReadingAnchor';

function timeline(measured = true) {
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
    if (measured) for (let index = 0; index < options.count; index++) virtual.itemSizeCache.set(`message:${index}`, 100);
    preserveTimelineReading(virtual, { current: element });
    virtual._willUpdate(); virtual.getVirtualItems();
    element.scrollTop = 600; observeOffset(600, true);
    return { virtual, element, observeOffset, cleanup: virtual._didMount() };
}

describe('asynchronous media while scrolling upward', () => {
    it('keeps the visible message fixed through first measurements during upward scrolling', () => {
        const { virtual, element, cleanup } = timeline(false);
        const before = virtual.getMeasurements()[6].start - element.scrollTop;
        virtual.resizeItem(1, 83); virtual.resizeItem(2, 85); virtual.resizeItem(3, 83);
        expect(virtual.getMeasurements()[6].start - element.scrollTop).toBe(before);
        expect(virtual.itemSizeCache.get('message:1')).toBe(83);
        cleanup();
    });
    it('keeps the same reading anchor after a media correction reports a forward scroll event', () => {
        const { virtual, element, observeOffset, cleanup } = timeline(false);
        const before = virtual.getMeasurements()[6].start - element.scrollTop;
        virtual.itemSizeCache.set('message:1', 100);
        virtual.resizeItem(1, 200); observeOffset(element.scrollTop, true);
        expect(virtual.scrollDirection).toBe('forward');
        virtual.resizeItem(3, 83);
        expect(virtual.getMeasurements()[6].start - element.scrollTop).toBe(before);
        cleanup();
    });
    it.each([true, false])('does not jump when a small upward step mounts a tall unmeasured row (scrolling: %s)', scrolling => {
        const { virtual, element, observeOffset, cleanup } = timeline(false);
        const before = virtual.getMeasurements()[6].start - element.scrollTop;
        element.scrollTop -= 15; observeOffset(element.scrollTop, scrolling);
        virtual.resizeItem(2, 280);
        expect(virtual.getMeasurements()[6].start - element.scrollTop).toBe(before + 15);
        cleanup();
    });
    it('retains the visible anchor while several new rows shrink after a small wheel step', () => {
        const { virtual, element, observeOffset, cleanup } = timeline(false);
        const before = virtual.getMeasurements()[6].start - element.scrollTop;
        element.scrollTop -= 15; observeOffset(element.scrollTop, true);
        for (const index of [1, 2, 3]) virtual.resizeItem(index, 42);
        expect(virtual.getMeasurements()[6].start - element.scrollTop).toBe(before + 15);
        cleanup();
    });
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
    it('does not move a partially visible image as its own height changes', () => {
        const { virtual, element, cleanup } = timeline();
        element.scrollTop = 650;
        virtual.resizeItem(6, 300);
        expect(element.scrollTop).toBe(650);
        cleanup();
    });
    it('keeps the next message fixed when a partially visible image shrinks out of view', () => {
        const { virtual, element, observeOffset, cleanup } = timeline();
        element.scrollTop = 650; observeOffset(650, true);
        const before = virtual.getMeasurements()[7].start - element.scrollTop;
        virtual.resizeItem(6, 40);
        expect(virtual.getMeasurements()[7].start - element.scrollTop).toBe(before);
        expect(element.scrollTop).toBe(590);
        cleanup();
    });
    it('preserves the visible message across a prepend and late image measurements', () => {
        const { virtual, element, cleanup } = timeline();
        const offset = virtual.getMeasurements()[6].start - element.scrollTop;
        virtual.setOptions({ ...virtual.options, count: 30, getItemKey: index => `message:${index - 10}` });
        virtual._willUpdate(); virtual.getVirtualItems();
        expect(virtual.getMeasurements()[16].start - element.scrollTop).toBe(offset);
        virtual.itemSizeCache.set('message:-8', 100); virtual.itemSizeCache.set('message:-5', 100);
        virtual.resizeItem(2, 280); virtual.resizeItem(5, 40);
        expect(virtual.getMeasurements()[16].start - element.scrollTop).toBe(offset);
        cleanup();
    });
    it('compensates the newly inserted overscan rows while an upward gesture is still active', () => {
        const { virtual, element, observeOffset, cleanup } = timeline();
        element.scrollTop = 20; observeOffset(20, true);
        const before = virtual.getMeasurements()[0].start - element.scrollTop;
        virtual.setOptions({ ...virtual.options, count: 30, getItemKey: index => `message:${index - 10}` });
        preserveTimelineReading(virtual, { current: element });
        virtual._willUpdate(); virtual.getVirtualItems();
        virtual.resizeItem(9, 83);
        expect(virtual.getMeasurements()[10].start - element.scrollTop).toBe(before);
        observeOffset(element.scrollTop - 50, true); element.scrollTop -= 50;
        const position = virtual.getMeasurements()[10].start - element.scrollTop;
        virtual.resizeItem(8, 85);
        expect(virtual.getMeasurements()[10].start - element.scrollTop).toBe(position);
        cleanup();
    });
});
