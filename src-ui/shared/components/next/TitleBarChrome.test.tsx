import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TitleBarChrome } from './TitleBarChrome';

const controls = vi.hoisted(() => ({
    isMaximized: false,
    minimize: vi.fn(),
    toggleMaximize: vi.fn(),
    close: vi.fn(),
}));
const closeSelf = vi.hoisted(() => vi.fn());
vi.mock('../../../hooks/desktop/useWindowControls', () => ({ useWindowControls: () => controls }));
vi.mock('../../../core/services/desktop.service', () => ({ windowControlService: { closeSelf } }));

describe('title bar window ownership', () => {
    beforeEach(() => vi.clearAllMocks());

    it('closes the tool window through its own native close path', () => {
        render(<TitleBarChrome tool />);
        fireEvent.click(screen.getByRole('button', { name: '关闭' }));
        expect(closeSelf).toHaveBeenCalledOnce();
        expect(controls.close).not.toHaveBeenCalled();
    });

    it('keeps the main window close policy', () => {
        render(<TitleBarChrome />);
        fireEvent.click(screen.getByRole('button', { name: '关闭' }));
        expect(controls.close).toHaveBeenCalledOnce();
        expect(closeSelf).not.toHaveBeenCalled();
    });
});
