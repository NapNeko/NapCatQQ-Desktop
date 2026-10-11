import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({ enabled: true }));
vi.mock('../../hooks/preferences/useMotion', () => ({
    useMotion: () => ({
        enabled: mocks.enabled,
        level: 'standard',
        speed: 1,
        duration: () => 0.3,
        ease: { exit: 'power2.in', damped: 'power3.out' },
    }),
}));
import {
    conversationSwitchDirection,
    useConversationTransition,
} from './useConversationTransition';

let scope: { current: HTMLDivElement };
let queued: Map<number, FrameRequestCallback>;
let animations: Array<{
    element: HTMLElement;
    keyframes: Keyframe[];
    finish: () => void;
    cancel: ReturnType<typeof vi.fn>;
}>;
let originalAnimate: PropertyDescriptor | undefined;

function frame() {
    const batch = [...queued.values()];
    queued.clear();
    act(() => batch.forEach((callback) => callback(performance.now())));
}

beforeEach(() => {
    mocks.enabled = true;
    queued = new Map();
    animations = [];
    let id = 0;
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
        queued.set(++id, callback);
        return id;
    });
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation((key) => {
        queued.delete(key);
    });
    originalAnimate = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'animate');
    Object.defineProperty(HTMLElement.prototype, 'animate', {
        configurable: true,
        value: function (this: HTMLElement, keyframes: Keyframe[]) {
            let resolve!: () => void;
            let reject!: (error: unknown) => void;
            const finished = new Promise<void>((yes, no) => {
                resolve = yes;
                reject = no;
            });
            const cancel = vi.fn(() => reject(new DOMException('Cancelled', 'AbortError')));
            animations.push({ element: this, keyframes, finish: resolve, cancel });
            return { finished, cancel };
        },
    });
    scope = { current: document.createElement('div') };
    const viewport = document.createElement('div');
    viewport.className = 'native-chat-timeline-frame';
    viewport.innerHTML =
        '<div class="native-chat-timeline-wrap"><div class="native-chat-timeline"><button id="old-button">旧消息</button></div></div>';
    scope.current.append(viewport);
    document.body.append(scope.current);
    vi.spyOn(viewport, 'getClientRects').mockReturnValue([{}] as unknown as DOMRectList);
    vi.spyOn(viewport, 'clientHeight', 'get').mockReturnValue(400);
    vi.spyOn(document, 'hidden', 'get').mockReturnValue(false);
});
afterEach(() => {
    scope.current.remove();
    if (originalAnimate) Object.defineProperty(HTMLElement.prototype, 'animate', originalAnimate);
    else Reflect.deleteProperty(HTMLElement.prototype, 'animate');
});

