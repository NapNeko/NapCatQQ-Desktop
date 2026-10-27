// @vitest-environment jsdom

import { useState } from 'react';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type {
    AssistantModelStatus,
    AssistantBuiltinSettings,
    AssistantProvider,
    AssistantSettings,
} from '../../core/domain/assistant/types';
import { preferencesStore } from '../../core/domain/settings/preferencesStore';
import { NewSession } from './NewSession';

vi.mock('./settings/ModelSetup', () => ({
    ModelSetup: ({ onMore }: { onMore: () => void }) => (
        <section aria-label="连接模型">
            <button type="button" onClick={onMore}>
                更多设置
            </button>
        </section>
    ),
}));

const onStart = vi.fn();
const onSettings = vi.fn();
const onDiscover = vi.fn();
const onBots = vi.fn();
const settings: AssistantSettings = {
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
    providers: ['dsh', 'custom'].map((provider) => ({
        provider: provider as AssistantProvider,
        enabled: true,
        executablePath: null,
        extraArgs: [],
        allowNativeTools: false,
    })),
};
function Form({
    status = { hasApiKey: true, configured: true, error: null },
    noProviders = false,
    noBots = false,
    dshInstalled = true,
    creating = false,
    builtin,
}: {
    status?: AssistantModelStatus;
    noProviders?: boolean;
    noBots?: boolean;
    dshInstalled?: boolean;
    creating?: boolean;
    builtin?: Partial<AssistantBuiltinSettings>;
}) {
    const [provider, setProvider] = useState<AssistantProvider | null>(null);
    const [draft, setDraft] = useState('');
    const [botId, setBotId] = useState('10001');
    return (
        <NewSession
            bots={
                noBots
                    ? []
                    : [
                          { id: '10001', name: '测试 Bot' },
                          { id: '10002', name: '备用 Bot' },
                      ]
            }
            botId={botId}
            provider={provider}
            settings={{ ...settings, builtin: { ...settings.builtin, ...builtin } }}
            providers={
                noProviders
                    ? []
                    : [
                          {
                              provider: 'dsh',
                              label: 'DSH',
                              installed: dshInstalled,
                              enabled: true,
                              executablePath: null,
                              version: '1',
                              error: null,
                              documentationUrl: '',
                          },
                          {
                              provider: 'custom',
                              label: '自定义 ACP Agent',
                              installed: true,
                              enabled: true,
                              executablePath: null,
                              version: '1',
                              error: null,
                              documentationUrl: '',
                          },
                      ]
            }
            modelStatus={status}
            loading={false}
            providersLoading={noProviders}
            creating={creating}
            draft={draft}
            onDraft={setDraft}
            onDiscover={onDiscover}
            onBot={setBotId}
            onProvider={setProvider}
            onStart={onStart}
            onSettings={onSettings}
            onBots={onBots}
        />
    );
}
beforeAll(() => {
    Element.prototype.hasPointerCapture ??= () => false;
    Element.prototype.releasePointerCapture ??= () => {};
    Element.prototype.scrollIntoView ??= () => {};
});
beforeEach(() => {
    [onStart, onSettings, onDiscover, onBots].forEach((mock) => mock.mockReset());
    act(() => {
        preferencesStore.reset();
        preferencesStore.setMotionEnabled(false);
    });
});
afterEach(() => {
    cleanup();
    act(() => preferencesStore.reset());
});

