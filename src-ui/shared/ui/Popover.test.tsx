import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { Popover, PopoverClose, PopoverContent, PopoverTrigger } from './Popover';

describe('Popover focus lifecycle', () => {
    it('finishes the entrance when portal content mounts after its parent', async () => {
        render(
            <Popover defaultOpen>
                <PopoverTrigger>打开</PopoverTrigger>
                <PopoverContent aria-label="资料">
                    <PopoverClose>完成</PopoverClose>
                </PopoverContent>
            </Popover>,
        );
        const content = await screen.findByRole('dialog', { name: '资料' });
        await waitFor(() => expect(content).toHaveStyle({ opacity: '1' }));
    });
    it('reruns caller autofocus when reopened during its exit animation', async () => {
        const autofocus = vi.fn((event: Event) => {
            event.preventDefault();
            document.querySelector<HTMLButtonElement>('[data-autofocus]')?.focus();
        });
        render(
            <Popover>
                <PopoverTrigger>打开</PopoverTrigger>
                <PopoverContent onOpenAutoFocus={autofocus}>
                    <PopoverClose data-autofocus>完成</PopoverClose>
                </PopoverContent>
            </Popover>,
        );
        const trigger = screen.getByRole('button', { name: '打开' });
        trigger.focus();
        fireEvent.click(trigger);
        await waitFor(() => expect(autofocus).toHaveBeenCalledTimes(1));
        fireEvent.click(screen.getByRole('button', { name: '完成' }));
        trigger.focus();
        fireEvent.click(trigger);
        await waitFor(() => expect(autofocus).toHaveBeenCalledTimes(2));
        await waitFor(() => expect(screen.getByRole('button', { name: '完成' })).toHaveFocus());
        await waitFor(() => expect(screen.getByRole('dialog')).toHaveStyle({ opacity: '1' }));
    });
    it('focuses content again when reopened after the exit animation', async () => {
        render(
            <Popover>
                <PopoverTrigger>打开</PopoverTrigger>
                <PopoverContent aria-label="选项">
                    <PopoverClose>完成</PopoverClose>
                </PopoverContent>
            </Popover>,
        );
        const trigger = screen.getByRole('button', { name: '打开' });
        trigger.focus();
        fireEvent.click(trigger);
        await waitFor(() => expect(screen.getByRole('button', { name: '完成' })).toHaveFocus());
        fireEvent.click(screen.getByRole('button', { name: '完成' }));
        await waitFor(() =>
            expect(screen.queryByRole('button', { name: '完成' })).not.toBeInTheDocument(),
        );
        trigger.focus();
        fireEvent.click(trigger);
        await waitFor(() => expect(screen.getByRole('button', { name: '完成' })).toHaveFocus());
    });
});
