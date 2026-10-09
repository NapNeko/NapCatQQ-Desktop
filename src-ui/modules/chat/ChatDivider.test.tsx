// @vitest-environment jsdom
import type { CSSProperties } from 'react';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ChatDivider } from './ChatDivider';

let frames: Map<number, FrameRequestCallback>;

function pointer(target: Element | Window | Document, type: string, clientX: number) {
    const event = new MouseEvent(type, { bubbles: true, button: 0, clientX });
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

function mount() {
    const onResize = vi.fn();
    const onCommit = vi.fn();
    const view = render(
        <div style={{ '--chat-list-width': '260px' } as CSSProperties}>
            <aside>
                <ChatDivider width={260} onResize={onResize} onCommit={onCommit} />
            </aside>
        </div>,
    );
    const separator = screen.getByRole('separator');
    const workspace = separator.parentElement!.parentElement!;
    return { ...view, separator, workspace, onResize, onCommit };
}

beforeEach(() => {
    frames = new Map();
    let next = 0;
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
        frames.set(++next, callback);
        return next;
    });
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation((id) => {
        frames.delete(id);
    });
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockReturnValue(
        new DOMRect(0, 0, 260, 600),
    );
    vi.spyOn(Element.prototype, 'clientWidth', 'get').mockReturnValue(1000);
});

afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
});

describe('ChatDivider', () => {
    it('previews the latest width without publishing workspace state until release', () => {
        const { separator, workspace, onResize, onCommit } = mount();
        pointer(separator, 'pointerdown', 260);
        pointer(window, 'pointermove', 285);
        pointer(window, 'pointermove', 310);
        expect(workspace.style.getPropertyValue('--chat-list-width')).toBe('260px');
        frame();
        expect(workspace.style.getPropertyValue('--chat-list-width')).toBe('310px');
        expect(separator).toHaveAttribute('aria-valuenow', '310');
        expect(onResize).not.toHaveBeenCalled();
        expect(onCommit).not.toHaveBeenCalled();
        pointer(window, 'pointerup', 335);
        expect(workspace.style.getPropertyValue('--chat-list-width')).toBe('335px');
        expect(onResize).toHaveBeenCalledTimes(1);
        expect(onResize).toHaveBeenLastCalledWith(335);
        expect(onCommit).toHaveBeenCalledTimes(1);
        expect(onCommit).toHaveBeenLastCalledWith(335);
    });

    it('rolls back preview and aria on cancellation without committing', () => {
        const { separator, workspace, onResize, onCommit } = mount();
        pointer(separator, 'pointerdown', 260);
        pointer(window, 'pointermove', 1000);
        frame();
        expect(workspace.style.getPropertyValue('--chat-list-width')).toBe('380px');
        pointer(window, 'pointercancel', 1000);
        expect(workspace.style.getPropertyValue('--chat-list-width')).toBe('260px');
        expect(separator).toHaveAttribute('aria-valuenow', '260');
        expect(onResize).not.toHaveBeenCalled();
        expect(onCommit).not.toHaveBeenCalled();
    });

    it('retains keyboard resizing and double click reset', () => {
        const { separator, onResize, onCommit } = mount();
        fireEvent.keyDown(separator, { key: 'ArrowRight' });
        expect(onResize).toHaveBeenLastCalledWith(270);
        expect(onCommit).toHaveBeenLastCalledWith(270);
        fireEvent.doubleClick(separator);
        expect(onResize).toHaveBeenLastCalledWith(260);
        expect(onCommit).toHaveBeenLastCalledWith(260);
    });
});
