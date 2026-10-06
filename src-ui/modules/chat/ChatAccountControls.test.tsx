import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { preferencesStore } from '../../hooks/preferences/preferencesStore';
import { chatDesktopService } from '../../core/services/chat-desktop.service';
import type { ChatAccountPreference } from '../../core/ipc/generated/chat/ChatAccountPreference';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import { ChatAccountControls } from './ChatAccountControls';

const clients: QueryClient[] = [];
afterEach(() => {
    cleanup();
    clients.splice(0).forEach((client) => client.clear());
    preferencesStore.reset();
});
async function setup() {
    preferencesStore.setMotionEnabled(false);
    const target: DebugTarget = {
        bot_id: 'settings-test',
        qq_id: 99,
        name: '测试账号',
        backend: 'snowluma',
        host: { kind: 'local' },
        running: true,
        online: true,
    };
    let preference: ChatAccountPreference = {
        botId: target.bot_id,
        selfId: '99',
        enabled: true,
        background: true,
        tray: true,
        trayNotification: 'badge',
        ignoredGroups: ['123'],
        hiddenGroups: ['456'],
        notifyUnknownGroups: false,
    };
    vi.spyOn(chatDesktopService, 'status').mockImplementation(async () => ({
        v: 1,
        accounts: [
            {
                target,
                preference,
                unread: 0,
                notificationUnread: 0,
                groups: [],
                connection: { state: 'connected' },
                error: null,
            },
        ],
    }));
    const save = vi.spyOn(chatDesktopService, 'setPreference').mockImplementation(async (value) => {
        preference = value;
    });
    const unmute = vi
        .spyOn(chatDesktopService, 'ignoreGroup')
        .mockImplementation(async (_bot, _self, id) => {
            preference = {
                ...preference,
                ignoredGroups: preference.ignoredGroups.filter((group) => group !== id),
            };
        });
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    clients.push(client);
    const reconnect = vi.fn();
    render(
        <QueryClientProvider client={client}>
            <ChatAccountControls
                target={target}
                connectionLabel="已连接"
                onReconnect={reconnect}
                contacts={[{ type: 'group', key: 'group:123', id: '123', name: '讨论群' }]}
            />
        </QueryClientProvider>,
    );
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: '聊天设置' }));
    await waitFor(() => expect(screen.getByRole('switch', { name: '用于聊天' })).toBeChecked());
    return { user, save, unmute, reconnect };
}
describe('chat settings organization', () => {
    it('shows account, notification and window sections on one page and preserves group preferences when changing tray mode', async () => {
        const { user, save } = await setup();
        expect(screen.getByRole('radio', { name: '红点' })).toHaveAttribute('aria-checked', 'true');
        expect(screen.getByRole('button', { name: '重新连接' })).toBeInTheDocument();
        expect(
            screen.getByRole('button', { name: /嵌回主窗口|在独立窗口打开/ }),
        ).toBeInTheDocument();
        await user.click(screen.getByRole('radio', { name: '头像闪烁' }));
        await waitFor(() =>
            expect(save).toHaveBeenCalledWith(
                expect.objectContaining({
                    trayNotification: 'flash',
                    hiddenGroups: ['456'],
                    ignoredGroups: ['123'],
                }),
            ),
        );
    });
    it('hides tray mode choices when the tray icon is turned off', async () => {
        const { user, save } = await setup();
        await user.click(screen.getByRole('switch', { name: '托盘图标' }));
        await waitFor(() =>
            expect(save).toHaveBeenCalledWith(expect.objectContaining({ tray: false })),
        );
        await waitFor(() =>
            expect(screen.queryByRole('radio', { name: '红点' })).not.toBeInTheDocument(),
        );
    });
    it('restores reminders through the group list without unhiding other groups', async () => {
        const { user, unmute } = await setup();
        await user.click(screen.getByText('本地免打扰群聊'));
        expect(screen.getByText('讨论群')).toBeInTheDocument();
        await user.click(screen.getByRole('button', { name: '恢复提醒' }));
        await waitFor(() =>
            expect(unmute).toHaveBeenCalledWith('settings-test', '99', '123', false),
        );
        await waitFor(() => expect(screen.queryByText('本地免打扰群聊')).not.toBeInTheDocument());
    });
    it('turns off dependent features when the account is disabled and keeps reconnect alongside the status', async () => {
        const { user, save, reconnect } = await setup();
        await user.click(screen.getByRole('button', { name: '重新连接' }));
        expect(reconnect).toHaveBeenCalledOnce();
        await waitFor(() => expect(screen.getByRole('switch', { name: '用于聊天' })).toBeEnabled());
        await user.click(screen.getByRole('switch', { name: '用于聊天' }));
        await waitFor(() =>
            expect(save).toHaveBeenCalledWith(
                expect.objectContaining({ enabled: false, background: false, tray: false }),
            ),
        );
    });
});
