import { useState } from 'react';
import userEvent from '@testing-library/user-event';
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import gsap from 'gsap';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { BotBasicConfig } from '../../../../core/ipc/generated/domain/BotBasicConfig';
import { createDefaultBotConfig } from '../../../../core/domain/bot/config-defaults';
import { IdentityTab } from './IdentityTab';

const serverProfiles = vi.hoisted(() => ({
    data: [
        { id: 'linux', name: 'Linux', host: 'example.com', platform: 'linux' },
        { id: 'windows', name: 'Windows', host: 'windows.example.com', platform: 'windows' },
    ],
}));
vi.mock('../../../../hooks/remote/useServers', () => ({
    useServers: () => ({ data: serverProfiles.data, isLoading: false }),
}));
vi.mock('../../../../hooks/preferences/featureTogglesStore', () => ({
    useFeatures: () => ({ napcat: true, snowluma: true }),
}));
vi.mock('../../../../hooks/docker/useDockerHosts', () => ({
    useDockerHosts: () => ({ statusByHost: {}, probingByHost: {}, imageReadyByHost: {} }),
}));
vi.mock('../../../../hooks/components/useRuntimeReadiness', () => ({
    useRuntimeReadiness: () => ({ readiness: undefined, probing: false }),
    useComponentNames: () => ({}),
}));
vi.mock('../../../../hooks/remote/useIsHostReachable', () => ({
    useIsHostReachable: () => true,
}));
vi.mock('../../../../hooks/bot/useCronPreview', () => ({
    useCronPreview: () => ({ status: 'idle' }),
}));

function IdentityForm({ initialData }: { initialData?: BotBasicConfig } = {}) {
    const [data, setData] = useState(initialData ?? createDefaultBotConfig().bot);
    return (
        <IdentityTab
            data={data}
            onChange={(patch) => setData((previous) => ({ ...previous, ...patch }))}
            isEditMode={false}
            isRunning={false}
        />
    );
}

function finishMotion() {
    act(() => {
        gsap.globalTimeline.getChildren().forEach((animation) => animation.totalProgress(1));
    });
}

afterEach(() => {
    cleanup();
    serverProfiles.data[0].platform = 'linux';
});

describe('身份配置动态区域', () => {
    it('NC / SL 切换时启动模式有完整退出阶段', () => {
        render(<IdentityForm />);
        const backends = within(screen.getByRole('radiogroup', { name: '底座类型' }));
        expect(screen.queryByRole('radiogroup', { name: 'SnowLuma 启动模式' })).toBeNull();

        fireEvent.click(backends.getByRole('radio', { name: /^SnowLuma/ }));
        finishMotion();
        const modes = screen.getByRole('radiogroup', { name: 'SnowLuma 启动模式' });
        fireEvent.click(backends.getByRole('radio', { name: /^NapCat/ }));

        expect(modes).toBeInTheDocument();
        expect(modes.closest('[inert]')).not.toBeNull();
        expect(modes.closest('[aria-hidden="true"]')).not.toBeNull();
        finishMotion();
        expect(modes).not.toBeInTheDocument();
    });

    it('收起定时计划时先禁用区域，再在动画结束后移除输入', () => {
        render(<IdentityForm />);
        const schedule = screen.getByRole('switch', { name: /^定时自动重启/ });
        fireEvent.click(schedule);
        finishMotion();
        const duration = screen.getByLabelText('重启间隔');

        fireEvent.click(schedule);

        expect(duration).toBeInTheDocument();
        expect(duration.closest('[inert]')).not.toBeNull();
        finishMotion();
        expect(duration).not.toBeInTheDocument();
    });
});

it('新建容器 Bot 可选择 Podman，已有容器的引擎归属保持锁定', async () => {
    const user = userEvent.setup();
    const data = {
        ...createDefaultBotConfig().bot,
        deploymentType: 'docker' as const,
        runtime_target: 'linux',
    };
    const onEngine = vi.fn();
    const props = {
        data,
        onChange: vi.fn(),
        isEditMode: false,
        isRunning: false,
        containerEngine: 'podman' as const,
        onContainerEngineChange: onEngine,
    };
    const { rerender } = render(<IdentityTab {...props} />);
    finishMotion();
    expect(screen.getByRole('combobox', { name: '容器引擎' })).toHaveTextContent('Podman');
    await user.click(screen.getByRole('combobox', { name: '容器引擎' }));
    finishMotion();
    await user.click(await screen.findByRole('option', { name: 'Docker', exact: true }));
    expect(onEngine).toHaveBeenCalledWith('docker');
    rerender(<IdentityTab {...props} isEditMode containerEngineLocked />);
    expect(screen.getByRole('combobox', { name: '容器引擎' })).toBeDisabled();
});

it('再次点击已选中的远程模式不会清空目标主机', () => {
    render(
        <IdentityForm initialData={{ ...createDefaultBotConfig().bot, runtime_target: 'linux' }} />,
    );
    finishMotion();
    const selected = screen.getByRole('combobox', { name: '目标远程主机' });
    expect(selected).toHaveTextContent('Linux · example.com');
    fireEvent.click(
        within(screen.getByRole('radiogroup', { name: '运行宿主' })).getByRole('radio', {
            name: /^远程主机/,
        }),
    );
    expect(selected).toHaveTextContent('Linux · example.com');
});

it('平台探测确认为 Windows 后把容器草稿切回直接运行', () => {
    serverProfiles.data[0].platform = 'auto';
    const initialData = {
        ...createDefaultBotConfig().bot,
        runtime_target: 'linux',
        deploymentType: 'docker' as const,
    };
    const { rerender } = render(<IdentityForm initialData={initialData} />);
    finishMotion();
    expect(screen.getByRole('radio', { name: '容器', exact: true })).toHaveAttribute(
        'aria-checked',
        'true',
    );
    serverProfiles.data[0].platform = 'windows';
    rerender(<IdentityForm initialData={initialData} />);
    finishMotion();
    expect(screen.getByRole('radio', { name: '容器', exact: true })).toBeDisabled();
    expect(screen.getByRole('radio', { name: '直接运行', exact: true })).toHaveAttribute(
        'aria-checked',
        'true',
    );
});
