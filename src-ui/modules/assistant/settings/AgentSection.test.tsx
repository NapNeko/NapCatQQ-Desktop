// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AssistantProvider, AssistantSettings } from '../../../core/domain/assistant/types';
import { preferencesStore } from '../../../core/domain/settings/preferencesStore';
import { AgentSection } from './AgentSection';

const service = vi.hoisted(() => ({
    settings: vi.fn(),
    providers: vi.fn(),
    saveSettings: vi.fn(),
    modelStatus: vi.fn(),
    setApiKey: vi.fn(),
    clearApiKey: vi.fn(),
    preview: false,
}));
vi.mock('../../../core/services/assistant.service', () => ({ assistantService: service }));
vi.mock('../../../hooks/useOpenExternal', () => ({ useOpenExternal: () => vi.fn() }));

function settings(): AssistantSettings {
    return {
        defaultProvider: 'dsh',
        builtin: {
            providerPreset: 'custom',
            apiFormat: 'openAiChatCompletions',
            baseUrl: '',
            model: '',
            maxSteps: 6,
            maxOutputTokens: 2048,
            requestTimeoutSecs: 90,
        },
        providers: [
            {
                provider: 'dsh',
                enabled: true,
                executablePath: null,
                extraArgs: [],
                allowNativeTools: false,
            },
        ],
    };
}
const clients: QueryClient[] = [];
function mount() {
    const client = new QueryClient({
        defaultOptions: {
            queries: { retry: false, gcTime: 0, refetchOnWindowFocus: false },
            mutations: { retry: false },
        },
    });
    clients.push(client);
    render(
        <QueryClientProvider client={client}>
            <AgentSection />
        </QueryClientProvider>,
    );
    return client;
}

beforeEach(() => {
    service.settings.mockReset().mockResolvedValue(settings());
    service.providers.mockReset().mockResolvedValue([
        {
            provider: 'dsh',
            label: 'DSH',
            enabled: true,
            installed: true,
            executablePath: 'C:\\agents\\dsh.cmd',
            version: '1.0',
            error: null,
            documentationUrl: 'https://example.com/dsh',
        },
    ]);
    service.saveSettings.mockReset().mockResolvedValue(undefined);
    service.modelStatus
        .mockReset()
        .mockResolvedValue({ hasApiKey: false, configured: false, error: null });
    service.setApiKey.mockReset();
    service.clearApiKey.mockReset();
    act(() => {
        preferencesStore.reset();
        preferencesStore.setMotionEnabled(false);
    });
});
afterEach(() => {
    cleanup();
    clients.forEach((client) => client.clear());
    clients.length = 0;
    act(() => preferencesStore.reset());
});

async function expand() {
    fireEvent.click(await screen.findByRole('button', { name: '外部 Agent（可选）' }));
    fireEvent.click(await screen.findByRole('button', { name: /^DSH/ }));
    return screen.getByRole('textbox', { name: '可执行文件路径' });
}

