import type { ReactNode } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { TooltipProvider } from '../../../shared/ui';
import { preferencesStore } from '../../../core/domain/settings/preferencesStore';
import type { RequestItem } from '../../../core/domain/debug/requestHandling';
import { _resetDangerSkipsForTests } from '../DangerConfirmDialog';
import { ChatViewContext, type ChatViewApi } from '../../../shared/chat/chatContext';
import { RequestCard } from './SystemRows';

function item(patch: Partial<RequestItem> = {}): RequestItem {
    return {
        kind: 'request',
        key: 'e1',
        seq: 1,
        at: 1_700_000_000_000,
        requestType: 'friend',
        userId: 10001,
        comment: '我是小明',
        flag: 'FLAG-1',
        raw: { post_type: 'request', request_type: 'friend' },
        ...patch,
    };
}

function api(overrides: Partial<ChatViewApi> = {}): ChatViewApi {
    return {
        findMessage: () => undefined,
        nameOf: (id) => (id === 10001 ? '小明' : undefined),
        sessionName: () => undefined,
        selfId: () => 1919810,
        bot: () => ({ id: 'bot-nc', name: '小雪' }),
        toggleSelect: () => {},
        reply: () => {},
        openDetail: () => {},
        previewFill: () => ({ ok: false, reason: '没有能填的参数' }),
        fill: () => ({ ok: false, reason: '没有能填的参数' }),
        handleRequest: async () => ({ ok: true }),
        openImage: () => {},
        openLink: () => {},
        revealMessage: () => false,
        isExpanded: () => false,
        setExpanded: () => {},
        ...overrides,
    };
}

function renderCard(entry: RequestItem, apiOverrides: Partial<ChatViewApi> = {}) {
    const wrapper = ({ children }: { children: ReactNode }) => (
        <TooltipProvider>
            <ChatViewContext.Provider value={api(apiOverrides)}>
                {children}
            </ChatViewContext.Provider>
        </TooltipProvider>
    );
    return render(<RequestCard item={entry} />, { wrapper });
}

beforeEach(() => {
    preferencesStore.setMotionEnabled(false);
});

afterEach(() => {
    cleanup();
    _resetDangerSkipsForTests();
});

describe('RequestCard 同意 / 拒绝', () => {
    it('点「同意」直接发 set_friend_add_request，成功后卡片标「已同意」', async () => {
        const user = userEvent.setup();
        const handleRequest = vi.fn(async () => ({ ok: true }) as const);
        renderCard(item(), { handleRequest });

        await user.click(screen.getByRole('button', { name: '同意' }));
        expect(handleRequest).toHaveBeenCalledWith(item(), true);
        expect(await screen.findByText('已同意')).toBeInTheDocument();
        expect(screen.queryByRole('button', { name: '拒绝' })).not.toBeInTheDocument();
    });

    it('点「拒绝」先弹二次确认，确认后才发；卡片标「已拒绝」', async () => {
        const user = userEvent.setup();
        const handleRequest = vi.fn(async () => ({ ok: true }) as const);
        renderCard(item(), { handleRequest });

        await user.click(screen.getByRole('button', { name: '拒绝' }));
        expect(handleRequest).not.toHaveBeenCalled();
        expect(await screen.findByText('会拒绝 小明 的加好友请求')).toBeInTheDocument();
        // Radix 模态期间 body 是 pointer-events: none，直接派发点击
        fireEvent.click(screen.getByRole('button', { name: '确认调用' }));
        await waitFor(() => expect(handleRequest).toHaveBeenCalledWith(item(), false));
        expect(await screen.findByText('已拒绝')).toBeInTheDocument();
    });

    it('取消确认就什么都没发生', async () => {
        const user = userEvent.setup();
        const handleRequest = vi.fn(async () => ({ ok: true }) as const);
        renderCard(item(), { handleRequest });

        await user.click(screen.getByRole('button', { name: '拒绝' }));
        fireEvent.click(await screen.findByRole('button', { name: '取消' }));
        await waitFor(() =>
            expect(screen.queryByText('会拒绝 小明 的加好友请求')).not.toBeInTheDocument(),
        );
        expect(handleRequest).not.toHaveBeenCalled();
        expect(screen.getByRole('button', { name: '拒绝' })).toBeInTheDocument();
    });

    it('处理失败：按钮留着可以重试，失败原因写在卡片上', async () => {
        const user = userEvent.setup();
        const handleRequest = vi.fn(
            async () => ({ ok: false, reason: 'retcode 1400 · 权限不足' }) as const,
        );
        renderCard(item(), { handleRequest });

        await user.click(screen.getByRole('button', { name: '同意' }));
        expect(await screen.findByText('retcode 1400 · 权限不足')).toBeInTheDocument();
        // 按钮没被吃掉，再来一次
        await user.click(screen.getByRole('button', { name: '同意' }));
        expect(handleRequest).toHaveBeenCalledTimes(2);
    });

    it('邀请 Bot 入群的标题和拒绝确认句按邀请写', async () => {
        const user = userEvent.setup();
        const invite = item({
            requestType: 'group',
            groupId: 100001,
            raw: {
                post_type: 'request',
                request_type: 'group',
                sub_type: 'invite',
                group_id: 100001,
            },
        });
        renderCard(invite, { sessionName: (key) => (key === 'group:100001' ? '甲群' : undefined) });

        expect(screen.getByText('小明 邀请 Bot 加入群 甲群')).toBeInTheDocument();
        await user.click(screen.getByRole('button', { name: '拒绝' }));
        expect(await screen.findByText('会拒绝 小明 拉 Bot 进群 甲群 的邀请')).toBeInTheDocument();
    });

    it('没有 flag 的请求不给「同意 / 拒绝」', () => {
        renderCard(item({ flag: '' }));
        expect(screen.queryByRole('button', { name: '同意' })).not.toBeInTheDocument();
        expect(screen.queryByRole('button', { name: '拒绝' })).not.toBeInTheDocument();
    });
});
