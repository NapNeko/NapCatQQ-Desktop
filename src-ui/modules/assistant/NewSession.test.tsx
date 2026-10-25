// @vitest-environment jsdom

import { useState } from 'react';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type {
    AssistantModelStatus,
    AssistantProvider,
    AssistantSettings,
} from '../../core/domain/assistant/types';
import { preferencesStore } from '../../core/domain/settings/preferencesStore';
import { NewSession } from './NewSession';

const onCreate = vi.fn();
const onSettings = vi.fn();
const settings: AssistantSettings = {
    defaultProvider: 'builtin',
    builtin: {
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
}: {
    status?: AssistantModelStatus;
    noProviders?: boolean;
}) {
    const [provider, setProvider] = useState<AssistantProvider | null>(null);
    const [externalOpen, setExternalOpen] = useState(false);
    return (
        <NewSession
            bots={[{ id: '10001', name: '测试 Bot' }]}
            botId="10001"
            provider={provider}
            settings={settings}
            providers={
                noProviders
                    ? []
                    : [
                          {
                              provider: 'dsh',
                              label: 'DSH',
                              installed: true,
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
            modelError={null}
            loading={false}
            providersLoading={noProviders}
            creating={false}
            externalOpen={externalOpen}
            onExternalOpen={setExternalOpen}
            onBot={vi.fn()}
            onProvider={setProvider}
            onCreate={onCreate}
            onSettings={onSettings}
            onBots={vi.fn()}
        />
    );
}
beforeAll(() => {
    Element.prototype.hasPointerCapture ??= () => false;
    Element.prototype.releasePointerCapture ??= () => {};
    Element.prototype.scrollIntoView ??= () => {};
});
beforeEach(() => {
    onCreate.mockReset();
    onSettings.mockReset();
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
    it('已配置时直接创建Builtin会话，外部CLI尚未发现也不阻塞', () => {
        render(<Form noProviders />);
        expect(screen.getByText('内置助手')).toBeVisible();
        expect(screen.queryByRole('combobox', { name: '外部 Agent' })).not.toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: '开始诊断' }));
        expect(onCreate).toHaveBeenCalledWith('builtin', '10001');
    });
    it('缺Key时保持明确的模型设置入口，不会误创建会话', () => {
        render(<Form status={{ hasApiKey: false, configured: false, error: null }} />);
        expect(screen.getByRole('button', { name: '开始诊断' })).toBeDisabled();
        fireEvent.click(screen.getByRole('button', { name: '配置模型' }));
        expect(onSettings).toHaveBeenCalledOnce();
        expect(onCreate).not.toHaveBeenCalled();
    });
    it('外部Agent按用户选择展开，可再切回默认内置助手', async () => {
        const user = userEvent.setup();
        render(<Form />);
        fireEvent.click(screen.getByRole('button', { name: '使用外部 Agent' }));
        await user.click(screen.getByRole('combobox', { name: '外部 Agent' }));
        await user.click(screen.getByRole('option', { name: 'DSH' }));
        fireEvent.click(screen.getByRole('button', { name: '开始诊断' }));
        expect(onCreate).toHaveBeenLastCalledWith('dsh', '10001');
        fireEvent.click(screen.getByRole('button', { name: '改用内置助手' }));
        fireEvent.click(screen.getByRole('button', { name: '开始诊断' }));
        expect(onCreate).toHaveBeenLastCalledWith('builtin', '10001');
    });
    it('自定义外部Agent仍需显式允许其内建工具', async () => {
        const user = userEvent.setup();
        render(<Form />);
        fireEvent.click(screen.getByRole('button', { name: '使用外部 Agent' }));
        await user.click(screen.getByRole('combobox', { name: '外部 Agent' }));
        await user.click(screen.getByRole('option', { name: '自定义 ACP Agent' }));
        expect(screen.getByRole('button', { name: '开始诊断' })).toBeDisabled();
        expect(screen.getByText('此 Agent 需要在设置中允许内建工具后使用。')).toBeVisible();
    });
});
