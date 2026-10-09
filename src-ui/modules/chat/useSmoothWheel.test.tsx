import { act, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { useSmoothWheel } from './useSmoothWheel';

afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
});

describe('smooth mouse wheel', () => {
    function setup(enabled = true) {
        const element = document.createElement('div');
        let height = 2000;
        let top = 400;
        Object.defineProperties(element, {
            clientHeight: { value: 400 },
            scrollHeight: { get: () => height },
            scrollTop: {
                get: () => top,
                set: (value: number) => {
                    top = Math.max(0, Math.min(height - element.clientHeight, value));
                },
            },
        });
        let now = 0;
        vi.spyOn(performance, 'now').mockImplementation(() => now);
        let nextId = 0;
        const frames = new Map<number, FrameRequestCallback>();
        vi.stubGlobal(
            'requestAnimationFrame',
            vi.fn((callback: FrameRequestCallback) => {
                const id = ++nextId;
                frames.set(id, callback);
                return id;
            }),
        );
        vi.stubGlobal(
            'cancelAnimationFrame',
            vi.fn((id: number) => {
                frames.delete(id);
            }),
        );
        const scroll = { current: element };
        const hook = renderHook(({ animate }) => useSmoothWheel(scroll, animate), {
            initialProps: { animate: enabled },
        });
        const wheel = (deltaY: number, options: WheelEventInit = {}) => {
            const event = new WheelEvent('wheel', { deltaY, cancelable: true, ...options });
            act(() => element.dispatchEvent(event));
            return event;
        };
        const frame = (time: number) =>
            act(() => {
                now = time;
                const callbacks = [...frames.values()];
                frames.clear();
                callbacks.forEach((callback) => callback(time));
            });
        return {
            element,
            wheel,
            frame,
            hook,
            pending: () => frames.size,
            time: (value: number) => {
                now = value;
            },
            prepend: (pixels: number) => {
                height += pixels;
                element.scrollTop += pixels;
            },
        };
    }
    it('responds during the wheel event and preserves a prepend anchor correction', () => {
        const { element, wheel, frame, prepend, pending } = setup();
        expect(wheel(-120).defaultPrevented).toBe(true);
        expect(element.scrollTop).toBeLessThan(370);
        expect(element.scrollTop).toBeGreaterThan(280);
        const immediate = element.scrollTop;
        frame(16);
        expect(element.scrollTop).toBeGreaterThan(280);
        expect(element.scrollTop).toBeLessThan(immediate);
        prepend(500);
        const compensated = element.scrollTop;
        frame(32);
        expect(element.scrollTop).toBeGreaterThan(780);
        expect(element.scrollTop).toBeLessThan(compensated);
        for (let time = 48; time <= 192; time += 16) frame(time);
        expect(element.scrollTop).toBeCloseTo(780, 1);
        expect(pending()).toBe(0);
    });
    it('leaves fine trackpad input and reduced motion to the browser', () => {
        const normal = setup();
        expect(normal.wheel(-2.5).defaultPrevented).toBe(false);
        normal.frame(16);
        expect(normal.element.scrollTop).toBe(400);
        normal.hook.unmount();
        const reduced = setup(false);
        expect(reduced.wheel(-120).defaultPrevented).toBe(false);
        reduced.frame(16);
        expect(reduced.element.scrollTop).toBe(400);
    });
    it.each(['pointerdown', 'keydown', 'touchstart'])(
        'cancels remaining movement when the user takes over with %s',
        (event) => {
            const { element, wheel, frame, pending } = setup();
            wheel(-120);
            frame(16);
            act(() => element.dispatchEvent(new Event(event)));
            const top = element.scrollTop;
            expect(pending()).toBe(0);
            frame(32);
            expect(element.scrollTop).toBe(top);
        },
    );
    it('reverses during the input event with a fresh clock and no residual drift', () => {
        const { element, wheel, frame, time, pending } = setup();
        wheel(-120);
        frame(16);
        const reversedAt = element.scrollTop;
        time(100);
        wheel(120);
        expect(element.scrollTop).toBeGreaterThan(reversedAt + 30);
        expect(element.scrollTop).toBeLessThan(reversedAt + 120);
        frame(108);
        expect(element.scrollTop).toBeLessThan(reversedAt + 100);
        let previous = element.scrollTop;
        for (let time = 124; time <= 300; time += 16) {
            frame(time);
            expect(element.scrollTop).toBeGreaterThanOrEqual(previous);
            previous = element.scrollTop;
        }
        expect(element.scrollTop).toBeCloseTo(reversedAt + 120, 1);
        expect(pending()).toBe(0);
    });
    it('catches up after a long frame instead of prolonging wheel inertia', () => {
        const { element, wheel, frame, pending } = setup();
        wheel(120);
        frame(16);
        expect(element.scrollTop).toBeLessThan(520);
        frame(116);
        expect(element.scrollTop).toBeGreaterThan(515);
        expect(element.scrollTop).toBeLessThanOrEqual(520);
        frame(132);
        frame(148);
        expect(element.scrollTop).toBeCloseTo(520, 1);
        expect(pending()).toBe(0);
    });
    it('lets a fractional touchpad gesture take over an unfinished wheel animation', () => {
        const { element, wheel, frame } = setup();
        wheel(-120);
        frame(16);
        const top = element.scrollTop;
        expect(wheel(-12.5).defaultPrevented).toBe(false);
        frame(32);
        expect(element.scrollTop).toBe(top);
    });
    it.each([{ ctrlKey: true }, { shiftKey: true }, { deltaX: 10 }, { cancelable: false }])(
        'hands browser-controlled input over without double scrolling: %o',
        (options) => {
            const { element, wheel, frame, pending } = setup();
            wheel(-120);
            frame(16);
            const top = element.scrollTop;
            expect(wheel(120, options).defaultPrevented).toBe(false);
            expect(pending()).toBe(0);
            frame(32);
            expect(element.scrollTop).toBe(top);
        },
    );
    it.each([
        { top: 0, delta: -120 },
        { top: 1600, delta: 120 },
    ])('leaves outward wheel input at a scroll boundary to the browser: %o', (boundary) => {
        const { element, wheel, frame, pending } = setup();
        element.scrollTop = boundary.top;
        expect(wheel(boundary.delta).defaultPrevented).toBe(false);
        expect(pending()).toBe(0);
        frame(16);
        expect(element.scrollTop).toBe(boundary.top);
    });
    it.each([
        { top: 10, end: 0, delta: -120 },
        { top: 1590, end: 1600, delta: 120 },
    ])('stops the tail after reaching the boundary and hands the next event over: %o', (edge) => {
        const { element, wheel, frame, pending } = setup();
        element.scrollTop = edge.top;
        expect(wheel(edge.delta).defaultPrevented).toBe(true);
        expect(element.scrollTop).toBe(edge.end);
        expect(pending()).toBe(0);
        expect(wheel(edge.delta).defaultPrevented).toBe(false);
        frame(16);
        expect(element.scrollTop).toBe(edge.end);
    });
    it('stops when its timeline becomes hidden and resumes only on new input', () => {
        const { element, wheel, frame, hook, pending } = setup();
        wheel(-120);
        frame(16);
        hook.rerender({ animate: false });
        const top = element.scrollTop;
        expect(pending()).toBe(0);
        expect(wheel(-120).defaultPrevented).toBe(false);
        frame(32);
        expect(element.scrollTop).toBe(top);
        hook.rerender({ animate: true });
        frame(48);
        expect(element.scrollTop).toBe(top);
        expect(wheel(-120).defaultPrevented).toBe(true);
        expect(element.scrollTop).toBeLessThan(top);
    });
    it('removes its listeners and pending frame on unmount', () => {
        const { element, wheel, frame, hook, pending } = setup();
        wheel(-120);
        frame(16);
        hook.unmount();
        const top = element.scrollTop;
        expect(pending()).toBe(0);
        expect(wheel(-120).defaultPrevented).toBe(false);
        frame(32);
        expect(element.scrollTop).toBe(top);
    });
});
