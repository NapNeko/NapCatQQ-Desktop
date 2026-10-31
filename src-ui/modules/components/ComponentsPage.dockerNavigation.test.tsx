// @vitest-environment jsdom
import type { ButtonHTMLAttributes, ReactNode, Ref } from 'react';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ComponentsView, HostInfo, MachineView } from '../../core/domain/components/types';
import {
    consumeDockerDeployment,
    getPendingDockerDeployment,
    openDockerDeployment,
} from '../../hooks/components/componentsNavigation';
import { clearComponentsHostBridge } from '../../hooks/desktop/componentsHostBridge';
import { ComponentsPageNext } from './ComponentsPage.next';

const harness = vi.hoisted(() => ({
    components: {
        view: { framework: [], runtimeDep: [], selfApp: [] } as ComponentsView,
        hosts: [] as HostInfo[],
        isLoading: false,
        error: null,
        refetch: vi.fn(),
    },
    serverLoading: false,
    motionEnabled: true,
    refetchDocker: vi.fn(),
    installDocker: vi.fn(),
    pushInfoBar: vi.fn(),
    probeQqDependencies: vi.fn(),
    queryClient: { invalidateQueries: vi.fn() },
}));

vi.mock('@tanstack/react-query', () => ({ useQueryClient: () => harness.queryClient }));
vi.mock('../../hooks/components/useComponents', () => ({
    useComponents: () => harness.components,
}));
vi.mock('../../hooks/components/useComponentAction', () => ({
    useComponentAction: () => ({
        getProgressFor: vi.fn(),
        startAction: vi.fn(),
        onTaskTerminal: vi.fn(),
    }),
}));
vi.mock('../../hooks/components/useComponentActionErrors', () => ({
    useComponentActionErrors: () => {},
}));
vi.mock('../../hooks/components/useComponentPageAlerts', () => ({
    useComponentPageAlerts: () => {},
}));
vi.mock('../../hooks/bot/useBotSnapshots', () => ({ useBotSnapshots: () => ({ data: [] }) }));
vi.mock('../../hooks/bot/useBotConfigsMap', () => ({ useBotConfigsMap: () => ({}) }));
vi.mock('../../hooks/components/useQqDependencyOps', () => ({
    useQqDependencyOps: () => ({ detectQqDependencies: harness.probeQqDependencies }),
}));
vi.mock('../../hooks/diagnostics/useReleases', () => ({
    useReleases: () => ({ snapshot: null, refetch: vi.fn(), isFetching: false }),
}));
vi.mock('../../hooks/docker/useDockerHosts', () => ({
    useDockerHosts: () => ({
        statusByHost: {},
        errorByHost: {},
        probingByHost: {},
        installingByHost: {},
        installHintByHost: {},
        imageReadyByHost: {},
        imageErrorByHost: {},
        containersByHost: {},
        refetch: harness.refetchDocker,
        install: harness.installDocker,
        openDownloadPage: vi.fn(),
        isPullingFrameworkImage: vi.fn(),
        pullFrameworkImage: vi.fn(),
    }),
}));
vi.mock('../../hooks/docker/useDockerInstallProgress', () => ({
    useDockerInstallProgress: () => null,
}));
vi.mock('../../hooks/apps/useAppInstances', () => ({
    useAppFrameworks: () => ({ data: [] }),
    useAppInstances: () => ({ instances: [], refetch: vi.fn() }),
}));
vi.mock('../../hooks/preferences/featureTogglesStore', () => ({ useFeatureEnabled: () => false }));
vi.mock('../../hooks/preferences/useMotion', () => ({
    useMotion: () => ({ enabled: harness.motionEnabled }),
}));
vi.mock('../../hooks/remote/useServerManager', () => ({
    useServerManager: () => ({ servers: [], isLoading: harness.serverLoading }),
}));
vi.mock('../../hooks/ui/globalInfoBarStore', () => ({ pushInfoBar: harness.pushInfoBar }));
vi.mock('../../shared/ui', () => ({
    Button: ({ children, onClick, disabled }: ButtonHTMLAttributes<HTMLButtonElement>) => (
        <button type="button" onClick={onClick} disabled={disabled}>
            {children}
        </button>
    ),
}));
vi.mock('../../shared/ui/motion', () => ({ MotionIcon: () => null, refreshMotion: () => 'none' }));
vi.mock('../../shared/ui/PagePlaceholder', () => ({
    PagePlaceholder: ({ children }: { children?: ReactNode }) => <div>{children}</div>,
}));
vi.mock('./HostSwitcher', () => ({
    HostSwitcher: ({
        machines,
        activeHostId,
        onSelect,
    }: {
        machines: MachineView[];
        activeHostId: string;
        onSelect: (hostId: string) => void;
    }) => (
        <div>
            {machines.map(({ host }) => (
                <button
                    key={host.host_id}
                    type="button"
                    aria-pressed={activeHostId === host.host_id}
                    onClick={() => onSelect(host.host_id)}
                >
                    {host.display_name}
                </button>
            ))}
        </div>
    ),
}));
vi.mock('./HostComponentsView', () => ({
    HostComponentsView: ({
        machine,
        dockerSectionRef,
    }: {
        machine: MachineView;
        dockerSectionRef?: Ref<HTMLElement>;
    }) => (
        <section
            ref={dockerSectionRef}
            tabIndex={-1}
            data-testid="docker-section"
            data-host={machine.host.host_id}
        >
            Docker 部署
        </section>
    ),
}));
vi.mock('./ReleaseNotesDialog', () => ({ ReleaseNotesDialog: () => null }));
vi.mock('./SnowLumaPackageDialog', () => ({ SnowLumaPackageDialog: () => null }));
vi.mock('../docker/SudoPasswordDialog', () => ({ SudoPasswordDialog: () => null }));
vi.mock('../apps', () => ({ CreateInstanceDialog: () => null, ImportInstanceDialog: () => null }));
vi.mock('./useQqDependencyProbes', () => ({
    useQqDependencyProbes: () => ({
        probeQqDependencies: harness.probeQqDependencies,
        reportByHost: {},
    }),
}));
vi.mock('./useComponentActions', () => ({
    useComponentActions: () => ({ slPkgPrompt: null, lifecycleBlockedReasonForHost: () => null }),
}));
vi.mock('./useDockerSudoOps', () => ({
    useDockerSudoOps: () => ({ sudoPrompt: null, handleInstallDocker: harness.installDocker }),
}));
vi.mock('./useAppInstanceDialogs', () => ({ useAppInstanceDialogs: () => ({}) }));

