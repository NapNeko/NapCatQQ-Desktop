// @vitest-environment jsdom

import type { ComponentProps } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { initialActionProgress } from '../../core/domain/components/progress';
import { LOCAL_QQ_SOURCE_QUERY_KEY } from '../../hooks/components/useLocalQqSource';
import { MachineComponentRowView } from './MachineComponentRow';

const service = vi.hoisted(() => ({ localQqSource: vi.fn(), openDirectory: vi.fn() }));
vi.mock('../../core/services/component.service', () => ({ componentService: service }));

const clients: QueryClient[] = [];

function renderCard(overrides: Partial<ComponentProps<typeof MachineComponentRowView>> = {}) {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    clients.push(client);
    const onAction = vi.fn();
    render(
        <QueryClientProvider client={client}>
            <MachineComponentRowView
                row={{
                    info: {
                        id: 'qq',
                        display_name: 'QQ',
                        description: 'QQ 桌面客户端',
                        repo_url: null,
                        supported_targets: [{ os: 'windows', locality: 'local' }],
                        category: 'runtime_dep',
                        uninstall: 'supported',
                    },
                    status: {
                        state: 'installed',
                        detected: { version: '9.9.36', source: 'QQ/versions/config.json' },
                    },
                }}
                hostId="local"
                latestRemoteVersion="9.9.37"
                activeProgress={null}
                onAction={onAction}
                onRetryDetect={vi.fn()}
                {...overrides}
            />
        </QueryClientProvider>,
    );
    return { client, onAction };
}

beforeEach(() => {
    service.localQqSource.mockReset().mockResolvedValue('system');
    service.openDirectory.mockReset().mockResolvedValue(undefined);
});

describe('component directory context menu', () => {
    async function openMenu() {
        const user = userEvent.setup();
        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('QQ') });
        await screen.findByRole('menu');
        return user;
    }

    it('opens the selected local component without changing its installation', async () => {
        const { onAction } = renderCard({ lifecycleBlockedReason: 'Bot 正在运行' });
        const user = await openMenu();
        await user.click(screen.getByRole('menuitem', { name: '打开本地目录' }));
        expect(service.openDirectory).toHaveBeenCalledTimes(1);
        expect(service.openDirectory).toHaveBeenCalledWith('qq', 'local');
        expect(onAction).not.toHaveBeenCalled();
    });

    it('does not offer a local file manager for a remote component', async () => {
        renderCard({ hostId: 'remote:production' });
        await openMenu();
        expect(screen.queryByRole('menuitem', { name: '打开本地目录' })).not.toBeInTheDocument();
        expect(service.openDirectory).not.toHaveBeenCalled();
    });
});

afterEach(() => {
    cleanup();
    for (const client of clients) client.clear();
    clients.length = 0;
});

describe('external Node.js component management', () => {
    it.each(['local', 'remote:linux'])(
        'offers an independent installation on %s without uninstalling the system Node.js',
        (hostId) => {
            const { onAction } = renderCard({
                hostId,
                row: {
                    info: {
                        id: 'nodejs',
                        display_name: 'Node.js',
                        description: 'Node.js 运行时',
                        repo_url: null,
                        supported_targets: [{ os: 'linux', locality: 'local' }],
                        category: 'runtime_dep',
                        uninstall: 'supported',
                    },
                    status: {
                        state: 'installed',
                        detected: { version: '26.12.1', source: '$PATH/node' },
                    },
                },
            });
            const install = screen.getByRole('button', { name: '独立安装' });
            expect(install).toHaveAttribute('title', expect.stringContaining('不改动系统'));
            expect(screen.queryByRole('button', { name: '卸载组件' })).not.toBeInTheDocument();
            expect(screen.queryByRole('button', { name: '更新' })).not.toBeInTheDocument();
            fireEvent.click(install);
            expect(onAction).toHaveBeenCalledTimes(1);
            expect(onAction).toHaveBeenCalledWith({ stepKind: 'force_install' });
        },
    );
});

describe('QQ component management', () => {
    it('offers a separate installation even when the system QQ is already installed', async () => {
        const { onAction } = renderCard();
        const install = await screen.findByRole('button', { name: '转为 NCD 管理' });
        expect(screen.getByText('系统 QQ')).toBeInTheDocument();
        expect(install).toHaveAttribute('title', expect.stringContaining('不改动系统 QQ'));
        expect(screen.queryByRole('button', { name: '更新' })).not.toBeInTheDocument();
        expect(screen.queryByRole('button', { name: '卸载组件' })).not.toBeInTheDocument();
        fireEvent.click(install);
        expect(onAction).toHaveBeenCalledTimes(1);
        expect(onAction).toHaveBeenCalledWith({ stepKind: 'force_install' });
    });

    it('refreshes the ownership label and restores managed actions after installation', async () => {
        const { client } = renderCard();
        await screen.findByText('系统 QQ');
        service.localQqSource.mockResolvedValue('managed');
        await act(async () => {
            await client.invalidateQueries({ queryKey: LOCAL_QQ_SOURCE_QUERY_KEY });
        });
        await screen.findByText('NCD 管理');
        expect(screen.queryByRole('button', { name: '转为 NCD 管理' })).not.toBeInTheDocument();
        expect(screen.getByRole('button', { name: '更新' })).toBeEnabled();
        expect(screen.getByRole('button', { name: '卸载组件' })).toBeEnabled();
    });

    it('keeps the source probe local to this computer', () => {
        renderCard({ hostId: 'remote:production' });
        expect(service.localQqSource).not.toHaveBeenCalled();
        expect(screen.queryByRole('button', { name: '转为 NCD 管理' })).not.toBeInTheDocument();
        expect(screen.getByRole('button', { name: '更新' })).toBeEnabled();
    });

    it('disables the entry when component operations are disabled', async () => {
        renderCard({ disabled: true });
        expect(await screen.findByRole('button', { name: '转为 NCD 管理' })).toBeDisabled();
    });

    it('prevents duplicate installation while a task cannot be cancelled', async () => {
        renderCard({
            activeProgress: {
                taskId: 'install-qq',
                cancellable: false,
                progress: { ...initialActionProgress, status: 'running' },
            },
        });
        expect(await screen.findByRole('button', { name: '转为 NCD 管理' })).toBeDisabled();
        expect(screen.getByText('进行中')).toBeInTheDocument();
    });
});
