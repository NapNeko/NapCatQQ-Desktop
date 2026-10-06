import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { SystemQqWarningDialog } from './SystemQqWarningDialog';

function setup() {
    const onContinue = vi.fn();
    const onInstall = vi.fn();
    const onCancel = vi.fn();
    render(
        <SystemQqWarningDialog
            open
            onContinue={onContinue}
            onInstall={onInstall}
            onCancel={onCancel}
        />,
    );
    return { onContinue, onInstall, onCancel };
}

describe('SystemQqWarningDialog', () => {
    it('说清共用 QQ 的后果与侧装的去处', () => {
        setup();
        expect(screen.getByText('这个 Bot 会用你自己装的 QQ')).toBeTruthy();
        expect(screen.getByText(/共用同一份程序/)).toBeTruthy();
        expect(screen.getByText(/只解压到桌面端数据目录/)).toBeTruthy();
    });

    it('勾选「不再提醒」后把标记带给调用方', () => {
        const { onContinue } = setup();
        fireEvent.click(screen.getByRole('checkbox'));
        fireEvent.click(screen.getByRole('button', { name: '继续启动' }));
        expect(onContinue).toHaveBeenCalledWith(true);
    });

    it('没勾选时两个出口都不带永久忽略', () => {
        const { onContinue, onInstall } = setup();
        fireEvent.click(screen.getByRole('button', { name: '继续启动' }));
        expect(onContinue).toHaveBeenCalledWith(false);
        fireEvent.click(screen.getByRole('button', { name: '去装一份' }));
        expect(onInstall).toHaveBeenCalledWith(false);
    });
});
