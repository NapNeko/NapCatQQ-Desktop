// @vitest-environment jsdom
import { StrictMode, type ReactNode } from 'react';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { BotConfig } from '../../core/ipc/generated/domain/BotConfig';
import { featureTogglesStore } from '../../hooks/preferences/featureTogglesStore';
import { clearBotTourBridge } from '../../hooks/desktop/botTourBridge';
import {
    consumePendingBotCreateRequest,
    getPendingBotCreateRequest,
    openCreateDockerBot,
} from '../../hooks/bot/botCreateNavigation';
import { BotPageNext } from './BotPage.next';

type PendingAction = () => void | Promise<void>;
interface ConfigPreviewProps {
    initialConfig?: BotConfig | null;
    botId: string | null;
    onBack: () => void;
    onSavedStay?: (id: string) => void;
}

const fixture = vi.hoisted(() => ({
    serverState: { servers: [] as Array<{ id: string }>, isLoading: false },
    consent: vi.fn<(action?: PendingAction) => Promise<boolean>>(),
    pushInfo: vi.fn(),
    pushError: vi.fn(),
    configProps: vi.fn(),
}));

vi.mock('../../hooks/remote/useServerManager', () => ({
    useServerManager: () => fixture.serverState,
}));
vi.mock('../../hooks/desktop/desktopConsentHost', () => ({
    requestDesktopConsent: fixture.consent,
}));
vi.mock('../../hooks/ui/globalInfoBarStore', () => ({ pushInfoBar: fixture.pushInfo }));
vi.mock('../../hooks/ui/pushErrorBar', () => ({ pushErrorBar: fixture.pushError }));
vi.mock('../../shared/ui', () => ({
    Button: ({ children, onClick }: { children: ReactNode; onClick?: () => void }) => (
        <button onClick={onClick}>{children}</button>
    ),
}));
vi.mock('../../shared/ui/RouteErrorBoundary', () => ({
    RouteErrorBoundary: ({ children }: { children: ReactNode }) => children,
}));
vi.mock('../../shared/ui/motion', async () => {
    const { useEffect } = await import('react');
    function PageTransition({
        visible,
        onExited,
        children,
    }: {
        visible: boolean;
        onExited: () => void;
        children: ReactNode;
    }) {
        useEffect(() => {
            if (!visible) onExited();
        }, [visible, onExited]);
        return <div>{children}</div>;
    }
    return { PageTransition };
});
vi.mock('./list/BotListPage.next', () => ({
    BotListPageNext: ({ onConfigureBot }: { onConfigureBot: (id: string | null) => void }) => (
        <button onClick={() => onConfigureBot(null)}>普通新建</button>
    ),
}));
vi.mock('./config/BotConfigPage.next', () => ({
    BotConfigPageNext: (props: ConfigPreviewProps) => {
        fixture.configProps(props);
        return (
            <section aria-label="配置预览">
                <output data-testid="initial-config">
                    {JSON.stringify(props.initialConfig ?? null)}
                </output>
                <output data-testid="editing-bot">{props.botId ?? ''}</output>
                <button onClick={props.onBack}>返回列表</button>
                <button onClick={() => props.onSavedStay?.('9988')}>保存当前配置</button>
            </section>
        );
    },
}));
vi.mock('./log/BotLogPage.next', () => ({ BotLogPageNext: () => null }));
vi.mock('./metrics/BotRuntimeMetricsPage.next', () => ({ BotRuntimeMetricsPageNext: () => null }));

function deferred<T>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>((done) => {
        resolve = done;
    });
    return { promise, resolve };
}

function holdConsent() {
    const gate = deferred<boolean>();
    fixture.consent.mockImplementationOnce(async (action) => {
        const accepted = await gate.promise;
        if (accepted) await action?.();
        return accepted;
    });
    return gate;
}

function clearPending(): void {
    const pending = getPendingBotCreateRequest();
    if (pending) consumePendingBotCreateRequest(pending.requestId);
}

function preset(): BotConfig | null {
    return JSON.parse(
        screen.getByTestId('initial-config').textContent ?? 'null',
    ) as BotConfig | null;
}

beforeEach(() => {
    clearPending();
    clearBotTourBridge();
    featureTogglesStore._reset();
    fixture.serverState = { servers: [{ id: 'one' }, { id: 'two' }], isLoading: false };
    fixture.consent.mockReset();
    fixture.consent.mockImplementation(async (action) => {
        await action?.();
        return true;
    });
});

