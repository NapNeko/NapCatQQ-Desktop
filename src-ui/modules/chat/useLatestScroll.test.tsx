import { act, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { useLatestScroll } from './useLatestScroll';

afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); document.body.replaceChildren(); });
function setup(enabled = true) {
    const element = document.createElement('div'); document.body.append(element);
    element.scrollTop = 200;
    let height = 4000;
    Object.defineProperties(element, { clientHeight: { value: 400 }, scrollHeight: { get: () => height } });
    let next: FrameRequestCallback | undefined;
    vi.spyOn(performance, 'now').mockReturnValue(0);
    vi.stubGlobal('requestAnimationFrame', vi.fn((callback: FrameRequestCallback) => { next = callback; return 1; }));
    vi.stubGlobal('cancelAnimationFrame', vi.fn(() => { next = undefined; }));
    const finish = vi.fn(() => { element.scrollTop = height - 400; }); const detach = vi.fn();
    const scroll = { current: element };
    const hook = renderHook(({ animate }) => useLatestScroll(scroll, { enabled: animate, duration: 400, finish, detach }), { initialProps: { animate: enabled } });
    const frame = (time: number) => act(() => { const callback = next; next = undefined; callback?.(time); });
    return { element, hook, frame, finish, detach, resize: (value: number) => { height = value; } };
}
describe('return to latest animation', () => {
    it('travels through the list without an initial teleport and lands at the actual bottom', () => {
        const { element, hook, frame, finish, detach } = setup();
        act(() => hook.result.current.start());
        expect(element.scrollTop).toBe(200); expect(detach).toHaveBeenCalledOnce();
        frame(16); expect(element.scrollTop).toBeGreaterThan(200); expect(element.scrollTop).toBeLessThan(1000);
        let previous = element.scrollTop;
        for (let time = 32; time <= 800; time += 16) { frame(time); expect(element.scrollTop).toBeGreaterThanOrEqual(previous); previous = element.scrollTop; }
        expect(element.scrollTop).toBe(3600); expect(finish).toHaveBeenCalledOnce(); expect(hook.result.current.running).toBe(false);
    });
    it('keeps heading down when a measured row or new message changes the end', () => {
        const { element, hook, frame, resize } = setup();
        act(() => hook.result.current.start()); frame(100);
        resize(4800); element.scrollTop += 800;
        const compensated = element.scrollTop; frame(116);
        expect(element.scrollTop).toBeGreaterThan(compensated);
        frame(800); expect(element.scrollTop).toBe(4400);
    });
    it('cancels immediately on user takeover without a delayed snap', () => {
        const { element, hook, frame, finish } = setup();
        act(() => hook.result.current.start()); frame(100);
        act(() => hook.result.current.cancel());
        const top = element.scrollTop; frame(800);
        expect(element.scrollTop).toBe(top); expect(finish).not.toHaveBeenCalled(); expect(hook.result.current.running).toBe(false);
    });
    it('snaps immediately with reduced motion and cancels when animation is disabled midflight', () => {
        const reduced = setup(false); act(() => reduced.hook.result.current.start());
        expect(reduced.element.scrollTop).toBe(3600); expect(reduced.hook.result.current.running).toBe(false); reduced.hook.unmount();
        const normal = setup(); act(() => normal.hook.result.current.start()); normal.frame(100);
        normal.hook.rerender({ animate: false }); const top = normal.element.scrollTop; normal.frame(800);
        expect(normal.element.scrollTop).toBe(top); expect(normal.finish).not.toHaveBeenCalled();
    });
    it('releases its animation frame when unmounted', () => {
        const { element, hook, frame, finish } = setup();
        act(() => hook.result.current.start()); frame(100); hook.unmount();
        const top = element.scrollTop; frame(800);
        expect(element.scrollTop).toBe(top); expect(finish).not.toHaveBeenCalled();
    });
});
