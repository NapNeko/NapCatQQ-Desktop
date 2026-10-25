// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useState } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { appFrameworkService } from '../../../../core/services/app-framework.service';
import { preferencesStore } from '../../../../core/domain/settings/preferencesStore';
import type { AppInstance } from '../../../../core/ipc/types';
import { NeoBotPanelContext } from '../../../../hooks/apps/neoBotPanelContext';
import { NeoBotDraftContext } from '../../../../hooks/apps/neoBotDraftContext';
import { NeoBotRuntimeTab } from './NeoBotRuntimeTab';
import { NeoBotScheduledTab } from './NeoBotScheduledTab';
import { PanelCredentialCard } from './PanelCredentialCard';
import { TooltipProvider } from '../../../../shared/ui';

vi.mock('../../../../core/services/app-framework.service', () => ({
    appFrameworkService: {
        panelCall: vi.fn(),
        panelPasswordSet: vi.fn(),
        setPanelPassword: vi.fn(),
    },
}));

const instance: AppInstance = {
    id: 'preview',
    framework_id: 'neobot',
    display_name: 'NeoBot',
    placement: 'local_native',
    host_id: 'local',
    install_dir: 'D:/preview',
    port: 8080,
    state: 'running',
    installed_version: '1.2.4a1',
    created_at_ms: 0,
    install_renderer: false,
    origin: 'created',
    auto_start: false,
};
const clients: QueryClient[] = [];
let motionEnabled: boolean;

function mount(node: React.ReactNode, goTab = vi.fn()) {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
    clients.push(client);
    return render(
        <QueryClientProvider client={client}>
            <TooltipProvider>
                <NeoBotPanelContext.Provider
                    value={{
                        instance,
                        loginSupported: true,
                        passwordRemembered: false,
                        onGoTab: goTab,
                        onOpenWebUi: vi.fn(),
                    }}
                >
                    {node}
                </NeoBotPanelContext.Provider>
            </TooltipProvider>
        </QueryClientProvider>,
    );
}

beforeEach(() => {
    vi.resetAllMocks();
    motionEnabled = preferencesStore.get().motionEnabled;
    preferencesStore.setMotionEnabled(false);
    vi.mocked(appFrameworkService.panelPasswordSet).mockResolvedValue(true);
    vi.mocked(appFrameworkService.setPanelPassword).mockResolvedValue(undefined);
});
afterEach(() => {
    cleanup();
    clients.splice(0).forEach((client) => client.clear());
    preferencesStore.setMotionEnabled(motionEnabled);
});

describe('多请求页面恢复入口', () => {
    it('六条请求均未授权时仅显示一次提示，并能进入凭据页', async () => {
        vi.mocked(appFrameworkService.panelCall).mockResolvedValue({
            kind: 'unauthorized',
            message: '',
        });
        const goTab = vi.fn();
        mount(<NeoBotRuntimeTab instanceId={instance.id} onGoTab={goTab} />, goTab);
        expect(await screen.findByText('需要面板密码')).toBeVisible();
        expect(screen.getAllByText('需要面板密码')).toHaveLength(1);
        expect(screen.queryByText('系统资源')).not.toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: '填写面板密码' }));
        expect(goTab).toHaveBeenCalledWith('console');
    });

    it('请求抛异常时显示重试入口，不停留在加载中', async () => {
        vi.mocked(appFrameworkService.panelCall).mockRejectedValue(new Error('连接中断'));
        mount(<NeoBotRuntimeTab instanceId={instance.id} onGoTab={vi.fn()} />);
        expect(await screen.findByText('无法读取面板')).toBeVisible();
        expect(screen.getByRole('button', { name: '刷新' })).toBeVisible();
        expect(screen.queryByText('正在读取面板…')).not.toBeInTheDocument();
    });

    it('局部成功时保留正常小节', async () => {
        vi.mocked(appFrameworkService.panelCall).mockImplementation(async (_id, _method, path) =>
            path === '/api/system'
                ? { kind: 'ok', data: { cpu_percent: 25, mem_percent: 30, disk_percent: 40 } }
                : { kind: 'unauthorized', message: '' },
        );
        mount(<NeoBotRuntimeTab instanceId={instance.id} onGoTab={vi.fn()} />);
        expect(await screen.findByText('系统资源')).toBeVisible();
        expect(screen.getByText('CPU')).toBeVisible();
        expect(screen.getAllByText('需要面板密码').length).toBeGreaterThan(0);
    });

    it('运行时不可用仍可重启进程，任务名称与内存支持面板返回字段', async () => {
        vi.mocked(appFrameworkService.panelCall).mockImplementation(async (_id, _method, path) => ({
            kind: 'ok',
            data:
                path === '/api/admin/power'
                    ? { available: false, transition: false }
                    : path === '/api/system'
                      ? { mem_used_mb: 50, mem_total_mb: 200 }
                      : path === '/api/tasks'
                        ? { scheduled: [{ title: '每日提醒', state: 'active', task_id: 't1' }] }
                        : {},
        }));
        mount(<NeoBotRuntimeTab instanceId={instance.id} onGoTab={vi.fn()} />);
        expect(await screen.findByRole('button', { name: '重启进程' })).toBeEnabled();
        expect(screen.getByRole('button', { name: '软重启运行' })).toBeDisabled();
        expect(screen.getByText('25')).toBeVisible();
        expect(screen.getByText('每日提醒')).toBeVisible();
    });
});

