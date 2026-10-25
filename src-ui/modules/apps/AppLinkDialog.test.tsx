import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { createDefaultBotConfig } from '../../core/domain/bot/config-defaults';
import { preferencesStore } from '../../core/domain/settings/preferencesStore';
import type { AppInstance, BotConfig, OneBotLinkPlan } from '../../core/ipc/types';
import { AppLinkDialog } from './AppLinkDialog';

const mocks = vi.hoisted(() => ({
    configs: {} as Record<string, BotConfig>,
    instances: [] as AppInstance[],
    plan: null as OneBotLinkPlan | null,
    previewError: null as string | null,
    applyError: null as string | null,
    preview: vi.fn(),
    retry: vi.fn(),
    apply: vi.fn(),
}));

vi.mock('../../hooks/bot/useBotSnapshots', () => ({
    useBotSnapshots: () => ({ data: [{ bot_id: '10001' }] }),
}));
vi.mock('../../hooks/bot/useBotConfigsMap', () => ({ useBotConfigsMap: () => mocks.configs }));
vi.mock('../../hooks/remote/useServerManager', () => ({
    useServerManager: () => ({
        servers: [
            { id: 'a', name: '服务器 A' },
            { id: 'b', name: '服务器 B' },
        ],
    }),
}));
vi.mock('../../hooks/apps/useAppInstances', () => ({
    useAppInstances: () => ({ instances: mocks.instances }),
    useAppFrameworks: () => ({ data: [{ id: 'karin', link_modes: ['reverse_ws'] }] }),
}));
vi.mock('../../hooks/apps/useAppLink', () => ({
    useAppLinkPlan: (...args: unknown[]) => {
        mocks.preview(...args);
        return {
            plan: args[1] ? mocks.plan : null,
            previewing: false,
            previewError: mocks.previewError,
            retryPreview: mocks.retry,
        };
    },
    useApplyAppLink: () => ({
        isPending: false,
        mutate: mocks.apply,
        isError: !!mocks.applyError,
        error: mocks.applyError,
        variables: { instanceId: 'app-1', botId: '10001' },
    }),
}));

beforeAll(() => {
    Element.prototype.hasPointerCapture ??= () => false;
    Element.prototype.releasePointerCapture ??= () => {};
    Element.prototype.scrollIntoView ??= () => {};
});

function instance(hostId = 'remote:a'): AppInstance {
    return {
        id: 'app-1',
        framework_id: 'karin',
        display_name: '应用一',
        placement: hostId === 'local' ? 'local_native' : 'remote_native',
        host_id: hostId,
        install_dir: '/apps/one',
        port: 8080,
        state: 'installed',
        created_at_ms: 0,
        install_renderer: false,
        origin: 'created',
        auto_start: false,
    };
}

function plan(mode: 'forward_ws' | 'reverse_ws' = 'reverse_ws'): OneBotLinkPlan {
    const common = {
        enable: true,
        name: 'ncd-app:app-1',
        messagePostFormat: 'array' as const,
        token: 'test-token',
        debug: false,
        reportSelfMessage: false,
        heartInterval: 30000,
        role: 'Universal' as const,
    };
    return {
        mode,
        bot_id: '10001',
        instance_id: 'app-1',
        access_token: 'test-token',
        app_side_writes: [{ path: '/apps/one/config.json', summary: '写入 OneBot 连接' }],
        connection:
            mode === 'reverse_ws'
                ? {
                      ...common,
                      kind: 'ws_client',
                      url: 'ws://172.18.0.1:18080/onebot',
                      reconnectInterval: 30000,
                  }
                : {
                      ...common,
                      kind: 'ws_server',
                      host: '0.0.0.0',
                      port: 3010,
                      path: '/',
                      role: 'Universal',
                      enableForcePushEvent: true,
                  },
    };
}

