import { useState } from 'react';
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import gsap from 'gsap';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createDefaultBotConfig } from '../../../../core/domain/bot/config-defaults';
import { IdentityTab } from './IdentityTab';

vi.mock('../../../../hooks/remote/useServerManager', () => ({
    useServerManager: () => ({
        servers: [{ id: 'linux', name: 'Linux', host: 'example.com' }],
        isLoading: false,
    }),
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

function IdentityForm() {
    const [data, setData] = useState(createDefaultBotConfig().bot);
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

afterEach(cleanup);

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
