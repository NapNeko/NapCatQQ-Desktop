// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import userEvent from '@testing-library/user-event';
import type { AssistantSettings } from '../../../core/domain/assistant/types';
import { preferencesStore } from '../../../core/domain/settings/preferencesStore';
import { requiresBuiltinApiKey } from '../../../core/domain/assistant/model';
import { AgentSection } from './AgentSection';
import { ModelSetup } from './ModelSetup';

const service = vi.hoisted(() => ({
    settings: vi.fn(),
    saveSettings: vi.fn(),
    providers: vi.fn(),
    modelStatus: vi.fn(),
    setApiKey: vi.fn(),
    clearApiKey: vi.fn(),
    preview: false,
}));
vi.mock('../../../core/services/assistant.service', () => ({ assistantService: service }));
vi.mock('../../../hooks/useOpenExternal', () => ({ useOpenExternal: () => vi.fn() }));
let saved: AssistantSettings;
let keyOrigin: string | null;
const status = () => {
    const hasApiKey =
        !!saved.builtin.baseUrl && keyOrigin === new URL(saved.builtin.baseUrl).origin;
    return {
        hasApiKey,
        configured: (hasApiKey || !requiresBuiltinApiKey(saved.builtin)) && !!saved.builtin.model,
        error: null,
    };
};
const clients: QueryClient[] = [];
beforeAll(() => {
    Element.prototype.hasPointerCapture ??= () => false;
    Element.prototype.releasePointerCapture ??= () => {};
    Element.prototype.scrollIntoView ??= () => {};
});
function mount(component = <AgentSection />) {
    const client = new QueryClient({
        defaultOptions: {
            queries: { retry: false, gcTime: 0, refetchOnWindowFocus: false },
            mutations: { retry: false },
        },
    });
    clients.push(client);
    return {
        ...render(<QueryClientProvider client={client}>{component}</QueryClientProvider>),
        client,
    };
}
beforeEach(() => {
    saved = {
        defaultProvider: 'builtin',
        builtin: {
            providerPreset: 'custom',
            apiFormat: 'openAiChatCompletions',
            baseUrl: 'https://models.example/v1',
            model: 'chosen-model',
            maxSteps: 6,
            maxOutputTokens: 2048,
            requestTimeoutSecs: 90,
        },
        providers: [],
    };
    keyOrigin = null;
    service.settings.mockReset().mockImplementation(async () => structuredClone(saved));
    service.saveSettings.mockReset().mockImplementation(async (next: AssistantSettings) => {
        saved = structuredClone(next);
    });
    service.providers.mockReset().mockResolvedValue([]);
    service.modelStatus.mockReset().mockImplementation(async () => status());
    service.setApiKey
        .mockReset()
        .mockImplementation(async (_apiKey: string, expectedOrigin: string) => {
            if (expectedOrigin !== new URL(saved.builtin.baseUrl).origin)
                throw new Error('服务地址已更改，请重新输入 API Key。');
            keyOrigin = new URL(saved.builtin.baseUrl).origin;
            return status();
        });
    service.clearApiKey.mockReset().mockImplementation(async (expectedOrigin: string) => {
        if (expectedOrigin !== new URL(saved.builtin.baseUrl).origin)
            throw new Error('服务地址已更改，请重新确认后清除 Key。');
        keyOrigin = null;
        return status();
    });
    act(() => {
        preferencesStore.reset();
        preferencesStore.setMotionEnabled(false);
    });
});
afterEach(() => {
    cleanup();
    clients.forEach((client) => client.clear());
    clients.length = 0;
    vi.restoreAllMocks();
    act(() => preferencesStore.reset());
});

