import type { ReactNode } from 'react';
import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ChatPopoutApp } from './ChatPopoutApp';

const bridge = vi.hoisted(() => ({
    onRequest: vi.fn(),
    reply: vi.fn(),
    reveal: vi.fn(),
    showMainWindow: vi.fn(),
    notice: vi.fn(),
}));
vi.mock('../hooks/desktop/useChatPopoutBridge', () => ({
    useChatPopoutBridge: () => bridge,
}));
vi.mock('../hooks/chat/chatStore', () => ({
    prepareChatHandoff: vi.fn(async () => {}),
    getChatSelectedBot: () => 'bot',
}));
vi.mock('../hooks/chat/useChatNotice', () => ({ useChatNotice: bridge.notice }));
vi.mock('../hooks/ui/useGlobalInfoBars', () => ({
    useGlobalInfoBars: () => ({ bars: [], dismiss: vi.fn(), remove: vi.fn() }),
}));
vi.mock('../shared/components/next/TitleBarChrome', () => ({ TitleBarChrome: () => null }));
vi.mock('../shared/ui/InfoBarStack', () => ({ InfoBarStack: () => null }));
vi.mock('../shared/ui/Tooltip', () => ({
    TooltipProvider: ({ children }: { children: ReactNode }) => children,
}));
vi.mock('../shared/ui/RouteErrorBoundary', () => ({
    RouteErrorBoundary: ({ children }: { children: ReactNode }) => children,
}));
vi.mock('../modules/chat/ChatPage', () => ({ ChatPage: () => <div>聊天工作区</div> }));

beforeEach(() => {
    vi.clearAllMocks();
    bridge.reveal.mockResolvedValue(undefined);
});
afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
});

describe('chat popout startup', () => {
    it('reveals only after its handoff listener is ready, even without animation frames', async () => {
        const frame = vi.spyOn(window, 'requestAnimationFrame').mockImplementation(() => 1);
        let ready!: (unlisten: () => void) => void;
        bridge.onRequest.mockReturnValueOnce(
            new Promise<() => void>((resolve) => {
                ready = resolve;
            }),
        );
        render(<ChatPopoutApp />);
        await screen.findByText('聊天工作区');
        expect(bridge.reveal).not.toHaveBeenCalled();
        await act(async () => {
            ready(vi.fn());
        });
        expect(bridge.reveal).toHaveBeenCalledOnce();
        expect(frame).not.toHaveBeenCalled();
    });

    it('cleans up a late listener without revealing a departed popout', async () => {
        let ready!: (unlisten: () => void) => void;
        bridge.onRequest.mockReturnValueOnce(
            new Promise<() => void>((resolve) => {
                ready = resolve;
            }),
        );
        const app = render(<ChatPopoutApp />);
        app.unmount();
        const stop = vi.fn();
        await act(async () => {
            ready(stop);
        });
        expect(stop).toHaveBeenCalledOnce();
        expect(bridge.reveal).not.toHaveBeenCalled();
    });

    it('reports a failed handoff listener before making the window interactive', async () => {
        bridge.onRequest.mockRejectedValueOnce(new Error('监听失败'));
        await act(async () => {
            render(<ChatPopoutApp />);
        });
        expect(bridge.reveal).not.toHaveBeenCalled();
        expect(bridge.notice).toHaveBeenCalledWith(
            'window:handoff',
            '聊天窗口切换失败',
            '监听失败',
        );
    });
});
