import { act, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { useSmoothWheel } from './useSmoothWheel';

afterEach(() => vi.unstubAllGlobals());

describe('smooth mouse wheel', () => {
    function setup(enabled = true) {
        const element = document.createElement('div'); element.scrollTop = 400;
        Object.defineProperty(element, 'clientHeight', { value: 400 });
        let nextFrame: FrameRequestCallback | undefined;
        vi.stubGlobal('requestAnimationFrame', vi.fn((callback: FrameRequestCallback) => { nextFrame = callback; return 1; }));
        vi.stubGlobal('cancelAnimationFrame', vi.fn(() => { nextFrame = undefined; }));
        const hook = renderHook(() => useSmoothWheel({ current: element }, enabled));
        const wheel = (deltaY: number) => {
            const event = new WheelEvent('wheel', { deltaY, cancelable: true });
            act(() => element.dispatchEvent(event)); return event;
        };
        const frame = (time: number) => act(() => { const callback = nextFrame; nextFrame = undefined; callback?.(time); });
        return { element, wheel, frame, hook };
    }
    it('spreads a mouse wheel over frames and preserves a prepend anchor correction', () => {
        const { element, wheel, frame } = setup();
        expect(wheel(-120).defaultPrevented).toBe(true);
        expect(element.scrollTop).toBe(400);
        frame(16);
        expect(element.scrollTop).toBeGreaterThan(280); expect(element.scrollTop).toBeLessThan(400);
        element.scrollTop += 500;
        frame(32);
        expect(element.scrollTop).toBeGreaterThan(780); expect(element.scrollTop).toBeLessThan(900);
    });
    it('leaves fine trackpad input and reduced motion to the browser', () => {
        const normal = setup();
        expect(normal.wheel(-2.5).defaultPrevented).toBe(false);
        normal.frame(16); expect(normal.element.scrollTop).toBe(400);
        normal.hook.unmount();
        const reduced = setup(false);
        expect(reduced.wheel(-120).defaultPrevented).toBe(false);
        reduced.frame(16); expect(reduced.element.scrollTop).toBe(400);
    });
    it('cancels remaining movement when the user takes over with the scrollbar', () => {
        const { element, wheel, frame } = setup();
        wheel(-120); frame(16);
        act(() => element.dispatchEvent(new Event('pointerdown')));
        const top = element.scrollTop; frame(32);
        expect(element.scrollTop).toBe(top);
    });
});
