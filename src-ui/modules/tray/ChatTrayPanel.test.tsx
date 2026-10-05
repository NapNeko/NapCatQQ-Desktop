import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { chatTrayService } from '../../core/services/chat-tray.service';
import type { ChatTrayPanelData } from '../../core/ipc/generated/chat/ChatTrayPanelData';
import { ChatTrayPanel } from './ChatTrayPanel';

let data: ChatTrayPanelData;
let changed: () => void;
beforeEach(async () => {
    data = (await chatTrayService.data())!;
    vi.spyOn(chatTrayService, 'data').mockImplementation(async () => structuredClone(data));
    vi.spyOn(chatTrayService, 'ready').mockResolvedValue();
    vi.spyOn(chatTrayService, 'hide').mockResolvedValue();
    vi.spyOn(chatTrayService, 'action').mockResolvedValue();
    vi.spyOn(chatTrayService, 'onChanged').mockImplementation(async cb => { changed = cb; return vi.fn(); });
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({ height: 280 } as DOMRect);
});
afterEach(cleanup);

describe('account tray panel', () => {
    it('renders themed actions with keyboard navigation and closes on Escape', async () => {
        render(<ChatTrayPanel />);
        const open = await screen.findByRole('menuitem', { name: /打开聊天/ });
        await waitFor(() => expect(open).toHaveFocus());
        fireEvent.keyDown(open, { key: 'ArrowDown' });
        expect(screen.getByRole('menuitem', { name: '打开控制台' })).toHaveFocus();
        fireEvent.keyDown(document.activeElement!, { key: 'End' });
        expect(screen.getByRole('menuitem', { name: '隐藏此账号托盘' })).toHaveFocus();
        fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
        expect(chatTrayService.hide).toHaveBeenCalledWith(data.generation);
    });
    it('binds background and hide actions to the displayed account generation', async () => {
        const user = userEvent.setup(); render(<ChatTrayPanel />);
        const background = await screen.findByRole('menuitemcheckbox', { name: '后台接收消息' });
        expect(background).toHaveAttribute('aria-checked', 'true');
        await user.click(background);
        expect(chatTrayService.action).toHaveBeenCalledWith(1, 'background', undefined);
        await waitFor(() => expect(background).toBeEnabled());
        await user.click(screen.getByRole('menuitem', { name: '隐藏此账号托盘' }));
        expect(chatTrayService.action).toHaveBeenCalledWith(1, 'hide', undefined);
    });
    it('only opens the selected conversation after clicking, without actions during hover preview', async () => {
        data.menu = false;
        render(<ChatTrayPanel />);
        const conversation = await screen.findByRole('button', { name: /林夕/ });
        expect(chatTrayService.action).not.toHaveBeenCalled();
        expect(conversation).not.toHaveFocus();
        await userEvent.click(conversation);
        expect(chatTrayService.action).toHaveBeenCalledWith(1, 'open', 'private:0');
    });
    it('ignores late responses after switching accounts and exposes action failures', async () => {
        render(<ChatTrayPanel />);
        await screen.findByRole('menu', { name: '账号操作' });
        let resolveOld!: (value: ChatTrayPanelData) => void;
        const old = structuredClone(data);
        vi.mocked(chatTrayService.data).mockImplementationOnce(() => new Promise(resolve => { resolveOld = resolve; }));
        act(() => changed());
        data.generation = 2; data.snapshot.account.target.name = '另一个账号';
        act(() => changed());
        await screen.findByText('另一个账号');
        await act(async () => resolveOld(old));
        expect(screen.getByText('另一个账号')).toBeInTheDocument();
        vi.mocked(chatTrayService.action).mockRejectedValueOnce(new Error('连接暂时不可用'));
        await userEvent.click(screen.getByRole('menuitem', { name: /打开聊天/ }));
        expect(await screen.findByRole('alert')).toHaveTextContent('连接暂时不可用');
        expect(chatTrayService.action).toHaveBeenCalledWith(2, 'open', undefined);
    });
});