describe('内置模型与Key设置', () => {
    it('完整 Chat Completions 终点改为 Responses 时替换标准终点，保留同源 Key', async () => {
        saved.builtin.baseUrl = 'https://models.example/v1/chat/completions';
        const user = userEvent.setup();
        mount();
        fireEvent.change(await screen.findByLabelText('API Key'), {
            target: { value: 'same-origin-unit-key' },
        });
        await user.click(screen.getByRole('combobox', { name: 'API 格式' }));
        await user.click(screen.getByRole('option', { name: 'OpenAI Responses' }));
        expect(screen.getByRole('textbox', { name: '服务地址' })).toHaveValue(
            'https://models.example/v1/responses',
        );
        expect(screen.getByLabelText('API Key')).toHaveValue('same-origin-unit-key');
        fireEvent.click(screen.getByRole('button', { name: '保存助手设置' }));
        await screen.findByText('API Key 已保存。');
        expect(saved.builtin).toMatchObject({
            apiFormat: 'openAiResponses',
            baseUrl: 'https://models.example/v1/responses',
        });
        expect(service.setApiKey).toHaveBeenCalledWith(
            'same-origin-unit-key',
            'https://models.example',
        );
    });
    it.each(['settings', 'setup'] as const)(
        '%s 后台换源时清空仅输入而未保存的 Key',
        async (surface) => {
            const { client } = mount(
                surface === 'setup' ? <ModelSetup onMore={vi.fn()} /> : <AgentSection />,
            );
            const input = await screen.findByLabelText('API Key');
            fireEvent.change(input, { target: { value: 'old-origin-unit-key' } });
            saved.builtin.baseUrl = 'https://background.example/v1';
            await act(async () => {
                await client.invalidateQueries({ queryKey: ['assistantSettings'] });
            });
            await waitFor(() =>
                expect(screen.getByRole('textbox', { name: '服务地址' })).toHaveValue(
                    'https://background.example/v1',
                ),
            );
            expect(screen.getByLabelText('API Key')).toHaveValue('');
            expect(screen.getByText('服务地址已换源，未保存的 API Key 已清空。')).toBeVisible();
            expect(service.setApiKey).not.toHaveBeenCalled();
        },
    );
    it('未保存的新格式不显示旧profile错误，同源已存Key仍可使用', async () => {
        keyOrigin = 'https://models.example';
        service.modelStatus.mockResolvedValue({
            hasApiKey: true,
            configured: false,
            error: '旧配置格式不受支持',
        });
        const user = userEvent.setup();
        mount();
        await screen.findByText('旧配置格式不受支持');
        await user.click(screen.getByRole('combobox', { name: 'API 格式' }));
        await user.click(screen.getByRole('option', { name: 'Anthropic Messages' }));
        expect(screen.queryByText('旧配置格式不受支持')).not.toBeInTheDocument();
        expect(screen.getByLabelText('API Key')).toHaveAttribute(
            'placeholder',
            '已保存，输入新 Key 可替换',
        );
        expect(screen.getByText('待配置')).toBeVisible();
    });
    it('已配置徽章只描述当前profile，未保存的模型变更显示待配置', async () => {
        keyOrigin = 'https://models.example';
        mount();
        await screen.findByText('已配置');
        fireEvent.change(screen.getByRole('textbox', { name: '模型名称' }), {
            target: { value: 'different-model' },
        });
        expect(screen.queryByText('已配置')).not.toBeInTheDocument();
        expect(screen.getByText('待配置')).toBeVisible();
        expect(screen.getByLabelText('API Key')).toHaveAttribute(
            'placeholder',
            '已保存，输入新 Key 可替换',
        );
    });
    it('设置保存成功而刷新失败时保留已保存值，并显示读取失败而不重复写入', async () => {
        mount();
        const model = await screen.findByRole('textbox', { name: '模型名称' });
        fireEvent.change(model, { target: { value: 'saved-before-refresh-error' } });
        service.settings.mockRejectedValueOnce(new Error('设置读取暂不可用'));
        fireEvent.click(screen.getByRole('button', { name: '保存助手设置' }));
        await screen.findByText('设置读取暂不可用');
        await screen.findByText('设置已保存。');
        expect(model).toHaveValue('saved-before-refresh-error');
        expect(saved.builtin.model).toBe('saved-before-refresh-error');
        expect(service.saveSettings).toHaveBeenCalledOnce();
        expect(service.setApiKey).not.toHaveBeenCalled();
    });
    it('保存设置后若实际服务被并发换源，绑定origin阻止Key写入新服务', async () => {
        service.saveSettings.mockImplementationOnce(async (next: AssistantSettings) => {
            saved = structuredClone(next);
            saved.builtin.baseUrl = 'https://raced.example/v1';
        });
        mount();
        fireEvent.change(await screen.findByRole('textbox', { name: '模型名称' }), {
            target: { value: 'updated-model' },
        });
        fireEvent.change(screen.getByLabelText('API Key'), {
            target: { value: 'race-protected-unit-key' },
        });
        fireEvent.click(screen.getByRole('button', { name: '保存助手设置' }));
        await screen.findByText('服务地址已更改，请重新输入 API Key。');
        expect(service.setApiKey).toHaveBeenCalledWith(
            'race-protected-unit-key',
            'https://models.example',
        );
        expect(keyOrigin).toBeNull();
        expect(screen.getByLabelText('API Key')).toHaveValue('');
    });
    it('切换服务商填默认地址，跨源时清空未提交 Key，模型可选示例或手填', async () => {
        saved.builtin.baseUrl = '';
        saved.builtin.model = '';
        const user = userEvent.setup();
        mount();
        await user.click(await screen.findByRole('combobox', { name: '服务商预设' }));
        await user.click(screen.getByRole('option', { name: 'OpenAI', exact: true }));
        expect(screen.getByRole('textbox', { name: '服务地址' })).toHaveValue(
            'https://api.openai.com/v1',
        );
        expect(screen.getByRole('combobox', { name: 'API 格式' })).toHaveTextContent(
            'OpenAI Responses',
        );
        expect(screen.getByRole('textbox', { name: '模型名称' })).toHaveValue('');
        fireEvent.change(screen.getByLabelText('API Key'), {
            target: { value: 'unsubmitted-provider-key' },
        });
        await user.click(screen.getByRole('combobox', { name: '服务商预设' }));
        await user.click(screen.getByRole('option', { name: 'Anthropic', exact: true }));
        expect(screen.getByRole('textbox', { name: '服务地址' })).toHaveValue(
            'https://api.anthropic.com/v1',
        );
        expect(screen.getByRole('combobox', { name: 'API 格式' })).toHaveTextContent(
            'Anthropic Messages',
        );
        expect(screen.getByLabelText('API Key')).toHaveValue('');
        expect(screen.getByText('服务地址已换源，未保存的 API Key 已清空。')).toBeVisible();
        fireEvent.click(screen.getByRole('button', { name: 'claude-sonnet-5-5' }));
        expect(screen.getByRole('textbox', { name: '模型名称' })).toHaveValue('claude-sonnet-5-5');
        fireEvent.change(screen.getByRole('textbox', { name: '模型名称' }), {
            target: { value: 'my-private-model' },
        });
        expect(screen.getByRole('textbox', { name: '模型名称' })).toHaveValue('my-private-model');
        expect(service.setApiKey).not.toHaveBeenCalled();
        expect(service.saveSettings).not.toHaveBeenCalled();
    });
    it('手动地址和模型不会被服务商预设覆盖，格式独立改且不清同源 Key', async () => {
        const user = userEvent.setup();
        mount();
        fireEvent.change(await screen.findByLabelText('API Key'), {
            target: { value: 'same-origin-unit-key' },
        });
        await user.click(screen.getByRole('combobox', { name: '服务商预设' }));
        await user.click(screen.getByRole('option', { name: 'Google Gemini' }));
        expect(screen.getByRole('textbox', { name: '服务地址' })).toHaveValue(
            'https://models.example/v1',
        );
        expect(screen.getByRole('textbox', { name: '模型名称' })).toHaveValue('chosen-model');
        expect(screen.getByLabelText('API Key')).toHaveValue('same-origin-unit-key');
        await user.click(screen.getByRole('combobox', { name: 'API 格式' }));
        await user.click(screen.getByRole('option', { name: 'OpenAI Responses' }));
        expect(screen.getByRole('textbox', { name: '服务地址' })).toHaveValue(
            'https://models.example/v1',
        );
        expect(screen.getByLabelText('API Key')).toHaveValue('same-origin-unit-key');
        fireEvent.click(screen.getByRole('button', { name: '保存助手设置' }));
        await screen.findByText('API Key 已保存。');
        expect(saved.builtin).toMatchObject({
            providerPreset: 'gemini',
            apiFormat: 'openAiResponses',
            baseUrl: 'https://models.example/v1',
            model: 'chosen-model',
        });
        expect(JSON.stringify(saved)).not.toContain('same-origin-unit-key');
    });
    it('首次页只修改预设与格式也会保存，地址和已存 Key 保持同源', async () => {
        keyOrigin = 'https://models.example';
        const user = userEvent.setup();
        mount(<ModelSetup onMore={vi.fn()} />);
        await user.click(await screen.findByRole('combobox', { name: '服务商预设' }));
        await user.click(screen.getByRole('option', { name: 'Anthropic', exact: true }));
        await waitFor(() =>
            expect(screen.getByRole('button', { name: '保存并继续' })).toBeEnabled(),
        );
        fireEvent.click(screen.getByRole('button', { name: '保存并继续' }));
        await waitFor(() => expect(service.saveSettings).toHaveBeenCalledOnce());
        expect(saved.builtin).toMatchObject({
            providerPreset: 'anthropic',
            apiFormat: 'anthropicMessages',
            baseUrl: 'https://models.example/v1',
        });
        expect(service.setApiKey).not.toHaveBeenCalled();
    });
    it('本机 Ollama 首次配置不要求 Key，使用远端地址后重新要求', async () => {
        saved.builtin.baseUrl = '';
        saved.builtin.model = '';
        const user = userEvent.setup();
        mount(<ModelSetup onMore={vi.fn()} />);
        await user.click(await screen.findByRole('combobox', { name: '服务商预设' }));
        await user.click(screen.getByRole('option', { name: 'Ollama（本机）' }));
        expect(screen.getByLabelText('API Key（可选）')).toBeEnabled();
        fireEvent.change(screen.getByRole('textbox', { name: '模型名称' }), {
            target: { value: 'my-local-model' },
        });
        expect(screen.getByRole('button', { name: '保存并继续' })).toBeEnabled();
        fireEvent.change(screen.getByRole('textbox', { name: '服务地址' }), {
            target: { value: 'https://ollama.example/v1' },
        });
        expect(screen.getByLabelText('API Key')).toBeEnabled();
        expect(screen.getByRole('button', { name: '保存并继续' })).toBeDisabled();
        fireEvent.change(screen.getByRole('textbox', { name: '服务地址' }), {
            target: { value: 'http://localhost:11434/v1' },
        });
        fireEvent.click(screen.getByRole('button', { name: '保存并继续' }));
        await waitFor(() => expect(service.saveSettings).toHaveBeenCalledOnce());
        expect(saved.builtin).toMatchObject({
            providerPreset: 'ollama',
            apiFormat: 'openAiChatCompletions',
            baseUrl: 'http://localhost:11434/v1',
            model: 'my-local-model',
        });
        expect(service.setApiKey).not.toHaveBeenCalled();
        expect(status()).toMatchObject({ configured: true, hasApiKey: false });
    });
    it('默认显示内置模型，外部Agent未展开时不探测CLI', async () => {
        mount();
        expect(await screen.findByRole('textbox', { name: '服务地址' })).toHaveValue(
            'https://models.example/v1',
        );
        expect(screen.getByRole('textbox', { name: '模型名称' })).toHaveValue('chosen-model');
        expect(screen.queryByRole('combobox', { name: '默认助手' })).not.toBeInTheDocument();
        expect(service.providers).not.toHaveBeenCalled();
    });
    it('Key一次提交后立即清空，缓存、配置和浏览器存储都不保留Key', async () => {
        const storage = vi.spyOn(Storage.prototype, 'setItem');
        const { client } = mount();
        const input = await screen.findByLabelText('API Key');
        expect(input).toHaveAttribute('type', 'password');
        expect(input).toHaveValue('');
        const key = 'component-unit-test-key';
        fireEvent.change(input, { target: { value: key } });
        fireEvent.click(screen.getByRole('button', { name: '保存助手设置' }));
        expect(input).toHaveValue('');
        await screen.findByText('API Key 已保存。');
        expect(service.setApiKey).toHaveBeenCalledOnce();
        expect(service.setApiKey).toHaveBeenCalledWith(key, 'https://models.example');
        expect(JSON.stringify(saved)).not.toContain(key);
        expect(
            JSON.stringify(
                client
                    .getQueryCache()
                    .getAll()
                    .map((query) => query.state.data),
            ),
        ).not.toContain(key);
        expect(
            JSON.stringify(
                client
                    .getMutationCache()
                    .getAll()
                    .map((mutation) => mutation.state.variables),
            ),
        ).not.toContain(key);
        expect(JSON.stringify(storage.mock.calls)).not.toContain(key);
        expect(document.body.textContent).not.toContain(key);
    });
    it('服务地址与新Key一次保存，先落盘地址再绑定凭据', async () => {
        keyOrigin = 'https://models.example';
        mount();
        const address = await screen.findByRole('textbox', { name: '服务地址' });
        await waitFor(() => expect(screen.getByRole('button', { name: '清除 Key' })).toBeEnabled());
        fireEvent.change(address, { target: { value: 'https://other.example/v1' } });
        expect(screen.getByLabelText('API Key')).toBeEnabled();
        expect(screen.getByLabelText('API Key')).toHaveAttribute('placeholder', '输入 API Key');
        expect(screen.queryByRole('button', { name: '清除 Key' })).not.toBeInTheDocument();
        expect(service.setApiKey).not.toHaveBeenCalled();
        expect(service.clearApiKey).not.toHaveBeenCalled();
        fireEvent.change(screen.getByLabelText('API Key'), {
            target: { value: 'new-endpoint-unit-key' },
        });
        fireEvent.click(screen.getByRole('button', { name: '保存助手设置' }));
        await screen.findByText('API Key 已保存。');
        expect(saved.builtin.baseUrl).toBe('https://other.example/v1');
        expect(keyOrigin).toBe('https://other.example');
        expect(service.saveSettings.mock.invocationCallOrder[0]).toBeLessThan(
            service.setApiKey.mock.invocationCallOrder[0]!,
        );
    });
    it('同一服务的API路径变更继续识别已存Key，不额外提交凭据', async () => {
        keyOrigin = 'https://models.example';
        mount();
        const address = await screen.findByRole('textbox', { name: '服务地址' });
        await waitFor(() => expect(screen.getByRole('button', { name: '清除 Key' })).toBeEnabled());
        fireEvent.change(address, { target: { value: 'https://models.example/compat/v1' } });
        expect(screen.getByLabelText('API Key')).toHaveAttribute(
            'placeholder',
            '已保存，输入新 Key 可替换',
        );
        expect(screen.getByRole('button', { name: '清除 Key' })).toBeEnabled();
        fireEvent.click(screen.getByRole('button', { name: '保存助手设置' }));
        await screen.findByText('设置已保存。');
        expect(service.setApiKey).not.toHaveBeenCalled();
        expect(saved.builtin.baseUrl).toBe('https://models.example/compat/v1');
    });
    it('清除Key不读取原Key，更新为未配置状态', async () => {
        keyOrigin = 'https://models.example';
        mount();
        await waitFor(() => expect(screen.getByRole('button', { name: '清除 Key' })).toBeEnabled());
        expect(screen.getByLabelText('API Key')).toHaveValue('');
        fireEvent.click(screen.getByRole('button', { name: '清除 Key' }));
        await screen.findByText('API Key 已清除。');
        expect(service.clearApiKey).toHaveBeenCalledOnce();
        expect(service.setApiKey).not.toHaveBeenCalled();
        expect(screen.queryByRole('button', { name: '清除 Key' })).not.toBeInTheDocument();
        expect(status()).toEqual({ hasApiKey: false, configured: false, error: null });
    });
    it('Key提交失败也清空输入，错误不显示Key；未提交输入随设置组件关闭而丢弃', async () => {
        const key = 'failed-unit-test-key';
        service.setApiKey.mockRejectedValueOnce(new Error(`错误 ${key}`));
        const page = mount();
        const input = await screen.findByLabelText('API Key');
        fireEvent.change(input, { target: { value: key } });
        fireEvent.click(screen.getByRole('button', { name: '保存助手设置' }));
        await screen.findByText('错误 [REDACTED]');
        expect(input).toHaveValue('');
        expect(document.body.textContent).not.toContain(key);
        fireEvent.change(input, { target: { value: 'not-submitted-unit-key' } });
        page.unmount();
        mount();
        expect(await screen.findByLabelText('API Key')).toHaveValue('');
        expect(service.setApiKey).toHaveBeenCalledOnce();
    });
    it('高级预算遵守后端范围，无效草稿可撤销，普通模型配置不发Key请求', async () => {
        mount();
        fireEvent.click(await screen.findByRole('button', { name: '处理限制' }));
        fireEvent.change(screen.getByRole('spinbutton', { name: '单轮最多步骤' }), {
            target: { value: '13' },
        });
        expect(screen.getByRole('button', { name: '保存助手设置' })).toBeDisabled();
        fireEvent.click(screen.getByRole('button', { name: '撤销' }));
        expect(screen.getByRole('spinbutton', { name: '单轮最多步骤' })).toHaveValue(6);
        fireEvent.change(screen.getByRole('textbox', { name: '模型名称' }), {
            target: { value: 'new-model' },
        });
        fireEvent.click(screen.getByRole('button', { name: '保存助手设置' }));
        await waitFor(() => expect(service.saveSettings).toHaveBeenCalledOnce());
        expect(saved.builtin.model).toBe('new-model');
        expect(service.setApiKey).not.toHaveBeenCalled();
        expect(service.clearApiKey).not.toHaveBeenCalled();
    });
    it('首次配置保存失败保留地址、模型和未提交Key，用户明确重试后一次保存', async () => {
        service.saveSettings.mockRejectedValueOnce(new Error('无法保存设置'));
        mount(<ModelSetup onMore={vi.fn()} />);
        const address = await screen.findByRole('textbox', { name: '服务地址' });
        fireEvent.change(address, { target: { value: 'https://new.example/v1' } });
        fireEvent.change(screen.getByRole('textbox', { name: '模型名称' }), {
            target: { value: 'new-model' },
        });
        fireEvent.change(screen.getByLabelText('API Key'), {
            target: { value: 'first-setup-unit-key' },
        });
        fireEvent.click(screen.getByRole('button', { name: '保存并继续' }));
        await screen.findByText('无法保存设置');
        expect(address).toHaveValue('https://new.example/v1');
        expect(screen.getByRole('textbox', { name: '模型名称' })).toHaveValue('new-model');
        expect(screen.getByLabelText('API Key')).toHaveValue('first-setup-unit-key');
        expect(service.setApiKey).not.toHaveBeenCalled();
        fireEvent.click(screen.getByRole('button', { name: '保存并继续' }));
        await waitFor(() => expect(service.setApiKey).toHaveBeenCalledOnce());
        expect(saved.builtin.model).toBe('new-model');
        expect(keyOrigin).toBe('https://new.example');
        expect(screen.getByLabelText('API Key')).toHaveValue('');
    });
    it('首次配置的凭据状态读取失败可重试，不发送模型或Key请求', async () => {
        service.modelStatus.mockRejectedValueOnce(new Error('凭据存储暂不可用'));
        mount(<ModelSetup onMore={vi.fn()} />);
        await screen.findByText('凭据存储暂不可用');
        fireEvent.click(screen.getByRole('button', { name: '重新读取凭据状态' }));
        await waitFor(() => expect(screen.queryByText('凭据存储暂不可用')).not.toBeInTheDocument());
        expect(service.setApiKey).not.toHaveBeenCalled();
        expect(service.saveSettings).not.toHaveBeenCalled();
    });
});
