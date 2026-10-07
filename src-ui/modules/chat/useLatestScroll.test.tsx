import { act, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { useLatestScroll } from './useLatestScroll';

afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    document.body.replaceChildren();
});
function setup(enabled = true) {
    const element = document.createElement('div');
    document.body.append(element);
    element.scrollTop = 200;
    let height = 4000;
    Object.defineProperties(element, {
        clientHeight: { value: 400 },
        scrollHeight: { get: () => height },
    });
    let next: FrameRequestCallback | undefined;
    let resized: ResizeObserverCallback | undefined;
    vi.stubGlobal(
        'ResizeObserver',
        class {
            constructor(callback: ResizeObserverCallback) {
                resized = callback;
            }
            observe() {}
            disconnect() {
                resized = undefined;
            }
        },
    );
    vi.spyOn(performance, 'now').mockReturnValue(0);
    vi.stubGlobal(
        'requestAnimationFrame',
        vi.fn((callback: FrameRequestCallback) => {
            next = callback;
            return 1;
        }),
    );
    vi.stubGlobal(
        'cancelAnimationFrame',
        vi.fn(() => {
            next = undefined;
        }),
    );
    const finish = vi.fn(() => {
        element.scrollTop = height - 400;
    });
    const detach = vi.fn();
    const scroll = { current: element };
    const hook = renderHook(
        ({ animate }) =>
            useLatestScroll(scroll, { enabled: animate, duration: 400, finish, detach }),
        { initialProps: { animate: enabled } },
    );
    const frame = (time: number) =>
        act(() => {
            const callback = next;
            next = undefined;
            callback?.(time);
        });
    return {
        element,
        hook,
        frame,
        finish,
        detach,
        resize: (value: number) => {
            height = value;
            resized?.([], {} as ResizeObserver);
        },
    };
}
describe('return to latest animation', () => {
    it('travels through the list without an initial teleport and lands at the actual bottom', () => {
        const { element, hook, frame, finish, detach } = setup();
        act(() => hook.result.current.start());
        expect(element.scrollTop).toBe(200);
        expect(detach).toHaveBeenCalledOnce();
        frame(16);
        expect(element.scrollTop).toBeGreaterThan(200);
        expect(element.scrollTop).toBeLessThan(1000);
        let previous = element.scrollTop;
        for (let time = 32; time <= 800; time += 16) {
            frame(time);
            expect(element.scrollTop).toBeGreaterThanOrEqual(previous);
            previous = element.scrollTop;
        }
        expect(element.scrollTop).toBe(3600);
        expect(finish).toHaveBeenCalledOnce();
        expect(hook.result.current.running).toBe(false);
    });
    it('keeps heading down when a measured row or new message changes the end', () => {
        const { element, hook, frame, resize } = setup();
        act(() => hook.result.current.start());
        frame(100);
        resize(4800);
        element.scrollTop += 800;
        const compensated = element.scrollTop;
        frame(116);
        expect(element.scrollTop).toBeGreaterThan(compensated);
        frame(800);
        expect(element.scrollTop).toBe(4400);
    });
    it('keeps settling when the virtual canvas grows after the first final snap', () => {
        const { element, hook, frame, resize, finish } = setup();
        act(() => hook.result.current.start());
        frame(800);
        expect(element.scrollTop).toBe(3600);
        expect(finish).not.toHaveBeenCalled();
        resize(4600);
        frame(816);
        expect(element.scrollTop).toBe(4200);
        frame(832);
        frame(848);
        expect(finish).toHaveBeenCalledOnce();
        expect(hook.result.current.isRunning()).toBe(false);
    });
    it('owns the load wait and does not restart scrolling after a user cancels it', async () => {
        const { element, hook, frame, finish } = setup();
        let resolve!: () => void;
        let signal!: AbortSignal;
        const load = vi.fn((value: AbortSignal) => {
            signal = value;
            return new Promise<void>((done) => {
                resolve = done;
            });
        });
        act(() => hook.result.current.start(load));
        expect(hook.result.current.isRunning()).toBe(true);
        expect(element.scrollTop).toBe(200);
        act(() => hook.result.current.cancel());
        expect(signal.aborted).toBe(true);
        await act(async () => resolve());
        frame(800);
        expect(element.scrollTop).toBe(200);
        expect(finish).not.toHaveBeenCalled();
    });
    it('moves immediately while history loads and settles at the committed page bottom', async () => {
        const { element, hook, frame, resize, finish } = setup();
        let resolve!: () => void;
        act(() =>
            hook.result.current.start(
                () =>
                    new Promise<void>((done) => {
                        resolve = done;
                    }),
            ),
        );
        frame(16);
        expect(element.scrollTop).toBeGreaterThan(200);
        frame(300);
        frame(316);
        frame(332);
        expect(element.scrollTop).toBe(3600);
        expect(finish).not.toHaveBeenCalled();
        expect(hook.result.current.isRunning()).toBe(true);
        expect(requestAnimationFrame).toHaveBeenCalledTimes(4);
        resize(6000);
        frame(348);
        expect(element.scrollTop).toBe(5600);
        await act(async () => resolve());
        frame(364);
        frame(380);
        expect(element.scrollTop).toBe(5600);
        expect(finish).toHaveBeenCalledOnce();
    });
    it('does not pull the user back when a late history response arrives after takeover', async () => {
        const { element, hook, frame, finish } = setup();
        let resolve!: () => void;
        let signal!: AbortSignal;
        act(() =>
            hook.result.current.start((value) => {
                signal = value;
                return new Promise<void>((done) => {
                    resolve = done;
                });
            }),
        );
        frame(300);
        frame(316);
        frame(332);
        expect(element.scrollTop).toBe(3600);
        act(() => hook.result.current.cancel());
        element.scrollTop = 1200;
        await act(async () => resolve());
        frame(400);
        expect(signal.aborted).toBe(true);
        expect(element.scrollTop).toBe(1200);
        expect(finish).not.toHaveBeenCalled();
    });
    it('cancels immediately on user takeover without a delayed snap', () => {
        const { element, hook, frame, finish } = setup();
        act(() => hook.result.current.start());
        frame(100);
        act(() => hook.result.current.cancel());
        const top = element.scrollTop;
        frame(800);
        expect(element.scrollTop).toBe(top);
        expect(finish).not.toHaveBeenCalled();
        expect(hook.result.current.running).toBe(false);
    });
    it('snaps immediately with reduced motion and cancels when animation is disabled midflight', () => {
        const reduced = setup(false);
        act(() => reduced.hook.result.current.start());
        expect(reduced.element.scrollTop).toBe(3600);
        expect(reduced.hook.result.current.running).toBe(false);
        reduced.hook.unmount();
        const normal = setup();
        act(() => normal.hook.result.current.start());
        normal.frame(100);
        normal.hook.rerender({ animate: false });
        const top = normal.element.scrollTop;
        normal.frame(800);
        expect(normal.element.scrollTop).toBe(top);
        expect(normal.finish).not.toHaveBeenCalled();
    });
    it('releases its animation frame when unmounted', () => {
        const { element, hook, frame, finish } = setup();
        act(() => hook.result.current.start());
        frame(100);
        hook.unmount();
        const top = element.scrollTop;
        frame(800);
        expect(element.scrollTop).toBe(top);
        expect(finish).not.toHaveBeenCalled();
    });
});
