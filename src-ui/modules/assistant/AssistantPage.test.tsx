// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { preferencesStore } from '../../core/domain/settings/preferencesStore';
import { assistantService } from '../../core/services/assistant.service';
import { assistantStore } from '../../hooks/assistant/assistantStore';
import { AssistantPage } from './AssistantPage';

const bots = vi.hoisted(() => ({
    snapshots: [
        { bot_id: '10001', state: 'running' },
        { bot_id: '10002', state: 'stopped' },
    ],
    configs: { '10001': { bot: { name: '预览 Bot' } }, '10002': { bot: { name: '备用 Bot' } } },
}));
vi.mock('../../hooks/bot/useBotSnapshots', () => ({
    useBotSnapshots: () => ({
        data: bots.snapshots,
        isPending: false,
        error: null,
        refetch: vi.fn(),
    }),
}));
vi.mock('../../hooks/bot/useBotConfigsMap', () => ({ useBotConfigsMap: () => bots.configs }));

beforeAll(() => {
    Element.prototype.hasPointerCapture ??= () => false;
    Element.prototype.releasePointerCapture ??= () => {};
    Element.prototype.scrollIntoView ??= () => {};
});
afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    act(() => preferencesStore.reset());
});

describe('诊断助手页面与 browser mock 集成', () => {
    it('创建、审批、追问、停止、保留草稿、恢复与设置均走实际前端契约', async () => {
        const warnings = vi.spyOn(console, 'warn');
        const providerProbe = vi.spyOn(assistantService, 'providers');
        act(() => {
            preferencesStore.reset();
            preferencesStore.setMotionEnabled(false);
        });
        const client = new QueryClient({
            defaultOptions: {
                queries: { retry: false, gcTime: 0, refetchOnWindowFocus: false },
                mutations: { retry: false },
            },
        });
        const mount = () =>
            render(
                <QueryClientProvider client={client}>
                    <AssistantPage onNavigate={vi.fn()} />
                </QueryClientProvider>,
            );
        let page = mount();
        const user = userEvent.setup();
        expect(
            await screen.findByText('当前为浏览器界面演示，不会调用真实模型或操作 Bot。'),
        ).toBeVisible();
        const openSessions = screen.getByRole('button', { name: '显示诊断会话列表' });
        fireEvent.click(openSessions);
        expect(screen.getByRole('button', { name: '收起会话列表' })).toHaveFocus();
        expect(openSessions.closest('section')).toHaveAttribute('inert');
        fireEvent.keyDown(screen.getByRole('button', { name: '收起会话列表' }), {
            key: 'Escape',
        });
        expect(openSessions).toHaveFocus();
        expect(openSessions.closest('section')).not.toHaveAttribute('inert');
        const modelSetup = await screen.findByRole('form', { name: '连接模型' });
        expect(within(modelSetup).getByRole('button', { name: '保存并继续' })).toBeDisabled();
        fireEvent.change(within(modelSetup).getByRole('textbox', { name: '服务地址' }), {
            target: { value: 'https://model.example/v1' },
        });
        fireEvent.change(within(modelSetup).getByRole('textbox', { name: '模型名称' }), {
            target: { value: 'mock-model' },
        });
        const keyInput = within(modelSetup).getByLabelText('API Key');
        expect(keyInput).toBeEnabled();
        fireEvent.change(keyInput, { target: { value: 'mock-secret-only-for-test' } });
        fireEvent.click(within(modelSetup).getByRole('button', { name: '保存并继续' }));
        const firstQuestion = await screen.findByRole('textbox', { name: '描述问题' });
        expect(screen.queryByRole('form', { name: '连接模型' })).not.toBeInTheDocument();
        expect(
            JSON.stringify(
                client
                    .getQueryCache()
                    .getAll()
                    .map((query) => query.state.data),
            ),
        ).not.toContain('mock-secret-only-for-test');
        expect(
            JSON.stringify(
                client
                    .getMutationCache()
                    .getAll()
                    .map((mutation) => mutation.state.variables),
            ),
        ).not.toContain('mock-secret-only-for-test');
        expect(providerProbe).not.toHaveBeenCalled();
        fireEvent.change(firstQuestion, { target: { value: '检查当前运行状态' } });
        fireEvent.click(screen.getByRole('button', { name: '开始诊断' }));
        await screen.findByRole('textbox', { name: '输入诊断问题' });
        expect(screen.getByRole('heading', { name: 'Bot-01' })).toBeVisible();
        expect(assistantStore.getSnapshot().infos[0]?.provider).toBe('builtin');
        await screen.findByRole('button', { name: '发送' }, { timeout: 3_000 });
        expect(
            assistantStore.getSnapshot().sessions[assistantStore.getSnapshot().selectedId!]!
                .messages,
        ).toContainEqual(expect.objectContaining({ role: 'user', text: '检查当前运行状态' }));

        fireEvent.change(screen.getByRole('textbox', { name: '输入诊断问题' }), {
            target: { value: '请重启' },
        });
        fireEvent.click(screen.getByRole('button', { name: '发送' }));
        const permission = await screen.findByRole('region', { name: '操作审批' });
        expect(within(permission).getByText('NCD 操作审批')).toBeVisible();
        expect(within(permission).getByText(/界面预览不会执行此操作/)).toBeVisible();
        fireEvent.click(within(permission).getByRole('button', { name: '仅允许这一次' }));
        await screen.findByText('界面样例：你已批准本次操作。预览不会真的重启 Bot。');
        await screen.findByRole('button', { name: '发送' }, { timeout: 3_000 });
        expect(screen.queryByRole('region', { name: '操作审批' })).not.toBeInTheDocument();

        fireEvent.change(screen.getByRole('textbox', { name: '输入诊断问题' }), {
            target: { value: '请追问我' },
        });
        fireEvent.click(screen.getByRole('button', { name: '发送' }));
        const question = await screen.findByRole('form', { name: '补充信息' });
        await user.click(within(question).getByRole('combobox', { name: /^出现时间/ }));
        await user.click(screen.getByRole('option', { name: '更新之后' }));
        fireEvent.click(within(question).getByRole('button', { name: '提交' }));
        await screen.findByText('界面样例：已收到补充信息：更新之后。');
        await screen.findByRole('button', { name: '发送' }, { timeout: 3_000 });

        fireEvent.change(screen.getByRole('textbox', { name: '输入诊断问题' }), {
            target: { value: '继续检查' },
        });
        fireEvent.click(screen.getByRole('button', { name: '发送' }));
        fireEvent.click(await screen.findByRole('button', { name: '停止' }));
        await screen.findByText('已停止本轮界面演示。');
        await screen.findByRole('button', { name: '恢复并发送' }, { timeout: 3_000 });

        fireEvent.change(screen.getByRole('textbox', { name: '输入诊断问题' }), {
            target: { value: '尚未发送的草稿' },
        });
        page.unmount();
        page = mount();
        expect(await screen.findByRole('textbox', { name: '输入诊断问题' })).toHaveValue(
            '尚未发送的草稿',
        );
        expect(screen.getByText('界面样例：已收到补充信息：更新之后。')).toBeVisible();

        const selected = assistantStore.getSnapshot().selectedId!;
        const current = await assistantService.getSession(selected);
        vi.spyOn(assistantService, 'getSession').mockResolvedValueOnce({
            ...current,
            info: { ...current.info, state: 'failed', error: '测试注入：连接中断' },
        });
        await act(async () => {
            await assistantStore.refresh();
        });
        expect(await screen.findByText('测试注入：连接中断')).toBeVisible();
        fireEvent.click(screen.getByRole('button', { name: '恢复并发送' }));
        await screen.findByRole('button', { name: '发送' }, { timeout: 3_000 });
        expect(assistantStore.getSnapshot().selectedId).toBe(selected);

        fireEvent.click(screen.getByRole('button', { name: '助手设置' }));
        const dialog = await screen.findByRole('dialog', { name: '助手设置' });
        expect(
            within(dialog).getByText('保存或清除 API Key 会停止内置会话，历史会保留。'),
        ).toBeVisible();
        const history = assistantStore
            .getSnapshot()
            .sessions[selected]!.messages.filter((message) => message.role !== 'system')
            .map((message) => message.id);
        fireEvent.change(within(dialog).getByLabelText('API Key'), {
            target: { value: 'rotated-mock-secret-only-for-test' },
        });
        fireEvent.click(within(dialog).getByRole('button', { name: '保存助手设置' }));
        await within(dialog).findByText('API Key 已保存。');
        expect(assistantStore.getSnapshot().sessions[selected]!.info.state).toBe('disconnected');
        fireEvent.click(within(dialog).getByRole('button', { name: '清除 Key' }));
        await within(dialog).findByText('API Key 已清除。');
        expect(assistantStore.getSnapshot().sessions[selected]!.info.state).toBe('disconnected');
        expect(
            assistantStore
                .getSnapshot()
                .sessions[selected]!.messages.filter((message) => message.role !== 'system')
                .map((message) => message.id),
        ).toEqual(history);
        fireEvent.click(await within(dialog).findByRole('button', { name: '外部 Agent（可选）' }));
        fireEvent.click(await within(dialog).findByRole('button', { name: /^DSH/ }));
        fireEvent.change(within(dialog).getByRole('textbox', { name: '可执行文件路径' }), {
            target: { value: 'C:\\preview\\dsh.cmd' },
        });
        fireEvent.change(within(dialog).getByRole('textbox', { name: '附加参数（每行一个）' }), {
            target: { value: '--model\ntest model' },
        });
        fireEvent.click(within(dialog).getByRole('button', { name: '保存助手设置' }));
        await waitFor(() =>
            expect(within(dialog).getByRole('button', { name: '保存助手设置' })).toBeDisabled(),
        );
        expect(
            (await assistantService.settings()).providers.find(
                (provider) => provider.provider === 'dsh',
            ),
        ).toMatchObject({
            executablePath: 'C:\\preview\\dsh.cmd',
            extraArgs: ['--model', 'test model'],
            allowNativeTools: false,
        });
        await user.keyboard('{Escape}');
        await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
        await user.click(screen.getByRole('button', { name: '会话操作' }));
        await user.click(screen.getByRole('menuitem', { name: '关闭会话' }));
        await screen.findByText('此会话已关闭，记录仍可查看。');
        expect(screen.queryByRole('textbox', { name: '输入诊断问题' })).not.toBeInTheDocument();
        expect(warnings.mock.calls.flat().join(' ')).not.toContain(
            'changing from uncontrolled to controlled',
        );
        page.unmount();
        client.clear();
    }, 15_000);
});