describe('内置助手默认入口', () => {
    it('已配置的本机 Ollama 无 Key 也能开始，其他服务仍保留 Key 门控', () => {
        const page = render(
            <Form
                status={{ configured: true, hasApiKey: false, error: null }}
                builtin={{ providerPreset: 'ollama', baseUrl: 'http://localhost:11434/v1' }}
            />,
        );
        fireEvent.click(screen.getByRole('button', { name: '帮我检查当前运行状态' }));
        expect(onStart).toHaveBeenCalledWith('builtin', '10001', '帮我检查当前运行状态');
        page.rerender(
            <Form
                status={{ configured: true, hasApiKey: false, error: null }}
                builtin={{ providerPreset: 'ollama', baseUrl: 'https://ollama.example/v1' }}
            />,
        );
        expect(screen.getByRole('region', { name: '连接模型' })).toBeVisible();
    });
    it('输入问题后创建并发送，外部 CLI 尚未发现也不阻塞内置助手', () => {
        render(<Form noProviders />);
        expect(screen.getByRole('button', { name: '开始诊断' })).toBeDisabled();
        expect(onDiscover).not.toHaveBeenCalled();
        fireEvent.change(screen.getByRole('textbox', { name: '描述问题' }), {
            target: { value: '  检查消息连接  ' },
        });
        fireEvent.click(screen.getByRole('button', { name: '开始诊断' }));
        expect(onStart).toHaveBeenCalledWith('builtin', '10001', '检查消息连接');
        expect(onDiscover).not.toHaveBeenCalled();
    });
    it('快捷问题直接创建并发送，不要求先填输入框', () => {
        render(<Form />);
        fireEvent.click(screen.getByRole('button', { name: '帮我检查当前运行状态' }));
        expect(onStart).toHaveBeenCalledWith('builtin', '10001', '帮我检查当前运行状态');
    });
    it('缺 Key 时展示首次配置，不会误创建会话', () => {
        render(<Form status={{ hasApiKey: false, configured: false, error: null }} />);
        expect(screen.getByRole('region', { name: '连接模型' })).toBeVisible();
        expect(screen.queryByRole('textbox', { name: '描述问题' })).not.toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: '更多设置' }));
        expect(onSettings).toHaveBeenCalledOnce();
        expect(onStart).not.toHaveBeenCalled();
    });
    it('助手下拉按用户选择切换，保留当前问题和目标 Bot', async () => {
        const user = userEvent.setup();
        render(<Form />);
        fireEvent.change(screen.getByRole('textbox', { name: '描述问题' }), {
            target: { value: '检查当前状态' },
        });
        await user.click(screen.getByRole('button', { name: '目标 Bot：测试 Bot' }));
        await user.click(screen.getByRole('menuitemradio', { name: /备用 Bot/ }));
        await user.click(screen.getByRole('button', { name: '助手：内置 · chosen-model' }));
        expect(onDiscover).toHaveBeenCalledOnce();
        await user.click(screen.getByRole('menuitemradio', { name: 'DSH' }));
        fireEvent.click(screen.getByRole('button', { name: '开始诊断' }));
        expect(onStart).toHaveBeenLastCalledWith('dsh', '10002', '检查当前状态');
        await user.click(screen.getByRole('button', { name: '助手：DSH' }));
        await user.click(screen.getByRole('menuitemradio', { name: /内置助手/ }));
        fireEvent.click(screen.getByRole('button', { name: '开始诊断' }));
        expect(onStart).toHaveBeenLastCalledWith('builtin', '10002', '检查当前状态');
    });
    it('自定义外部Agent仍需显式允许其内建工具', async () => {
        const user = userEvent.setup();
        render(<Form />);
        await user.click(screen.getByRole('button', { name: '助手：内置 · chosen-model' }));
        await user.click(screen.getByRole('menuitemradio', { name: '自定义 ACP Agent' }));
        fireEvent.change(screen.getByRole('textbox', { name: '描述问题' }), {
            target: { value: '检查状态' },
        });
        expect(screen.getByRole('button', { name: '开始诊断' })).toBeDisabled();
        expect(screen.getByText('此 Agent 需要在设置中允许内建工具后使用。')).toBeVisible();
        fireEvent.keyDown(screen.getByRole('textbox', { name: '描述问题' }), { key: 'Enter' });
        expect(onStart).not.toHaveBeenCalled();
    });
    it('未安装的外部 Agent 可查看原因并进入设置，不能发送', async () => {
        const user = userEvent.setup();
        render(<Form dshInstalled={false} />);
        await user.click(screen.getByRole('button', { name: '助手：内置 · chosen-model' }));
        await user.click(screen.getByRole('menuitemradio', { name: /DSH/ }));
        expect(
            screen.getByText('未检测到此 Agent，请在设置中检查安装或可执行文件路径。'),
        ).toBeVisible();
        expect(screen.getByRole('button', { name: '开始诊断' })).toBeDisabled();
        fireEvent.click(screen.getByRole('button', { name: '打开设置' }));
        expect(onSettings).toHaveBeenCalledOnce();
        expect(onStart).not.toHaveBeenCalled();
    });
    it('无 Bot 时提供添加入口，创建期间禁用重复发送与选择器', () => {
        const page = render(<Form noBots />);
        fireEvent.click(screen.getByRole('button', { name: '前往机器人' }));
        expect(onBots).toHaveBeenCalledOnce();
        page.rerender(<Form creating />);
        expect(screen.getByRole('textbox', { name: '描述问题' })).toBeDisabled();
        expect(screen.getByRole('button', { name: '目标 Bot：测试 Bot' })).toBeDisabled();
        expect(screen.getByRole('button', { name: '开始诊断' })).toBeDisabled();
        expect(onStart).not.toHaveBeenCalled();
    });
});
