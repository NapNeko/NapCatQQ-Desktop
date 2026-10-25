// @vitest-environment jsdom

import { useState } from 'react';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { NetworkTab } from './NetworkTab';
import { draftFromBackendAndPrefs, type SettingsDraft } from '../settings-draft';
import { mockBackendSettings } from '../../../core/ipc/mock/settings.mock';
import { defaultNetworkSettings } from '../../../core/domain/settings/network';

const service = vi.hoisted(() => ({ listDownloadMirrors: vi.fn(), test: vi.fn() }));
vi.mock('../../../core/services/network.service', () => ({ networkService: service }));

function Form() {
    const [draft, setDraft] = useState(() =>
        draftFromBackendAndPrefs({
            ...mockBackendSettings,
            network: {
                ...defaultNetworkSettings(),
                proxyMode: 'manual',
                proxyUrl: 'http://localhost:7890',
                proxyUsername: 'user',
                githubDownloadMode: 'mirror',
                githubMirror: 'https://custom.example.com',
            },
            proxyPassword: 'draft-secret',
        }),
    );
    return (
        <NetworkTab
            draft={draft}
            patchDraft={(patch: Partial<SettingsDraft>) =>
                setDraft((current) => ({ ...current, ...patch }))
            }
        />
    );
}

beforeEach(() => {
    service.listDownloadMirrors.mockReset().mockResolvedValue([
        {
            id: 'github-gh-proxy',
            label: 'gh-proxy.com',
            url: 'https://gh-proxy.com',
            kind: 'github',
        },
    ]);
    service.test.mockReset().mockResolvedValue([
        {
            target: 'GitHub 下载',
            url: 'https://custom.example.com',
            elapsedMs: 24,
            success: false,
            message: 'fixture: mirror unavailable',
        },
    ]);
});
beforeAll(() => {
    Element.prototype.hasPointerCapture ??= () => false;
    Element.prototype.releasePointerCapture ??= () => {};
    Element.prototype.scrollIntoView ??= () => {};
});

afterEach(cleanup);

describe('network settings tab', () => {
    it('edits a Python package source and can restore the uv configuration', async () => {
        const user = userEvent.setup();
        render(<Form />);
        const source = screen.getByRole('combobox', { name: 'Python 包源' });
        expect(source).toHaveTextContent('跟随 uv 配置');
        await user.click(source);
        await user.click(screen.getByRole('option', { name: '自定义地址' }));
        fireEvent.change(screen.getByLabelText('Python 自定义包源地址'), {
            target: { value: 'http://pypi.example/simple' },
        });
        expect(screen.getByText('Python 包源仅支持 HTTPS')).toBeInTheDocument();
        expect(screen.getByRole('button', { name: '测试连接' })).toBeDisabled();
        fireEvent.change(screen.getByLabelText('Python 自定义包源地址'), {
            target: { value: 'https://pypi.example/simple' },
        });
        await user.click(screen.getByRole('button', { name: '测试连接' }));
        await waitFor(() =>
            expect(service.test).toHaveBeenLastCalledWith(
                expect.objectContaining({ pypiIndex: 'https://pypi.example/simple' }),
                'draft-secret',
            ),
        );
        await user.click(source);
        await user.click(screen.getByRole('option', { name: '跟随 uv 配置' }));
        await user.click(screen.getByRole('button', { name: '测试连接' }));
        await waitFor(() =>
            expect(service.test).toHaveBeenLastCalledWith(
                expect.objectContaining({ pypiIndex: '' }),
                'draft-secret',
            ),
        );
    });

    it('validates editable URLs, reveals passwords, and tests the unsaved draft', async () => {
        render(<Form />);
        expect(screen.getByLabelText('代理密码')).toHaveAttribute('type', 'password');
        fireEvent.click(await screen.findByRole('button', { name: '显示代理密码' }));
        expect(screen.getByLabelText('代理密码')).toHaveAttribute('type', 'text');
        fireEvent.change(screen.getByLabelText('代理地址'), {
            target: { value: 'http://user:secret@localhost:7890' },
        });
        expect(screen.getByRole('button', { name: '测试连接' })).toBeDisabled();
        expect(screen.getByText('地址中不能包含用户名或密码')).toBeInTheDocument();
        fireEvent.change(screen.getByLabelText('代理地址'), {
            target: { value: 'socks5h://localhost:1080' },
        });
        fireEvent.change(screen.getByLabelText('GitHub 自定义镜像地址'), {
            target: { value: 'https://draft.example.com' },
        });
        fireEvent.click(screen.getByRole('button', { name: '测试连接' }));
        await screen.findByText('fixture: mirror unavailable');
        expect(service.test).toHaveBeenCalledWith(
            expect.objectContaining({
                proxyUrl: 'socks5h://localhost:1080',
                githubMirror: 'https://draft.example.com',
            }),
            'draft-secret',
        );
        expect(screen.getByText('24 ms')).toBeInTheDocument();
        fireEvent.change(screen.getByLabelText('代理密码'), { target: { value: 'edited-secret' } });
        await waitFor(() =>
            expect(screen.queryByText('fixture: mirror unavailable')).not.toBeInTheDocument(),
        );
    });

    it('shows unsupported preview diagnostics without reporting a connection success', async () => {
        service.test.mockRejectedValueOnce(
            new Error('浏览器预览不执行真实网络测试，请在桌面端测试连接'),
        );
        render(<Form />);
        fireEvent.click(screen.getByRole('button', { name: '测试连接' }));
        await waitFor(() =>
            expect(screen.getByRole('alert')).toHaveTextContent('浏览器预览不执行真实网络测试'),
        );
        expect(screen.queryByLabelText('成功')).not.toBeInTheDocument();
    });
});
