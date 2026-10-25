// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AssistantSettings } from '../../../core/domain/assistant/types';
import { preferencesStore } from '../../../core/domain/settings/preferencesStore';
import { AgentSection } from './AgentSection';

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
    return { hasApiKey, configured: hasApiKey && !!saved.builtin.model, error: null };
};
const clients: QueryClient[] = [];
function mount() {
    const client = new QueryClient({
        defaultOptions: {
            queries: { retry: false, gcTime: 0, refetchOnWindowFocus: false },
            mutations: { retry: false },
        },
    });
    clients.push(client);
    return {
        ...render(
            <QueryClientProvider client={client}>
                <AgentSection />
            </QueryClientProvider>,
        ),
        client,
    };
}
beforeEach(() => {
    saved = {
        defaultProvider: 'builtin',
        builtin: {
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
    service.setApiKey.mockReset().mockImplementation(async () => {
        keyOrigin = new URL(saved.builtin.baseUrl).origin;
        return status();
    });
    service.clearApiKey.mockReset().mockImplementation(async () => {
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
        fireEvent.click(screen.getByRole('button', { name: '保存 Key' }));
        expect(input).toHaveValue('');
        await screen.findByText('API Key 已保存。');
        expect(service.setApiKey).toHaveBeenCalledOnce();
        expect(service.setApiKey).toHaveBeenCalledWith(key);
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
    it('更换未保存的服务地址时禁用Key保存和清除，保存地址后才可绑定新Key', async () => {
        keyOrigin = 'https://models.example';
        mount();
        const address = await screen.findByRole('textbox', { name: '服务地址' });
        await waitFor(() => expect(screen.getByRole('button', { name: '清除 Key' })).toBeEnabled());
        fireEvent.change(screen.getByLabelText('API Key'), {
            target: { value: 'discard-before-endpoint-change' },
        });
        fireEvent.change(address, { target: { value: 'https://other.example/v1' } });
        expect(screen.getByLabelText('API Key')).toHaveValue('');
        expect(screen.getByLabelText('API Key')).toBeDisabled();
        expect(screen.getByRole('button', { name: '保存 Key' })).toBeDisabled();
        expect(screen.getByRole('button', { name: '清除 Key' })).toBeDisabled();
        expect(service.setApiKey).not.toHaveBeenCalled();
        expect(service.clearApiKey).not.toHaveBeenCalled();
        fireEvent.click(screen.getByRole('button', { name: '保存助手设置' }));
        await waitFor(() => expect(screen.getByLabelText('API Key')).toBeEnabled());
        fireEvent.change(screen.getByLabelText('API Key'), {
            target: { value: 'new-endpoint-unit-key' },
        });
        fireEvent.click(screen.getByRole('button', { name: '保存 Key' }));
        await screen.findByText('API Key 已保存。');
        expect(saved.builtin.baseUrl).toBe('https://other.example/v1');
        expect(keyOrigin).toBe('https://other.example');
        expect(service.saveSettings.mock.invocationCallOrder[0]).toBeLessThan(
            service.setApiKey.mock.invocationCallOrder[0]!,
        );
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
        expect(screen.getByRole('button', { name: '清除 Key' })).toBeDisabled();
        expect(status()).toEqual({ hasApiKey: false, configured: false, error: null });
    });
    it('Key提交失败也清空输入，错误不显示Key；未提交输入随设置组件关闭而丢弃', async () => {
        const key = 'failed-unit-test-key';
        service.setApiKey.mockRejectedValueOnce(new Error(`错误 ${key}`));
        const page = mount();
        const input = await screen.findByLabelText('API Key');
        fireEvent.change(input, { target: { value: key } });
        fireEvent.click(screen.getByRole('button', { name: '保存 Key' }));
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
});
