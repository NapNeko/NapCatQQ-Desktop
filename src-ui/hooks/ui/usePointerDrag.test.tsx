// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { usePointerDrag } from './usePointerDrag';

let frames: Map<number, FrameRequestCallback>;
let bodyStyle: string;

function pointer(target: Element | Window | Document, type: string, x = 10, pointerId = 1) {
    const event = new MouseEvent(type, { bubbles: true, button: 0, clientX: x, clientY: 20 });
    Object.defineProperties(event, {
        pointerId: { value: pointerId },
        isPrimary: { value: true },
    });
    fireEvent(target, event);
}

function frame() {
    act(() => {
        const pending = [...frames.values()];
        frames.clear();
        pending.forEach((callback) => callback(16));
    });
}

function Harness({
    onMove,
    onEnd,
}: {
    onMove: (event: PointerEvent) => void;
    onEnd: (reason: string) => void;
}) {
    const drag = usePointerDrag<HTMLButtonElement>({
        cursor: 'col-resize',
        onStart: () => {},
        onMove,
        onEnd,
    });
    return (
        <button onPointerDown={drag.start} data-dragging={drag.dragging}>
            拖动
        </button>
    );
}

function mount() {
    const onMove = vi.fn();
    const onEnd = vi.fn();
    const view = render(<Harness onMove={onMove} onEnd={onEnd} />);
    const button = screen.getByRole('button');
    const release = vi.fn();
    Object.defineProperties(button, {
        setPointerCapture: { configurable: true, value: vi.fn() },
        hasPointerCapture: { configurable: true, value: () => true },
        releasePointerCapture: { configurable: true, value: release },
    });
    return { ...view, button, onMove, onEnd, release };
}

beforeEach(() => {
    frames = new Map();
    let next = 0;
    bodyStyle = document.body.style.cssText;
    vi.spyOn(document, 'hidden', 'get').mockReturnValue(false);
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
        frames.set(++next, callback);
        return next;
    });
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation((id) => {
        frames.delete(id);
    });
});

afterEach(() => {
    cleanup();
    document.body.style.cssText = bodyStyle;
    vi.restoreAllMocks();
});

describe('usePointerDrag', () => {
    it('coalesces moves to the newest coordinate and ignores other pointers', () => {
        const { button, onMove } = mount();
        pointer(button, 'pointerdown');
        pointer(window, 'pointermove', 30);
        pointer(window, 'pointermove', 60);
        pointer(window, 'pointermove', 90, 2);
        expect(onMove).not.toHaveBeenCalled();
        expect(frames.size).toBe(1);
        frame();
        expect(onMove).toHaveBeenCalledTimes(1);
        expect(onMove.mock.calls[0][0].clientX).toBe(60);
    });

    it('flushes the release coordinate once before committing', () => {
        const { button, onMove, onEnd, release } = mount();
        pointer(button, 'pointerdown');
        pointer(window, 'pointermove', 30);
        pointer(window, 'pointerup', 70);
        expect(onMove).toHaveBeenCalledTimes(1);
        expect(onMove.mock.calls[0][0].clientX).toBe(70);
        expect(onEnd).toHaveBeenCalledTimes(1);
        expect(onEnd).toHaveBeenLastCalledWith('commit');
        expect(release).toHaveBeenCalledTimes(1);
        expect(release).toHaveBeenLastCalledWith(1);
        expect(frames.size).toBe(0);
        pointer(window, 'pointerup', 80);
        expect(onEnd).toHaveBeenCalledTimes(1);
    });

    it('does not resize a pure click but flushes a changed release without a move event', () => {
        const { button, onMove, onEnd } = mount();
        pointer(button, 'pointerdown');
        pointer(window, 'pointerup');
        expect(onMove).not.toHaveBeenCalled();
        expect(onEnd).toHaveBeenCalledTimes(1);
        expect(onEnd).toHaveBeenLastCalledWith('cancel');
        pointer(button, 'pointerdown');
        pointer(window, 'pointerup', 55);
        expect(onMove).toHaveBeenCalledTimes(1);
        expect(onMove.mock.calls[0][0].clientX).toBe(55);
        expect(onEnd).toHaveBeenLastCalledWith('commit');
    });

    it.each(['pointercancel', 'blur', 'hidden', 'lostpointercapture', 'resize'])(
        'rolls back on %s and restores existing global styles',
        (reason) => {
            document.body.style.setProperty('cursor', 'crosshair', 'important');
            document.body.style.setProperty('user-select', 'text');
            const { button, onMove, onEnd } = mount();
            pointer(button, 'pointerdown');
            pointer(window, 'pointermove', 30);
            if (reason === 'hidden') {
                vi.spyOn(document, 'hidden', 'get').mockReturnValue(true);
                fireEvent(document, new Event('visibilitychange'));
            } else if (reason === 'lostpointercapture') pointer(button, reason);
            else if (reason === 'pointercancel') pointer(window, reason);
            else fireEvent(window, new Event(reason));
            expect(onEnd).toHaveBeenCalledTimes(1);
            expect(onEnd).toHaveBeenLastCalledWith('cancel');
            expect(onMove).not.toHaveBeenCalled();
            expect(frames.size).toBe(0);
            expect(document.body.style.cursor).toBe('crosshair');
            expect(document.body.style.getPropertyPriority('cursor')).toBe('important');
            expect(document.body.style.userSelect).toBe('text');
            expect(button).toHaveAttribute('data-dragging', 'false');
            pointer(window, 'pointermove', 80);
            pointer(window, 'pointerup', 80);
            expect(onEnd).toHaveBeenCalledTimes(1);
        },
    );

    it('cancels frames and listeners when unmounted', () => {
        const { button, onMove, onEnd, unmount } = mount();
        pointer(button, 'pointerdown');
        pointer(window, 'pointermove', 30);
        unmount();
        expect(onEnd).toHaveBeenCalledTimes(1);
        expect(onEnd).toHaveBeenLastCalledWith('unmount');
        expect(frames.size).toBe(0);
        expect(document.body.style.cursor).toBe('');
        expect(document.body.style.userSelect).toBe('');
        pointer(window, 'pointermove', 80);
        pointer(window, 'pointerup', 80);
        frame();
        expect(onMove).not.toHaveBeenCalled();
        expect(onEnd).toHaveBeenCalledTimes(1);
    });
});
