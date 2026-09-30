import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { preferencesStore } from '../../hooks/preferences/preferencesStore';
import { Dialog, DialogContent, DialogTitle } from '../../shared/ui';
import { matchDebugShortcut, useDebugShortcuts, type DebugShortcutHandlers } from './debugShortcuts';

const key = (k: string, mods: Partial<Record<'ctrlKey' | 'metaKey' | 'shiftKey' | 'altKey', boolean>> = {}) => ({
    key: k,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    altKey: false,
    ...mods,
});

describe('matchDebugShortcut', () => {
    it('认得调试台的几个键', () => {
        expect(matchDebugShortcut(key('k', { ctrlKey: true }))).toBe('palette');
        expect(matchDebugShortcut(key('K', { metaKey: true }))).toBe('palette');
        expect(matchDebugShortcut(key('Enter', { ctrlKey: true }))).toBe('send');
        expect(matchDebugShortcut(key('Escape'))).toBe('cancel');
        expect(matchDebugShortcut(key('w', { ctrlKey: true }))).toBe('closeTab');
        expect(matchDebugShortcut(key('Tab', { ctrlKey: true }))).toBe('nextTab');
        expect(matchDebugShortcut(key('Tab', { ctrlKey: true, shiftKey: true }))).toBe('prevTab');
        expect(matchDebugShortcut(key('T', { ctrlKey: true, shiftKey: true }))).toBe('reopenTab');
    });

    it('别的组合一概不认，尤其是终端的 Ctrl+`', () => {
        expect(matchDebugShortcut(key('`', { ctrlKey: true }))).toBeNull();
        expect(matchDebugShortcut(key('k'))).toBeNull();
        expect(matchDebugShortcut(key('Enter'))).toBeNull();
        expect(matchDebugShortcut(key('t', { ctrlKey: true }))).toBeNull();
        expect(matchDebugShortcut(key('k', { ctrlKey: true, altKey: true }))).toBeNull();
        expect(matchDebugShortcut(key('Tab'))).toBeNull();
        expect(matchDebugShortcut(key('Escape', { ctrlKey: true }))).toBeNull();
    });
});

function Harness({ handlers }: { handlers: DebugShortcutHandlers }) {
    useDebugShortcuts(handlers);
    return (
        <div>
            <input aria-label="普通输入框" />
            <div role="dialog">
                <input aria-label="对话框里的输入框" />
            </div>
            <div className="xterm">
                <textarea aria-label="终端" />
            </div>
        </div>
    );
}

afterEach(cleanup);

describe('useDebugShortcuts', () => {
    it('处理了就拦下默认行为', () => {
        const closeTab = vi.fn();
        render(<Harness handlers={{ closeTab }} />);
        const ev = fireEvent.keyDown(screen.getByLabelText('普通输入框'), { key: 'w', ctrlKey: true });
        expect(closeTab).toHaveBeenCalledTimes(1);
        expect(ev).toBe(false);
    });

    it('焦点在对话框、终端里时不抢', () => {
        const palette = vi.fn();
        render(<Harness handlers={{ palette }} />);
        fireEvent.keyDown(screen.getByLabelText('对话框里的输入框'), { key: 'k', ctrlKey: true });
        fireEvent.keyDown(screen.getByLabelText('终端'), { key: 'k', ctrlKey: true });
        expect(palette).not.toHaveBeenCalled();
    });

    it('别处已经处理过的键不重复处理；返回 false 时不拦默认行为', () => {
        const send = vi.fn();
        const cancel = vi.fn(() => false);
        render(<Harness handlers={{ send, cancel }} />);
        const input = screen.getByLabelText('普通输入框');
        input.addEventListener('keydown', (e) => e.preventDefault(), { once: true });
        fireEvent.keyDown(input, { key: 'Enter', ctrlKey: true });
        expect(send).not.toHaveBeenCalled();

        const ev = fireEvent.keyDown(input, { key: 'Escape' });
        expect(cancel).toHaveBeenCalledTimes(1);
        expect(ev).toBe(true);
    });

    it('按住不放时发送只触发一次', () => {
        const send = vi.fn();
        render(<Harness handlers={{ send }} />);
        const input = screen.getByLabelText('普通输入框');
        fireEvent.keyDown(input, { key: 'Enter', ctrlKey: true });
        fireEvent.keyDown(input, { key: 'Enter', ctrlKey: true, repeat: true });
        expect(send).toHaveBeenCalledTimes(1);
    });

    it('共享 Dialog 开着、焦点在里面时，关标签 / 切标签都不生效', async () => {
        preferencesStore.setMotionEnabled(false);
        const closeTab = vi.fn();
        const nextTab = vi.fn();
        function WithDialog() {
            useDebugShortcuts({ closeTab, nextTab });
            return (
                <Dialog open onOpenChange={() => {}}>
                    <DialogContent>
                        <DialogTitle>收藏请求</DialogTitle>
                        <input aria-label="名字" />
                    </DialogContent>
                </Dialog>
            );
        }
        render(<WithDialog />);
        const input = await screen.findByRole('textbox', { name: '名字' });
        fireEvent.keyDown(input, { key: 'w', ctrlKey: true });
        fireEvent.keyDown(input, { key: 'Tab', ctrlKey: true });
        expect(closeTab).not.toHaveBeenCalled();
        expect(nextTab).not.toHaveBeenCalled();
        preferencesStore.reset();
    });
});