afterEach(() => {
    cleanup();
    clearPending();
    clearBotTourBridge();
    featureTogglesStore._reset();
});

describe('Docker 部署进入 Bot 新建配置', () => {
    it('等待异步主机清单后预填真实服务器 ID、框架和 Docker，账号仍为空', async () => {
        fixture.serverState = { servers: [], isLoading: true };
        openCreateDockerBot({ hostId: 'remote:two', flavor: 'snowluma' });
        const { rerender } = render(<BotPageNext />);
        expect(fixture.consent).not.toHaveBeenCalled();
        expect(screen.queryByRole('region', { name: '配置预览' })).toBeNull();

        fixture.serverState = { servers: [{ id: 'two' }], isLoading: false };
        rerender(<BotPageNext />);
        await waitFor(() => expect(screen.getByRole('region', { name: '配置预览' })).toBeTruthy());
        expect(preset()?.bot).toMatchObject({
            QQID: 0,
            name: '',
            runtime_target: 'two',
            backend_type: 'snowluma',
            deploymentType: 'docker',
        });
        expect(getPendingBotCreateRequest()).toBeNull();
    });

    it('主机已不存在时拒绝新建，不回退成本机或首台主机', async () => {
        openCreateDockerBot({ hostId: 'remote:deleted', flavor: 'napcat' });
        render(<BotPageNext />);
        await waitFor(() =>
            expect(fixture.pushInfo).toHaveBeenCalledWith(
                expect.objectContaining({ content: '目标远程主机已不存在，请重新选择主机。' }),
            ),
        );
        expect(fixture.consent).not.toHaveBeenCalled();
        expect(fixture.configProps).not.toHaveBeenCalled();
        expect(getPendingBotCreateRequest()).toBeNull();
    });

    it('等待清单期间框架被关闭时，不绕过功能开关打开配置', async () => {
        fixture.serverState = { servers: [], isLoading: true };
        openCreateDockerBot({ hostId: 'remote:one', flavor: 'napcat' });
        const { rerender } = render(<BotPageNext />);
        featureTogglesStore.apply({ napcat: false });
        fixture.serverState = { servers: [{ id: 'one' }], isLoading: false };
        rerender(<BotPageNext />);
        await waitFor(() =>
            expect(fixture.pushInfo).toHaveBeenCalledWith(
                expect.objectContaining({ content: '请先在设置中启用 NapCat。' }),
            ),
        );
        expect(fixture.consent).not.toHaveBeenCalled();
        expect(fixture.configProps).not.toHaveBeenCalled();
    });

    it('同意门未放行时不打开配置，后续普通新建不会继承 Docker 预填', async () => {
        fixture.consent.mockResolvedValueOnce(false);
        openCreateDockerBot({ hostId: 'remote:one', flavor: 'napcat' });
        render(<BotPageNext />);
        await waitFor(() => expect(fixture.consent).toHaveBeenCalledOnce());
        expect(screen.queryByRole('region', { name: '配置预览' })).toBeNull();
        fireEvent.click(screen.getByRole('button', { name: '普通新建' }));
        await waitFor(() => expect(screen.getByRole('region', { name: '配置预览' })).toBeTruthy());
        expect(preset()).toBeNull();
        expect(getPendingBotCreateRequest()).toBeNull();
    });

    it('旧主机的迟到门禁放行不能覆盖新主机的待创建请求', async () => {
        const firstGate = holdConsent();
        const secondGate = holdConsent();
        openCreateDockerBot({ hostId: 'remote:one', flavor: 'napcat' });
        render(<BotPageNext />);
        await waitFor(() => expect(fixture.consent).toHaveBeenCalledOnce());

        act(() => openCreateDockerBot({ hostId: 'remote:two', flavor: 'snowluma' }));
        await waitFor(() => expect(fixture.consent).toHaveBeenCalledTimes(2));
        await act(async () => {
            firstGate.resolve(true);
            await firstGate.promise;
        });
        expect(screen.queryByRole('region', { name: '配置预览' })).toBeNull();
        expect(getPendingBotCreateRequest()).toMatchObject({
            hostId: 'remote:two',
            flavor: 'snowluma',
        });

        await act(async () => {
            secondGate.resolve(true);
            await secondGate.promise;
        });
        await waitFor(() => expect(preset()?.bot.runtime_target).toBe('two'));
        expect(preset()?.bot.backend_type).toBe('snowluma');
        expect(getPendingBotCreateRequest()).toBeNull();
    });

    it('门禁等待期间主机被删除时，再次检查主机而不是打开过期配置', async () => {
        const gate = holdConsent();
        openCreateDockerBot({ hostId: 'remote:one', flavor: 'napcat' });
        const { rerender } = render(<BotPageNext />);
        await waitFor(() => expect(fixture.consent).toHaveBeenCalledOnce());
        fixture.serverState = { servers: [{ id: 'two' }], isLoading: false };
        rerender(<BotPageNext />);
        await act(async () => {
            gate.resolve(true);
            await gate.promise;
        });

        expect(fixture.configProps).not.toHaveBeenCalled();
        expect(fixture.pushInfo).toHaveBeenCalledWith(
            expect.objectContaining({ content: '目标远程主机已不存在，请重新选择主机。' }),
        );
        expect(getPendingBotCreateRequest()).toBeNull();
    });

    it('门禁等待期间关闭框架时，放行回调也不能绕过功能开关', async () => {
        const gate = holdConsent();
        openCreateDockerBot({ hostId: 'remote:one', flavor: 'napcat' });
        render(<BotPageNext />);
        await waitFor(() => expect(fixture.consent).toHaveBeenCalledOnce());
        featureTogglesStore.apply({ napcat: false });
        await act(async () => {
            gate.resolve(true);
            await gate.promise;
        });

        expect(fixture.configProps).not.toHaveBeenCalled();
        expect(fixture.pushInfo).toHaveBeenCalledWith(
            expect.objectContaining({ content: '请先在设置中启用 NapCat。' }),
        );
    });

    it('StrictMode 重放 effect 不会重复请求同意门或丢掉待创建配置', async () => {
        const gate = holdConsent();
        openCreateDockerBot({ hostId: 'remote:one', flavor: 'napcat' });
        render(
            <StrictMode>
                <BotPageNext />
            </StrictMode>,
        );
        await waitFor(() => expect(fixture.consent).toHaveBeenCalledOnce());
        await act(async () => {
            gate.resolve(true);
            await gate.promise;
        });
        await waitFor(() => expect(preset()?.bot.runtime_target).toBe('one'));
        expect(fixture.consent).toHaveBeenCalledOnce();
    });

    it('退出 Docker 新建后，普通新建不继承上一台主机或框架预填', async () => {
        openCreateDockerBot({ hostId: 'remote:two', flavor: 'snowluma' });
        render(<BotPageNext />);
        await waitFor(() => expect(preset()?.bot.runtime_target).toBe('two'));
        fireEvent.click(screen.getByRole('button', { name: '返回列表' }));
        await waitFor(() => expect(screen.getByRole('button', { name: '普通新建' })).toBeTruthy());
        fireEvent.click(screen.getByRole('button', { name: '普通新建' }));
        await waitFor(() => expect(screen.getByRole('region', { name: '配置预览' })).toBeTruthy());
        expect(preset()).toBeNull();
    });

    it('保存转入已有实例编辑后清理新建预填', async () => {
        openCreateDockerBot({ hostId: 'remote:one', flavor: 'napcat' });
        render(<BotPageNext />);
        await waitFor(() => expect(preset()?.bot.runtime_target).toBe('one'));
        fireEvent.click(screen.getByRole('button', { name: '保存当前配置' }));
        await waitFor(() => expect(screen.getByTestId('editing-bot').textContent).toBe('9988'));
        expect(preset()).toBeNull();
    });

    it('页面卸载后，迟到的放行不消费请求，也不创建配置', async () => {
        const gate = holdConsent();
        openCreateDockerBot({ hostId: 'remote:one', flavor: 'napcat' });
        const { unmount } = render(<BotPageNext />);
        await waitFor(() => expect(fixture.consent).toHaveBeenCalledOnce());
        const request = getPendingBotCreateRequest();
        unmount();
        await act(async () => {
            gate.resolve(true);
            await gate.promise;
        });
        expect(fixture.configProps).not.toHaveBeenCalled();
        expect(getPendingBotCreateRequest()).toBe(request);
    });
});
