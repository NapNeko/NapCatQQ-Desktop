import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { chatDesktopService } from '../../core/services/chat-desktop.service';
import type { ChatDesktopStatus } from '../../core/ipc/generated/chat/ChatDesktopStatus';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import { useChatNotifications } from './useChatNotifications';

describe('chat notification hydration', () => {
    it('keeps the chat usable when the frontend reloads before the native process is rebuilt', async () => {
        const target: DebugTarget = { bot_id: '99', qq_id: 99, name: '测试', backend: 'snowluma', host: { kind: 'local' }, running: false, online: false };
        const old = { v: 1, accounts: [{ target, preference: { botId: '99', selfId: '99', enabled: false, background: false, tray: false }, unread: 2, connection: { state: 'stopped', reason: '预览' }, error: null }] };
        vi.spyOn(chatDesktopService, 'status').mockResolvedValue(old as ChatDesktopStatus);
        const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
        const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
        const hook = renderHook(() => useChatNotifications(target, []), { wrapper });
        await waitFor(() => expect(client.getQueryData(['chat', 'desktop'])).toEqual(old));
        expect(hook.result.current.hidden.size).toBe(0);
        expect(hook.result.current.ignored.size).toBe(0);
        expect(hook.result.current.qqMuted.size).toBe(0);
        hook.unmount(); client.clear();
    });
});
