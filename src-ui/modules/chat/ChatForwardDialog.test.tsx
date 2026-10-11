import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { ChatAccountStore } from '../../hooks/chat/chatStore';
import { emptyAccount, ingestMessage, type Contact } from '../../core/domain/chat/model';
import { ChatForwardError } from '../../core/domain/chat/messageTransfer';
import { ChatForwardDialog } from './ChatForwardDialog';

vi.mock('@tanstack/react-virtual', async (importOriginal) => ({
    ...(await importOriginal<typeof import('@tanstack/react-virtual')>()),
    useVirtualizer: (options: { count: number; getItemKey: (index: number) => string }) => ({
        getTotalSize: () => options.count * 68,
        getVirtualItems: () =>
            Array.from({ length: options.count }, (_, index) => ({
                index,
                start: index * 68,
                key: options.getItemKey(index),
            })),
        scrollToIndex: vi.fn(),
    }),
}));

const recipient: Contact = { key: 'private:2', type: 'private', id: '2', name: '小红' };

function setup() {
    const store = new ChatAccountStore(
        {
            bot_id: 'forward-dialog',
            name: '测试',
            qq_id: 99,
            backend: 'napcat',
            host: { kind: 'local' },
            running: true,
            online: true,
        },
        { call: vi.fn(), subscribe: vi.fn(), unsubscribe: vi.fn() },
    );
    const snapshot = store.getSnapshot();
    snapshot.connection = { state: 'connected' };
    snapshot.contacts = [recipient];
    snapshot.account = ingestMessage(emptyAccount('99'), {
        self_id: 99,
        user_id: 1,
        message_type: 'private',
        message_id: 8,
        sender: { nickname: '小明' },
        message: '待转发的内容',
    });
    const messageKeys = snapshot.account.messages.map((message) => message.key);
    const forward = vi.spyOn(store, 'forward').mockResolvedValue(undefined);
    const onSent = vi.fn();
    const onClose = vi.fn();
    render(
        <ChatForwardDialog
            store={store}
            messageKeys={messageKeys}
            onClose={onClose}
            onSent={onSent}
        />,
    );
    return { store, messageKeys, forward, onSent, onClose };
}

describe('chat forwarding dialog', () => {
    it('supports keyboard mode selection and forwards with the selected recipient and mode', async () => {
        const user = userEvent.setup();
        const { messageKeys, forward, onSent } = setup();
        const merged = await screen.findByRole('radio', { name: '合并转发' });
        const individual = screen.getByRole('radio', { name: '逐条转发' });
        expect(merged).toBeChecked();
        await waitFor(() => expect(merged).toHaveFocus());
        await user.click(merged);
        await user.keyboard('{ArrowRight}');
        expect(individual).toBeChecked();
        await waitFor(() => expect(individual).toHaveFocus());
        expect(
            within(screen.getByRole('region', { name: '待转发消息' })).getByText('逐条消息'),
        ).toBeInTheDocument();
        await user.click(screen.getByRole('option', { name: /小红/ }));
        await user.click(screen.getByRole('button', { name: '转发', exact: true }));
        expect(forward).toHaveBeenCalledWith(
            messageKeys,
            recipient,
            'individual',
            expect.any(Function),
        );
        await waitFor(() => expect(onSent).toHaveBeenCalledOnce());
    });

    it('locks mode and recipients while sending and keeps an uncertain result from being resent', async () => {
        const user = userEvent.setup();
        const { forward, onClose, onSent } = setup();
        let rejectForward!: (failure: Error) => void;
        forward.mockImplementation(
            () =>
                new Promise<void>((_resolve, reject) => {
                    rejectForward = reject;
                }),
        );
        await screen.findByRole('radio', { name: '合并转发' });
        const target = screen.getByRole('option', { name: /小红/ });
        await user.click(target);
        await user.click(screen.getByRole('button', { name: '转发', exact: true }));
        expect(screen.getByRole('radio', { name: '逐条转发' })).toBeDisabled();
        expect(screen.getByRole('combobox', { name: '搜索转发目标' })).toBeDisabled();
        expect(target).toBeDisabled();
        expect(screen.getByRole('button', { name: '取消' })).toBeDisabled();
        await act(async () => {
            rejectForward(new ChatForwardError('转发结果未确认', true));
        });
        expect(screen.getByRole('button', { name: '结果待确认' })).toBeDisabled();
        expect(screen.getByRole('radio', { name: '合并转发' })).toBeDisabled();
        expect(target).toBeDisabled();
        expect(onSent).not.toHaveBeenCalled();
        expect(forward).toHaveBeenCalledOnce();
        await user.click(screen.getByRole('button', { name: '取消' }));
        expect(onClose).toHaveBeenCalledOnce();
    });
});