describe('Agent 高级设置', () => {
    it('新安装的八个默认 Agent 都可以配置，不会因缺少既有配置而显示空列表', async () => {
        const providers: [AssistantProvider, string][] = [
            ['dsh', 'DeepSeek DSH'],
            ['qwen', 'Qwen Code'],
            ['kimi', 'Kimi Code'],
            ['miniMax', 'MiniMax Code'],
            ['codeBuddy', 'CodeBuddy Code'],
            ['qoder', 'Qoder CLI'],
            ['trae', 'TRAE CLI'],
            ['custom', '自定义 ACP Agent'],
        ];
        service.settings.mockResolvedValue({
            defaultProvider: null,
            builtin: settings().builtin,
            providers: providers.map(([provider]) => ({
                provider,
                enabled: true,
                executablePath: null,
                extraArgs: [],
                allowNativeTools: false,
            })),
        });
        service.providers.mockResolvedValue(
            providers.map(([provider, label]) => ({
                provider,
                label,
                enabled: true,
                installed: false,
                executablePath: null,
                version: null,
                error: null,
                documentationUrl: 'https://example.com/agent',
            })),
        );
        mount();
        fireEvent.click(await screen.findByRole('button', { name: '外部 Agent（可选）' }));
        await screen.findByRole('switch', { name: '启用 DeepSeek DSH' });
        for (const [, label] of providers)
            expect(screen.getByRole('switch', { name: `启用 ${label}` })).toBeChecked();
        expect(screen.getAllByRole('switch')).toHaveLength(8);
        fireEvent.click(screen.getByRole('button', { name: /^自定义 ACP Agent/ }));
        expect(screen.getByRole('textbox', { name: '可执行文件路径' })).toHaveValue('');
    });

    it('逐行参数保留含空格的单个参数，清理空行与路径外部空白后保存', async () => {
        mount();
        const path = await expand();
        fireEvent.change(path, { target: { value: ' C:\\my agents\\dsh.cmd ' } });
        fireEvent.change(screen.getByRole('textbox', { name: '附加参数（每行一个）' }), {
            target: { value: '--model\nmodel with spaces\n\n--flag' },
        });
        fireEvent.click(screen.getByRole('button', { name: '保存助手设置' }));
        await waitFor(() =>
            expect(service.saveSettings).toHaveBeenCalledWith({
                defaultProvider: 'dsh',
                builtin: settings().builtin,
                providers: [
                    {
                        provider: 'dsh',
                        enabled: true,
                        executablePath: 'C:\\my agents\\dsh.cmd',
                        extraArgs: ['--model', 'model with spaces', '--flag'],
                        allowNativeTools: false,
                    },
                ],
            }),
        );
    });

    it('保存失败显示原因并保留草稿，后台重新读取也不能覆盖用户输入', async () => {
        service.saveSettings.mockRejectedValueOnce(new Error('参数不受支持'));
        const client = mount();
        const path = await expand();
        fireEvent.change(path, { target: { value: 'C:\\draft\\agent.cmd' } });
        fireEvent.click(screen.getByRole('button', { name: '保存助手设置' }));
        expect(await screen.findByRole('alert')).toHaveTextContent('参数不受支持');
        expect(path).toHaveValue('C:\\draft\\agent.cmd');
        expect(screen.getByRole('button', { name: '保存助手设置' })).toBeEnabled();
        await act(async () => {
            await client.invalidateQueries({ queryKey: ['assistantSettings'] });
        });
        expect(path).toHaveValue('C:\\draft\\agent.cmd');
        fireEvent.click(screen.getByRole('button', { name: '保存助手设置' }));
        await waitFor(() => expect(service.saveSettings).toHaveBeenCalledTimes(2));
        expect(service.saveSettings.mock.calls[1]?.[0].providers[0].executablePath).toBe(
            'C:\\draft\\agent.cmd',
        );
    });

    it('保存期间禁用外部Agent的可编辑字段，完成后恢复输入', async () => {
        let finish!: () => void;
        const saving = new Promise<void>((resolve) => {
            finish = resolve;
        });
        service.saveSettings.mockReturnValueOnce(saving);
        mount();
        const path = await expand();
        fireEvent.change(path, { target: { value: 'C:\\saved\\agent.cmd' } });
        fireEvent.click(screen.getByRole('button', { name: '保存助手设置' }));
        expect(path).toBeDisabled();
        expect(screen.getByRole('textbox', { name: '附加参数（每行一个）' })).toBeDisabled();
        expect(screen.getByRole('combobox', { name: '默认助手' })).toBeDisabled();
        expect(screen.getByRole('switch', { name: '启用 DSH' })).toBeDisabled();
        expect(screen.getByRole('switch', { name: '允许 Agent 内建工具' })).toBeDisabled();
        await act(async () => finish());
        await waitFor(() => expect(path).toBeEnabled());
    });

    it('内建工具默认关闭，明确开启后才随配置写入', async () => {
        mount();
        await expand();
        const nativeTools = screen.getByRole('switch', { name: '允许 Agent 内建工具' });
        expect(nativeTools).not.toBeChecked();
        expect(screen.getByText(/可能读写本机文件或执行命令/)).toBeVisible();
        fireEvent.click(nativeTools);
        fireEvent.click(screen.getByRole('button', { name: '保存助手设置' }));
        await waitFor(() =>
            expect(service.saveSettings).toHaveBeenCalledWith({
                ...settings(),
                providers: [{ ...settings().providers[0]!, allowNativeTools: true }],
            }),
        );
    });
});
