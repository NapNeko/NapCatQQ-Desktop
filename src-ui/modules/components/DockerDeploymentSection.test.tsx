import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render as renderUI, screen } from '@testing-library/react';
import { preferencesStore } from '../../core/domain/settings/preferencesStore';
import {
    DockerDeploymentSection,
    type DockerDeploymentSectionProps,
} from './DockerDeploymentSection';

function render(ui: ReactNode) {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
    return renderUI(<QueryClientProvider client={client}>{ui}</QueryClientProvider>);
}

const navigation = vi.hoisted(() => ({ create: vi.fn(), manage: vi.fn() }));
const engine = vi.hoisted(() => ({
    engine: 'docker' as 'docker' | 'podman',
    loading: false,
    switching: false,
    pendingEngine: undefined as 'docker' | 'podman' | undefined,
    error: null,
    select: vi.fn(),
    refetch: vi.fn(),
}));
vi.mock('../../hooks/docker/useContainerEngine', () => ({ useContainerEngine: () => engine }));

vi.mock('../../hooks/bot/botCreateNavigation', () => ({
    openCreateDockerBot: navigation.create,
}));
vi.mock('../../hooks/docker/dockerNavigation', () => ({
    openDockerManagement: navigation.manage,
}));
vi.mock('../../hooks/useOpenExternal', () => ({ useOpenExternal: () => vi.fn() }));

beforeEach(() => {
    preferencesStore.setMotionEnabled(false);
    engine.engine = 'docker';
    engine.loading = false;
    engine.switching = false;
    engine.pendingEngine = undefined;
    engine.select.mockReset().mockResolvedValue(undefined);
});
afterEach(() => {
    cleanup();
    preferencesStore.reset();
});

function props(
    overrides: Partial<DockerDeploymentSectionProps> = {},
): DockerDeploymentSectionProps {
    return {
        hostId: 'remote:second-host',
        hostLabel: '第二台主机',
        os: 'linux',
        status: {
            installed: true,
            daemonRunning: true,
            composeAvailable: true,
            version: '27.3.1',
        },
        isProbing: false,
        isInstalling: false,
        flavors: ['napcat'],
        imageReadyByFlavor: { napcat: false },
        isPullingImage: () => false,
        onPullImage: vi.fn(),
        onInstall: vi.fn(),
        onRetry: vi.fn(),
        onOpenDownload: vi.fn(),
        ...overrides,
    };
}

describe('Docker 部署区', () => {
    it.each([
        ['安装 Docker', { installed: false, daemonRunning: false, composeAvailable: false }],
        ['启动 Docker', { installed: true, daemonRunning: false, composeAvailable: true }],
    ] as const)('根据主机状态提供 %s 动作', (actionLabel, state) => {
        const input = props({ status: { ...state, version: '27.3.1' } });
        render(<DockerDeploymentSection {...input} />);
        fireEvent.click(screen.getByRole('button', { name: actionLabel }));
        expect(input.onInstall).toHaveBeenCalledOnce();
        expect(screen.getByRole('button', { name: '新建 NapCat Bot' })).toBeDisabled();
    });

    it('探测失败只能重试，不能把未知状态当作未安装', () => {
        const input = props({ status: undefined, probeError: new Error('SSH 不可达') });
        render(<DockerDeploymentSection {...input} />);
        expect(screen.getByText('探测失败')).toBeInTheDocument();
        expect(screen.queryByRole('button', { name: '安装 Docker' })).toBeNull();
        fireEvent.click(screen.getByRole('button', { name: '重新探测' }));
        expect(input.onRetry).toHaveBeenCalledOnce();
        expect(input.onInstall).not.toHaveBeenCalled();
    });

    it('缺少 Compose 时保留拉镜像能力，只阻止新建 Docker Bot', () => {
        const input = props();
        input.status!.composeAvailable = false;
        render(<DockerDeploymentSection {...input} />);
        expect(screen.getByRole('button', { name: '拉镜像' })).toBeEnabled();
        expect(screen.getByRole('button', { name: '新建 NapCat Bot' })).toBeDisabled();
        fireEvent.click(screen.getByRole('button', { name: '补全 Compose' }));
        expect(input.onInstall).toHaveBeenCalledOnce();
        expect(navigation.create).not.toHaveBeenCalled();
        expect(screen.getByRole('button', { name: '容器管理' })).toBeEnabled();
        expect(screen.getByRole('button', { name: '容器管理' })).toHaveAttribute(
            'title',
            expect.not.stringContaining('Compose'),
        );
    });

    it.each([
        { installed: false, daemonRunning: false, composeAvailable: false },
        { installed: true, daemonRunning: false, composeAvailable: true },
    ])('does not offer an unusable management route for %o', (status) => {
        render(
            <DockerDeploymentSection {...props({ status: { ...status, version: '27.3.1' } })} />,
        );
        expect(screen.getByRole('button', { name: '容器管理' })).toBeDisabled();
    });

    it('does not expose local Windows container management', () => {
        render(<DockerDeploymentSection {...props({ hostId: 'local', os: 'windows' })} />);
        expect(screen.queryByRole('button', { name: '容器管理' })).not.toBeInTheDocument();
        expect(screen.queryByRole('radiogroup', { name: '选择容器引擎' })).not.toBeInTheDocument();
    });

    it('新建入口保留原主机和框架，管理入口也不跳到首台主机', () => {
        const input = props({
            flavors: ['napcat', 'snowluma'],
            imageReadyByFlavor: { napcat: true, snowluma: true },
        });
        render(<DockerDeploymentSection {...input} />);
        fireEvent.click(screen.getByRole('button', { name: '新建 SnowLuma Bot' }));
        expect(navigation.create).toHaveBeenCalledWith({
            hostId: 'remote:second-host',
            flavor: 'snowluma',
        });
        fireEvent.click(screen.getByRole('button', { name: '容器管理' }));
        expect(navigation.manage).toHaveBeenCalledWith('remote:second-host');
    });

    it('blocks management and deployment while changing the engine', () => {
        engine.switching = true;
        engine.pendingEngine = 'podman';
        render(<DockerDeploymentSection {...props()} />);
        expect(screen.getByText('正在切换引擎', { selector: 'span' })).toBeVisible();
        expect(screen.getByRole('button', { name: '容器管理' })).toBeDisabled();
        expect(screen.getByRole('button', { name: '新建 NapCat Bot' })).toBeDisabled();
        expect(screen.getByRole('radio', { name: 'Podman' })).toBeDisabled();
    });

    it('keeps engine switching locked for an active pull even if that framework is hidden', () => {
        render(
            <DockerDeploymentSection
                {...props({
                    flavors: ['napcat'],
                    isPullingImage: (_, flavor) => flavor === 'snowluma',
                })}
            />,
        );
        expect(screen.getByRole('radio', { name: 'Podman' })).toBeDisabled();
    });

    it('只显示已启用的框架，镜像就绪后仍需用户主动新建 Bot', () => {
        const input = props({ flavors: ['napcat'], imageReadyByFlavor: { napcat: true } });
        render(<DockerDeploymentSection {...input} />);
        expect(screen.getByText('镜像已就绪')).toBeInTheDocument();
        expect(screen.queryByText('SnowLuma')).toBeNull();
        expect(screen.queryByRole('button', { name: '新建 SnowLuma Bot' })).toBeNull();
        expect(screen.getByRole('button', { name: '新建 NapCat Bot' })).toBeEnabled();
        expect(screen.queryByText(/容器已创建|已部署/)).toBeNull();
        expect(input.onPullImage).not.toHaveBeenCalled();
        expect(navigation.create).not.toHaveBeenCalled();
    });
});
