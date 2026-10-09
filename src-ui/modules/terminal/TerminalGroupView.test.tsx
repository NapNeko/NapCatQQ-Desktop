// @vitest-environment jsdom
import { act, cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { TerminalGroup } from '../../hooks/terminal/terminalStore';
import { TerminalGroupView } from './TerminalGroupView';

const ratio = vi.hoisted(() => vi.fn());
vi.mock('../../hooks/terminal/terminalStore', () => ({ terminalStore: { setRatio: ratio } }));
vi.mock('./TerminalPane', () => ({ TerminalPane: () => <span>终端</span> }));

const group: TerminalGroup = {
    id: 'split-test',
    panes: ['first', 'second'],
    focused: 'first',
    split: 'row',
    ratio: 0.5,
};
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
    const view = render(<TerminalGroupView group={group} visible drop={null} />);
    const divider = view.container.querySelector('.ncd-term-divider')!;
    const pane = view.container.querySelector('.ncd-term-group')!.firstElementChild as HTMLElement;
    return { ...view, divider, pane };
}

beforeEach(() => {
    frames = new Map();
    let next = 0;
    ratio.mockClear();
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
        frames.set(++next, callback);
        return next;
    });
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation((id) => {
        frames.delete(id);
    });
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockReturnValue(
        new DOMRect(0, 0, 1000, 500),
    );
});

afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
});

describe('TerminalGroupView resizing', () => {
    it('previews split sizes with no store writes and commits the final release once', () => {
        const { divider, pane } = mount();
        pointer(divider, 'pointerdown', 500);
        pointer(window, 'pointermove', 600);
        pointer(window, 'pointermove', 700);
        expect(pane.style.flexBasis).toBe('50%');
        frame();
        expect(pane.style.flexBasis).toBe('70%');
        expect(ratio).not.toHaveBeenCalled();
        pointer(window, 'pointerup', 750);
        expect(pane.style.flexBasis).toBe('75%');
        expect(ratio).toHaveBeenCalledTimes(1);
        expect(ratio).toHaveBeenLastCalledWith('split-test', 0.75);
    });

    it('keeps a pure click from changing or persisting the ratio', () => {
        const { divider, pane } = mount();
        pointer(divider, 'pointerdown', 501);
        pointer(window, 'pointerup', 501);
        expect(pane.style.flexBasis).toBe('50%');
        expect(ratio).not.toHaveBeenCalled();
    });

    it('restores the split after cancellation and keeps the existing ratio', () => {
        const { divider, pane } = mount();
        pointer(divider, 'pointerdown', 500);
        pointer(window, 'pointermove', 950);
        frame();
        expect(pane.style.flexBasis).toBe('85%');
        pointer(window, 'pointercancel', 950);
        expect(pane.style.flexBasis).toBe('50%');
        expect(ratio).not.toHaveBeenCalled();
    });

    it.each(['split', 'hidden'])('cancels the active drag when the layout becomes %s', (change) => {
        const { divider, pane, rerender } = mount();
        pointer(divider, 'pointerdown', 500);
        pointer(window, 'pointermove', 700);
        frame();
        rerender(
            <TerminalGroupView
                group={change === 'split' ? { ...group, split: 'column' } : group}
                visible={change !== 'hidden'}
                drop={null}
            />,
        );
        expect(pane.style.flexBasis).toBe('50%');
        pointer(window, 'pointerup', 800);
        expect(ratio).not.toHaveBeenCalled();
    });
});
