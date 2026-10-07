import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ChatSelectionBar } from './ChatSelectionBar';

describe('chat selection panel', () => {
    it('keeps cancellation available with an empty selection and enables forwarding once messages are chosen', async () => {
        const onCancel = vi.fn();
        const onForward = vi.fn();
        const view = render(
            <ChatSelectionBar
                visible
                count={0}
                disabled={false}
                onCancel={onCancel}
                onForward={onForward}
            />,
        );
        await screen.findByRole('toolbar', { name: '消息多选' });
        expect(screen.getByRole('button', { name: '转发' })).toBeDisabled();
        fireEvent.click(screen.getByRole('button', { name: '取消' }));
        expect(onCancel).toHaveBeenCalledOnce();
        view.rerender(
            <ChatSelectionBar
                visible
                count={2}
                disabled={false}
                onCancel={onCancel}
                onForward={onForward}
            />,
        );
        fireEvent.click(screen.getByRole('button', { name: '转发' }));
        expect(onForward).toHaveBeenCalledOnce();
    });
});