const local: HostInfo = {
    host_id: 'local',
    display_name: '本机',
    locality: 'local',
    os: 'windows',
};
const remote: HostInfo = {
    host_id: 'remote:target',
    display_name: '目标远端',
    locality: 'remote',
    os: 'linux',
};
const frames = new Map<number, FrameRequestCallback>();
let nextFrame = 0;
const scrollIntoView = vi.fn();
const originalScroll = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'scrollIntoView');

function setHosts(hosts: HostInfo[]): void {
    harness.components.hosts = hosts;
    harness.components.view = {
        framework: [
            {
                info: {
                    id: 'napcat',
                    display_name: 'NapCat',
                    description: '',
                    supported_targets: [],
                    category: 'framework',
                    uninstall: 'supported',
                },
                rows: hosts.map((host) => ({
                    component_id: 'napcat',
                    host,
                    status: { state: 'not_installed' },
                })),
            },
        ],
        runtimeDep: [],
        selfApp: [],
    };
}

function flushFrames(): void {
    act(() => {
        const callbacks = [...frames.values()];
        frames.clear();
        for (const callback of callbacks) callback(0);
    });
}

beforeEach(() => {
    vi.clearAllMocks();
    clearComponentsHostBridge();
    harness.components.isLoading = false;
    harness.serverLoading = false;
    harness.motionEnabled = true;
    setHosts([local, remote]);
    frames.clear();
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
        const id = ++nextFrame;
        frames.set(id, callback);
        return id;
    });
    vi.stubGlobal('cancelAnimationFrame', (id: number) => frames.delete(id));
    Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', {
        configurable: true,
        writable: true,
        value: scrollIntoView,
    });
});

