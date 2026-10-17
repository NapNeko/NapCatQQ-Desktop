import type { ReactNode } from 'react';
import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ChatPopoutApp } from './ChatPopoutApp';
import type { ChatWindowRequest } from '../core/ipc/generated/chat/ChatWindowRequest';

const bridge = vi.hoisted(() => ({
    onRequest: vi.fn(),
    reply: vi.fn(),
    reveal: vi.fn(),
    showMainWindow: vi.fn(),
    notice: vi.fn(),
    prepare: vi.fn(),
}));
vi.mock('../hooks/desktop/useChatPopoutBridge', () => ({
    useChatPopoutBridge: () => bridge,
}));
vi.mock('../hooks/chat/chatStore', () => ({
    prepareChatHandoff: bridge.prepare,
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
    bridge.prepare.mockReset().mockResolvedValue(undefined);
    bridge.reply.mockResolvedValue(undefined);
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

    it('waits for old draft cleanup before remounting after a timed-out handoff', async () => {
        let receive!: (request: ChatWindowRequest) => void;
        bridge.onRequest.mockImplementationOnce(async (cb) => {
            receive = cb;
            return vi.fn();
        });
        let reject!: (error: Error) => void;
        bridge.prepare.mockImplementationOnce((_bot, unmount) => {
            unmount();
            return new Promise<void>((_resolve, fail) => {
                reject = fail;
            });
        });
        render(<ChatPopoutApp />);
        await screen.findByText('聊天工作区');
        await act(async () => receive({ v: 1, requestId: 'old', action: 'close' }));
        expect(screen.queryByText('聊天工作区')).not.toBeInTheDocument();
        await act(async () => receive({ v: 1, requestId: '', action: 'resume' }));
        expect(screen.queryByText('聊天工作区')).not.toBeInTheDocument();
        await act(async () => reject(new Error('聊天交接已取消')));
        expect(screen.getByText('聊天工作区')).toBeInTheDocument();
        expect(bridge.reply).not.toHaveBeenCalled();
        expect(bridge.notice).not.toHaveBeenCalledWith(
            'window:handoff',
            '聊天窗口切换失败',
            '聊天交接已取消',
        );
    });

    it('ignores a late unmount after resume and serializes the next handoff', async () => {
        let receive!: (request: ChatWindowRequest) => void;
        bridge.onRequest.mockImplementationOnce(async (cb) => {
            receive = cb;
            return vi.fn();
        });
        let finish!: () => void;
        let lateUnmount!: () => void;
        bridge.prepare.mockImplementationOnce((_bot, unmount) => {
            lateUnmount = unmount;
            return new Promise<void>((resolve) => {
                finish = resolve;
            });
        });
        render(<ChatPopoutApp />);
        await screen.findByText('聊天工作区');
        await act(async () => receive({ v: 1, requestId: 'old', action: 'close' }));
        await act(async () => receive({ v: 1, requestId: '', action: 'resume' }));
        act(() => lateUnmount());
        expect(screen.getByText('聊天工作区')).toBeInTheDocument();
        await act(async () => receive({ v: 1, requestId: 'new', action: 'embed' }));
        expect(bridge.prepare).toHaveBeenCalledOnce();
        await act(async () => finish());
        expect(bridge.prepare).toHaveBeenCalledTimes(2);
        expect(bridge.prepare).toHaveBeenLastCalledWith('bot', expect.any(Function), true);
        expect(bridge.reply).toHaveBeenCalledOnce();
        expect(bridge.reply).toHaveBeenCalledWith('new', null);
    });
});