beforeEach(() => {
    preferencesStore.setMotionEnabled(false);
    mocks.preview.mockReset();
    mocks.retry.mockReset();
    mocks.apply.mockReset();
    const config = createDefaultBotConfig();
    config.bot = {
        ...config.bot,
        name: 'Docker Bot',
        QQID: 10001,
        runtime_target: 'a',
        deploymentType: 'docker',
    };
    mocks.configs = { '10001': config };
    mocks.instances = [instance()];
    mocks.plan = plan();
    mocks.previewError = null;
    mocks.applyError = null;
});

afterEach(() => {
    cleanup();
    preferencesStore.reset();
});

describe('AppLinkDialog Docker and cross-host links', () => {
    it('keeps Docker Bot selectable and requests its preview', async () => {
        const user = userEvent.setup();
        render(<AppLinkDialog open onOpenChange={() => {}} instanceId="app-1" />);
        await user.click(screen.getByRole('combobox', { name: '协议 Bot' }));
        const option = await screen.findByRole('option', { name: /Docker Bot/ });
        expect(option).not.toHaveAttribute('data-disabled');
        await user.click(option);
        expect(mocks.preview).toHaveBeenLastCalledWith('app-1', '10001', true);
        expect(screen.queryByText(/暂不支持对接/)).not.toBeInTheDocument();
        expect(screen.getByText(/仅绑定 Docker 网桥地址/)).toBeInTheDocument();
        expect(screen.getByRole('button', { name: '应用对接' })).toBeEnabled();
    });

    it('explains resident cross-server tunnels and applies the selected Docker plan', () => {
        mocks.instances = [instance('remote:b')];
        render(<AppLinkDialog open onOpenChange={() => {}} instanceId="app-1" botId="10001" />);
        expect(screen.getByText(/跨主机部分自动建立 SSH 隧道/)).toBeInTheDocument();
        expect(screen.getByText(/桌面端退出后仍保留/)).toHaveTextContent(
            '主机重启后需重新连接桌面端恢复隧道',
        );
        fireEvent.click(screen.getByRole('button', { name: '应用对接' }));
        expect(mocks.apply).toHaveBeenCalledWith(
            { instanceId: 'app-1', botId: '10001', connectionName: 'ncd-app:app-1' },
            expect.objectContaining({ onSuccess: expect.any(Function) }),
        );
    });

    it('explains published Docker ports and requires Desktop for a local application', () => {
        mocks.instances = [instance('local')];
        mocks.plan = plan('forward_ws');
        render(<AppLinkDialog open onOpenChange={() => {}} instanceId="app-1" botId="10001" />);
        expect(screen.getByText(/端口映射自动配置/)).toBeInTheDocument();
        expect(screen.getByText(/需保持桌面端在线/)).toHaveTextContent('退出后隧道断开');
        expect(screen.getByText(/新增发布端口时会重建/)).toHaveTextContent('数据卷保留');
    });

    it('shows the backend failure, disables apply and lets the user regenerate the plan', () => {
        mocks.plan = null;
        mocks.previewError = '服务器 B 的 SSH 登录失败，请检查主机档案';
        render(<AppLinkDialog open onOpenChange={() => {}} instanceId="app-1" botId="10001" />);
        expect(screen.getByRole('alert')).toHaveTextContent(mocks.previewError);
        expect(screen.getByRole('button', { name: '应用对接' })).toBeDisabled();
        fireEvent.click(screen.getByRole('button', { name: '重新生成计划' }));
        expect(mocks.retry).toHaveBeenCalledTimes(1);
    });

    it('keeps a concrete application failure visible in the dialog', () => {
        mocks.applyError = '无法建立服务器 B 的 SSH 隧道：认证失败';
        render(<AppLinkDialog open onOpenChange={() => {}} instanceId="app-1" botId="10001" />);
        expect(screen.getByRole('alert')).toHaveTextContent(mocks.applyError);
        expect(screen.getByRole('button', { name: '应用对接' })).toBeEnabled();
    });
});