afterEach(() => {
    cleanup();
    const pending = getPendingDockerDeployment();
    if (pending) consumeDockerDeployment(pending.requestId);
    frames.clear();
    vi.unstubAllGlobals();
    if (originalScroll)
        Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', originalScroll);
    else Reflect.deleteProperty(HTMLElement.prototype, 'scrollIntoView');
});

describe('组件页 Docker 部署导航', () => {
    it('等待目标主机异步加载，选中并定位一次；消费后允许自由切换主机', () => {
        setHosts([local]);
        harness.components.isLoading = true;
        harness.serverLoading = true;
        openDockerDeployment(remote.host_id);
        const view = render(<ComponentsPageNext />);
        expect(getPendingDockerDeployment()?.hostId).toBe(remote.host_id);
        expect(scrollIntoView).not.toHaveBeenCalled();
        expect(harness.pushInfoBar).not.toHaveBeenCalled();

        setHosts([local, remote]);
        harness.components.isLoading = false;
        harness.serverLoading = false;
        view.rerender(<ComponentsPageNext />);
        flushFrames();

        const section = screen.getByTestId('docker-section');
        expect(section).toHaveAttribute('data-host', remote.host_id);
        expect(section).toHaveFocus();
        expect(scrollIntoView).toHaveBeenCalledWith({ behavior: 'smooth', block: 'start' });
        expect(getPendingDockerDeployment()).toBeNull();
        fireEvent.click(screen.getByRole('button', { name: '本机' }));
        expect(screen.getByTestId('docker-section')).toHaveAttribute('data-host', 'local');
        expect(scrollIntoView).toHaveBeenCalledOnce();
    });

    it('本机 Linux 可以定位 Docker 部署区', () => {
        setHosts([{ ...local, os: 'linux' }]);
        openDockerDeployment('local');
        render(<ComponentsPageNext />);
        flushFrames();

        expect(getPendingDockerDeployment()).toBeNull();
        expect(screen.getByTestId('docker-section')).toHaveFocus();
        expect(scrollIntoView).toHaveBeenCalledOnce();
        expect(harness.pushInfoBar).not.toHaveBeenCalled();
    });

    it('失效目标清除请求并提示，不把本机当作部署目标聚焦或执行操作', () => {
        openDockerDeployment('remote:removed');
        render(<ComponentsPageNext />);
        flushFrames();

        expect(getPendingDockerDeployment()).toBeNull();
        expect(harness.pushInfoBar).toHaveBeenCalledWith(
            expect.objectContaining({
                tone: 'warning',
                title: '无法打开 Docker 部署',
            }),
        );
        expect(scrollIntoView).not.toHaveBeenCalled();
        expect(screen.getByTestId('docker-section')).not.toHaveFocus();
        expect(harness.installDocker).not.toHaveBeenCalled();
    });

    it('关闭动效时立即滚动，刷新包含 Docker 状态', () => {
        harness.motionEnabled = false;
        openDockerDeployment(remote.host_id);
        render(<ComponentsPageNext />);
        flushFrames();

        expect(scrollIntoView).toHaveBeenCalledWith({ behavior: 'auto', block: 'start' });
        fireEvent.click(screen.getByRole('button', { name: '刷新' }));
        expect(harness.components.refetch).toHaveBeenCalledOnce();
        expect(harness.refetchDocker).toHaveBeenCalledOnce();
    });
});
