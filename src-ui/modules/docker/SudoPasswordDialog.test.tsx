// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { SudoPasswordDialog } from './SudoPasswordDialog';

afterEach(cleanup);

describe('sudo password dialog', () => {
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
