import type { ReactNode } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { DebugChannels } from '../../../core/ipc/generated/debug/DebugChannels';
import type { DebugRequestDraft } from '../../../core/ipc/generated/debug/DebugRequestDraft';

const service = {
    channels: vi.fn(),
};

vi.mock('../../../core/services/onebot-debug.service', () => ({
    onebotDebugService: new Proxy(
        {},
        {
            get: (_t, key: string) => (...args: unknown[]) =>
                (service as Record<string, (...a: unknown[]) => unknown>)[key](...args),
        },
    ),
}));
vi.mock('../../../hooks/ui/pushErrorBar', () => ({ pushErrorBar: vi.fn() }));

import { TooltipProvider } from '../../../shared/ui';
import { preferencesStore } from '../../../hooks/preferences/preferencesStore';
import { ExportSnippetDialog } from './ExportSnippetDialog';

const CHANNELS: DebugChannels = {
    bot_id: 'bot-nc',
    channels: [
        {
            id: { kind: 'internal' },
            label: '内部通道（WebUI）',
            can_call: true,
            can_receive: true,
            status: { kind: 'available' },
            endpoint: 'WebUI',
            token_hint: null,
        },
        {
            id: { kind: 'http', name: 'main' },
            label: 'HTTP · main :3000',
            can_call: true,
            can_receive: false,
            status: { kind: 'available' },
            endpoint: '127.0.0.1:3000/',
            token_hint: 'my***en',
        },
    ],
    auto_call: { kind: 'internal' },
    auto_events: { kind: 'internal' },
};

function tab(patch: Partial<DebugRequestDraft> = {}): DebugRequestDraft {
    return { id: 't1', action: 'send_group_msg', params_text: '', timeout_ms: null, channel: null, ...patch };
}

function renderDialog(tabDraft = tab(), params: Record<string, unknown> = { group_id: 100001 }) {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = ({ children }: { children: ReactNode }) => (
        <QueryClientProvider client={client}>
            <TooltipProvider>{children}</TooltipProvider>
        </QueryClientProvider>
    );
    return render(
        <ExportSnippetDialog open onOpenChange={() => {}} tab={tabDraft} params={params} botId="bot-nc" callChannel={{ kind: 'auto' }} />,
        { wrapper },
    );
}

beforeEach(() => {
    service.channels.mockReset();
    service.channels.mockResolvedValue(CHANNELS);
    preferencesStore.setMotionEnabled(false);
});

afterEach(() => {
    cleanup();
});

describe('ExportSnippetDialog', () => {
    it('标签没指定通道、自动落在内部通道：占位地址加说明，三种语言都能切出来', async () => {
        const user = userEvent.setup();
        renderDialog();

        expect(await screen.findByText(/内部通道走 Bot WebUI 自带的调试接口/)).toBeInTheDocument();
        expect(screen.getByText(/http:\/\/127\.0\.0\.1:3000\/send_group_msg/)).toBeInTheDocument();

        await user.click(screen.getByRole('tab', { name: 'Python' }));
        expect(screen.getByText(/import requests/)).toBeInTheDocument();

        await user.click(screen.getByRole('tab', { name: 'JavaScript' }));
        expect(screen.getByText(/const response = await fetch/)).toBeInTheDocument();
    });

    it('标签指定了 HTTP 通道：地址用通道的 endpoint，写明 token 打码值', async () => {
        renderDialog(tab({ channel: { kind: 'http', name: 'main' } }));

        // 说明行和打码提醒都被 JSX 插值拆成多个文本节点，直接看整个对话框的 textContent
        const dialog = await screen.findByRole('dialog');
        await waitFor(() => expect(dialog.textContent).toContain('按通道 HTTP · main 生成'));
        expect(dialog.textContent).toContain('token 打码显示（my***en）');
        expect(dialog.textContent).toContain('Authorization: Bearer my***en');
    });

    it('一键复制：写进剪贴板的是当前语言的片段', async () => {
        const user = userEvent.setup();
        renderDialog();

        await screen.findByRole('tab', { name: 'Python' });
        await user.click(screen.getByRole('button', { name: '复制片段' }));
        await waitFor(async () => expect(await navigator.clipboard.readText()).toContain('curl -X POST'));
        expect(screen.getByRole('button', { name: '已复制' })).toBeInTheDocument();
    });
});
