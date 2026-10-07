import { expect, it } from 'vitest';
import { Virtualizer, type VirtualItem } from '@tanstack/react-virtual';
import { captureForwardPosition, forwardPositionOffset } from './forwardPosition';

it('restores the same message after variable-height parent records are remounted', () => {
    let start = 0;
    const measured: VirtualItem[] = Array.from({ length: 41 }, (_, index) => {
        const size = index === 0 ? 48 : 400;
        const row = { index, key: index, start, size, end: start + size, lane: 0 };
        start += size;
        return row;
    });
    const position = captureForwardPosition(10_048, measured);
    expect(position.anchor).toEqual({ index: 26, offset: 0 });
    const restored = new Virtualizer({
        count: 41,
        getScrollElement: () => null,
        estimateSize: (index) => (index === 0 ? 48 : 110),
        initialRect: { width: 600, height: 500 },
        initialMeasurementsCache: position.measurements,
        initialOffset: forwardPositionOffset(position, 41),
        scrollToFn: () => {},
        observeElementRect: () => {},
        observeElementOffset: () => {},
    });
    restored.getVirtualItems();
    expect(restored.range?.startIndex).toBe(26);
    expect(forwardPositionOffset({ ...position, measurements: [] }, 41)).toBe(2_798);
});

it('bounds measurements to the maximum 500 messages plus the summary', () => {
    const rows: VirtualItem[] = Array.from({ length: 900 }, (_, index) => ({
        index,
        key: index,
        start: index * 100,
        end: (index + 1) * 100,
        size: 100,
        lane: 0,
    }));
    const position = captureForwardPosition(20_050, rows);
    expect(position.measurements).toHaveLength(501);
    expect(position.anchor).toEqual({ index: 200, offset: 50 });
    expect(forwardPositionOffset(position, 501)).toBe(20_050);
});
