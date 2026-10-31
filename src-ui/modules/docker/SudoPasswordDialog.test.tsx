// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { SudoPasswordDialog } from './SudoPasswordDialog';

afterEach(cleanup);

describe('sudo password dialog', () => {
    it('keeps local sudo passwords limited to the current operation', async () => {
        const onConfirm = vi.fn();
        render(
            <SudoPasswordDialog
                hostName="本机"
                actionLabel="修复 QQ 依赖"
                isSubmitting={false}
                allowRemember={false}
                onConfirm={onConfirm}
                onClose={vi.fn()}
            />,
        );
        expect(screen.queryByRole('checkbox')).not.toBeInTheDocument();
        fireEvent.change(screen.getByLabelText('sudo 密码'), { target: { value: 'password' } });
        await act(async () => {
            fireEvent.click(screen.getByRole('button', { name: '修复 QQ 依赖' }));
        });
        expect(onConfirm).toHaveBeenCalledWith('password', false);
    });

    it('shows the QQ action and prevents duplicate confirmation while validation is pending', async () => {
        let resolve!: () => void;
        const onConfirm = vi.fn(
            () =>
                new Promise<void>((done) => {
                    resolve = done;
                }),
        );
        render(
            <SudoPasswordDialog
                hostName="远端"
                actionLabel="更新 QQ"
                isSubmitting={false}
                onConfirm={onConfirm}
                onClose={vi.fn()}
            />,
        );
        fireEvent.change(screen.getByLabelText('sudo 密码'), { target: { value: 'password' } });
        const button = screen.getByRole('button', { name: '更新 QQ' });
        fireEvent.click(button);
        fireEvent.click(button);
        expect(onConfirm).toHaveBeenCalledOnce();
        expect(onConfirm).toHaveBeenCalledWith('password', false);
        expect((button as HTMLButtonElement).disabled).toBe(true);
        await act(async () => resolve());
    });
});