it('定时任务切页保留非法元数据草稿并继续禁用保存', async () => {
    vi.mocked(appFrameworkService.panelCall).mockResolvedValue({ kind: 'ok', data: { tasks: [] } });
    const store = {
        instanceId: instance.id,
        values: new Map<string, unknown>([
            [
                'scheduled:draft',
                {
                    title: '提醒',
                    start_at: '2026-10-24T10:00',
                    end_at: '2026-10-24T11:00',
                    bindings: [{ kind: 'group', id: '10001' }],
                    metadata: {},
                },
            ],
        ]),
    };
    function Harness() {
        const [visible, setVisible] = useState(true);
        return (
            <NeoBotDraftContext.Provider value={store}>
                <button onClick={() => setVisible(!visible)}>切换任务页</button>
                {visible && <NeoBotScheduledTab instanceId={instance.id} onGoTab={vi.fn()} />}
            </NeoBotDraftContext.Provider>
        );
    }
    mount(<Harness />);
    fireEvent.click(await screen.findByRole('button', { name: '高级元数据' }));
    fireEvent.change(screen.getByRole('textbox', { name: '元数据' }), {
        target: { value: '{broken' },
    });
    expect(screen.getByRole('button', { name: '保存任务' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: '切换任务页' }));
    fireEvent.click(screen.getByRole('button', { name: '切换任务页' }));
    fireEvent.click(await screen.findByRole('button', { name: '高级元数据' }));
    expect(screen.getByRole('textbox', { name: '元数据' })).toHaveValue('{broken');
    expect(screen.getByRole('button', { name: '保存任务' })).toBeDisabled();
});

it('验证期间不能保存或清除密码，验证完成后恢复操作', async () => {
    let finish!: (result: { kind: 'ok'; data: object }) => void;
    vi.mocked(appFrameworkService.panelCall).mockImplementation(async (_id, _method, path) =>
        path === '/api/auth/status'
            ? { kind: 'ok', data: { configured: true, setup_allowed: false } }
            : new Promise((resolve) => {
                  finish = resolve;
              }),
    );
    mount(<PanelCredentialCard instance={instance} onOpenWebUi={vi.fn()} />);
    const verify = await screen.findByRole('button', { name: '验证已存密码' });
    fireEvent.change(screen.getByLabelText('面板密码'), { target: { value: 'preview-only' } });
    fireEvent.click(verify);
    await waitFor(() => expect(screen.getByRole('button', { name: '保存并验证' })).toBeDisabled());
    expect(screen.getByRole('button', { name: '清除已存密码' })).toBeDisabled();
    expect(appFrameworkService.setPanelPassword).not.toHaveBeenCalled();
    finish({ kind: 'ok', data: {} });
    expect(await screen.findByText('验证成功，面板密码可用。')).toBeVisible();
    expect(screen.getByRole('button', { name: '保存并验证' })).toBeEnabled();
});