describe('composited conversation handoff', () => {
    it.each([
        { order: ['private:1', 'private:2', 'private:3'], next: 'private:3', shift: -400 },
        { order: ['private:1', 'private:2', 'private:3'], next: 'private:1', shift: 400 },
        { order: ['private:3', 'private:2', 'private:1'], next: 'private:3', shift: 400 },
        { order: ['private:1', 'private:3'], next: 'private:3', shift: -400 },
    ])('uses the visible list order $order when opening $next', ({ order, next, shift }) => {
        const { result, rerender } = renderHook(
            ({ token }) => useConversationTransition(scope, token),
            { initialProps: { token: 'private:2' } },
        );
        const direction = conversationSwitchDirection(
            order.map((key) => ({ key })),
            'private:2',
            next,
        );
        act(() => result.current(next, vi.fn(), direction));
        frame();
        expect(animations[0].keyframes[1].transform).toBe(`translate3d(0,${shift}px,0)`);
        frame();
        rerender({ token: next });
        expect(animations[1].keyframes[0].transform).toBe(`translate3d(0,${-shift}px,0)`);
    });

    it('coalesces rapid selections and commits before the outgoing animation finishes', async () => {
        const { result, rerender } = renderHook(
            ({ token }) => useConversationTransition(scope, token),
            {
                initialProps: { token: 'private:1' },
            },
        );
        const first = vi.fn();
        const latest = vi.fn();
        act(() => {
            result.current('private:2', first);
            result.current('private:3', latest, 'down');
        });
        frame();
        expect(animations).toHaveLength(1);
        expect(animations[0].keyframes[1].transform).toBe('translate3d(0,400px,0)');
        const snapshot = scope.current.querySelector('[data-chat-snapshot]');
        expect(snapshot).toHaveAttribute('aria-hidden', 'true');
        expect(snapshot?.querySelector('[id]')).toBeNull();
        frame();
        expect(first).not.toHaveBeenCalled();
        expect(latest).toHaveBeenCalledOnce();
        rerender({ token: 'private:3' });
        expect(animations).toHaveLength(2);
        expect(animations[1].keyframes[0].transform).toBe('translate3d(0,-400px,0)');
        expect(animations[1].element).not.toHaveAttribute('data-chat-snapshot');
        act(() => result.current('private:3', vi.fn()));
        expect(animations[1].cancel).not.toHaveBeenCalled();
        await act(async () => {
            animations.forEach((animation) => animation.finish());
        });
        expect(scope.current.querySelector('[data-chat-snapshot]')).toBeNull();
        expect(scope.current.querySelector('.native-chat-timeline-frame')).not.toHaveAttribute(
            'data-switching',
        );
    });

    it('never keeps more than one frozen outgoing view during further selections', () => {
        const { result, rerender } = renderHook(
            ({ token }) => useConversationTransition(scope, token),
            {
                initialProps: { token: 'private:1' },
            },
        );
        act(() => result.current('private:2', vi.fn()));
        frame();
        frame();
        rerender({ token: 'private:2' });
        act(() => result.current('private:3', vi.fn()));
        frame();
        frame();
        rerender({ token: 'private:3' });
        expect(scope.current.querySelectorAll('[data-chat-snapshot]')).toHaveLength(1);
        expect(animations).toHaveLength(3);
    });

    it('does not reuse a previous list direction for external navigation', () => {
        const { result, rerender } = renderHook(
            ({ token }) => useConversationTransition(scope, token),
            { initialProps: { token: 'private:1' } },
        );
        act(() => result.current('private:2', vi.fn(), 'down'));
        frame();
        frame();
        rerender({ token: 'private:2' });
        expect(animations[1].keyframes[0].transform).toBe('translate3d(0,-400px,0)');
        rerender({ token: 'private:4' });
        expect(animations[2].keyframes[0].transform).toBe('translate3d(0,400px,0)');
    });

    it.each([null, 'private:4'])(
        'an external navigation to %s supersedes a queued selection',
        (token) => {
            const { result, rerender } = renderHook(
                ({ token }) => useConversationTransition(scope, token),
                {
                    initialProps: { token: 'private:1' as string | null },
                },
            );
            const stale = vi.fn();
            act(() => result.current('private:2', stale));
            frame();
            rerender({ token });
            frame();
            expect(stale).not.toHaveBeenCalled();
            expect(scope.current.querySelector('[data-chat-snapshot]')).toBeNull();
        },
    );

    it('returning to the list cancels a pending selection even when the active token stays unchanged', () => {
        const { result } = renderHook(() => useConversationTransition(scope, 'private:1'));
        const stale = vi.fn();
        act(() => result.current('private:2', stale));
        frame();
        act(() => result.current.cancel());
        frame();
        expect(stale).not.toHaveBeenCalled();
        expect(scope.current.querySelector('[data-chat-snapshot]')).toBeNull();
        expect(scope.current.querySelector('.native-chat-timeline-frame')).not.toHaveAttribute(
            'data-switching',
        );
        expect(animations[0].cancel).toHaveBeenCalled();
    });

    it('turning motion off commits the latest selection and releases its frozen frame', () => {
        const { result, rerender } = renderHook(() =>
            useConversationTransition(scope, 'private:1'),
        );
        const latest = vi.fn();
        act(() => result.current('private:2', latest));
        frame();
        mocks.enabled = false;
        rerender();
        frame();
        expect(latest).toHaveBeenCalledOnce();
        expect(scope.current.querySelector('[data-chat-snapshot]')).toBeNull();
        expect(scope.current.querySelector('.native-chat-timeline-frame')).not.toHaveAttribute(
            'data-switching',
        );
    });

    it('switches immediately without motion and discards pending work on unmount', () => {
        mocks.enabled = false;
        const instant = renderHook(() => useConversationTransition(scope, 'private:1'));
        const commit = vi.fn();
        act(() => instant.result.current('private:2', commit));
        expect(commit).toHaveBeenCalledOnce();
        expect(animations).toHaveLength(0);
        instant.unmount();
        mocks.enabled = true;
        const active = renderHook(() => useConversationTransition(scope, 'private:1'));
        const stale = vi.fn();
        act(() => active.result.current('private:2', stale));
        frame();
        active.unmount();
        frame();
        expect(stale).not.toHaveBeenCalled();
        expect(scope.current.querySelector('[data-chat-snapshot]')).toBeNull();
    });
});
