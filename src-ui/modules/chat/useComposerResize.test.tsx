// @vitest-environment jsdom
import { useRef } from 'react';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useComposerResize } from './useComposerResize';

const preferences = vi.hoisted(() => ({ height: 150 as number | null, save: vi.fn() }));
vi.mock('./chatPreferences', () => ({
    useChatPreferences: () => ({ composerHeight: preferences.height }),
    setChatPreferences: preferences.save,
}));

let frames: Map<number, FrameRequestCallback>;
const rendered = vi.fn();

function Harness({ collapsed = false }: { collapsed?: boolean }) {
    rendered();
    const input = useRef<HTMLTextAreaElement>(null);
    const composer = useRef<HTMLDivElement>(null);
    const { handle } = useComposerResize(input, composer, '草稿', collapsed);
    return (
        <section>
            <div ref={composer}>
                <button {...handle} />
                <textarea ref={input} aria-label="草稿" defaultValue="草稿" />
            </div>
        </section>
    );
}

function pointer(target: Element | Window | Document, type: string, clientY: number) {
    const event = new MouseEvent(type, { bubbles: true, button: 0, clientY });
    Object.defineProperties(event, {
        pointerId: { value: 1 },
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

beforeEach(() => {
    frames = new Map();
    let next = 0;
    preferences.height = 150;
    preferences.save.mockClear();
    rendered.mockClear();
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
        frames.set(++next, callback);
        return next;
    });
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation((id) => {
        frames.delete(id);
    });
    vi.spyOn(Element.prototype, 'clientHeight', 'get').mockReturnValue(800);
    vi.spyOn(Element.prototype, 'scrollHeight', 'get').mockReturnValue(100);
});

afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
});

describe('useComposerResize', () => {
    it('previews height without rerendering the composer and saves the release coordinate once', () => {
        render(<Harness />);
        const input = screen.getByRole('textbox');
        const handle = screen.getByRole('separator');
        expect(input.style.height).toBe('150px');
        pointer(handle, 'pointerdown', 400);
        const rendersBeforeMove = rendered.mock.calls.length;
        pointer(window, 'pointermove', 370);
        pointer(window, 'pointermove', 340);
        frame();
        expect(input.style.height).toBe('210px');
        expect(handle).toHaveAttribute('aria-valuenow', '210');
        expect(rendered).toHaveBeenCalledTimes(rendersBeforeMove);
        expect(preferences.save).not.toHaveBeenCalled();
        pointer(window, 'pointerup', 300);
        expect(input.style.height).toBe('250px');
        expect(preferences.save).toHaveBeenCalledTimes(1);
        expect(preferences.save).toHaveBeenLastCalledWith({ composerHeight: 250 });
        expect(input).toHaveValue('草稿');
    });

    it('restores automatic height on cancellation without changing the preference', () => {
        preferences.height = null;
        render(<Harness />);
        const input = screen.getByRole('textbox');
        const handle = screen.getByRole('separator');
        expect(input.style.height).toBe('100px');
        pointer(handle, 'pointerdown', 400);
        pointer(window, 'pointermove', 300);
        frame();
        expect(input.style.height).toBe('200px');
        pointer(window, 'pointercancel', 300);
        expect(input.style.height).toBe('100px');
        expect(handle).toHaveAttribute('aria-valuenow', '100');
        expect(preferences.save).not.toHaveBeenCalled();
    });

    it('keeps automatic sizing after clicking the resize handle without moving', () => {
        preferences.height = null;
        render(<Harness />);
        const input = screen.getByRole('textbox');
        pointer(screen.getByRole('separator'), 'pointerdown', 400);
        pointer(window, 'pointerup', 400);
        expect(input.style.height).toBe('100px');
        expect(preferences.height).toBeNull();
        expect(preferences.save).not.toHaveBeenCalled();
    });

    it('rolls back an active drag when the composer collapses', () => {
        const { rerender } = render(<Harness />);
        const input = screen.getByRole('textbox');
        pointer(screen.getByRole('separator'), 'pointerdown', 400);
        pointer(window, 'pointermove', 350);
        frame();
        rerender(<Harness collapsed />);
        expect(input.style.height).toBe('150px');
        pointer(window, 'pointerup', 300);
        expect(preferences.save).not.toHaveBeenCalled();
    });

    it('retains keyboard height limits and double click automatic sizing', () => {
        render(<Harness />);
        const handle = screen.getByRole('separator');
        fireEvent.keyDown(handle, { key: 'End' });
        expect(screen.getByRole('textbox').style.height).toBe('420px');
        expect(preferences.save).toHaveBeenLastCalledWith({ composerHeight: 420 });
        fireEvent.doubleClick(handle);
        expect(screen.getByRole('textbox').style.height).toBe('100px');
        expect(preferences.save).toHaveBeenLastCalledWith({ composerHeight: null });
    });
});
