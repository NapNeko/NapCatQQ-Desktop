// @vitest-environment jsdom

import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { componentActionStore } from '../../hooks/components/componentActionStore';
import { useDockerSudoOps, type DockerSudoOpsInput } from './useDockerSudoOps';
import type { QqSudoActionKind } from '../../core/domain/components/qqProbe';

vi.mock('../../hooks/ui/globalInfoBarStore', () => ({
    globalInfoBarStore: { push: vi.fn(), remove: vi.fn() },
}));
vi.mock('../../hooks/ui/pushErrorBar', () => ({ pushErrorBar: vi.fn() }));

let taskSequence = 0;

function failedTask(id: string, kind: QqSudoActionKind, hostId = 'remote:one') {
    id = `${id}-${taskSequence++}`;
    componentActionStore.started(id, 'qq', hostId, kind);
    componentActionStore.failTask(id, 'elevation_required: 请输入 sudo 密码');
}

function inputs(): DockerSudoOpsInput {
    return {
        hostNameOf: (id) => id,
        installDocker: vi.fn(),
        startAction: vi.fn().mockResolvedValue('retried'),
        onTaskTerminal: vi.fn(),
        refetch: vi.fn(),
        probeQqDependencies: vi.fn().mockResolvedValue(undefined),
    };
}

beforeEach(() => componentActionStore._reset());
afterEach(() => {
    cleanup();
    componentActionStore._reset();
});

describe('QQ sudo action retry', () => {
    it.each<QqSudoActionKind>([
        'update',
        'force_install',
        'ensure_installed',
        'ensure_dependencies',
    ])('retries %s with a one-time password', async (kind) => {
        failedTask('original', kind);
        const args = inputs();
        const { result } = renderHook(() => useDockerSudoOps(args));
        expect(result.current.sudoPrompt).toMatchObject({ purpose: 'qq_action', kind });
        await act(async () => result.current.handleSudoConfirm('one-time-password', false));
        expect(args.startAction).toHaveBeenCalledWith('qq', 'remote:one', kind, {
            sudoPassword: 'one-time-password',
            rememberSudo: false,
        });
        expect(result.current.sudoPrompt).toBeNull();
        expect(JSON.stringify(componentActionStore.getSnapshot())).not.toContain(
            'one-time-password',
        );
    });

    it('forwards the remember choice and refreshes after the original action succeeds', async () => {
        failedTask('original', 'update');
        const args = inputs();
        const { result } = renderHook(() => useDockerSudoOps(args));
        await act(async () => result.current.handleSudoConfirm('password', true));
        expect(args.startAction).toHaveBeenCalledWith('qq', 'remote:one', 'update', {
            sudoPassword: 'password',
            rememberSudo: true,
        });
        const callback = vi.mocked(args.onTaskTerminal).mock.calls[0][1];
        callback('success');
        expect(args.refetch).toHaveBeenCalledOnce();
        expect(args.probeQqDependencies).toHaveBeenCalledWith('remote:one', true);
    });

    it('keeps the password dialog on validation failure and does not reopen that failed attempt', async () => {
        failedTask('original', 'update');
        const args = inputs();
        vi.mocked(args.startAction).mockImplementationOnce(async () => {
            failedTask('wrong-password', 'update');
            throw new Error('elevation_required: 密码不正确');
        });
        const { result } = renderHook(() => useDockerSudoOps(args));
        await act(async () => {
            await expect(result.current.handleSudoConfirm('wrong', false)).rejects.toThrow(
                '密码不正确',
            );
        });
        expect(result.current.sudoPrompt).toMatchObject({ purpose: 'qq_action', kind: 'update' });
        await act(async () => result.current.handleSudoConfirm('correct', false));
        expect(result.current.sudoPrompt).toBeNull();
        expect(args.startAction).toHaveBeenCalledTimes(2);
    });

    it('keeps another host pending until the open dialog closes', () => {
        failedTask('first', 'update');
        failedTask('second', 'force_install', 'remote:two');
        const args = inputs();
        const { result } = renderHook(() => useDockerSudoOps(args));
        expect(result.current.sudoPrompt?.hostId).toBe('remote:one');
        act(() => result.current.closeSudoPrompt());
        expect(result.current.sudoPrompt).toMatchObject({
            hostId: 'remote:two',
            kind: 'force_install',
        });
    });

    it('queues a delayed Docker password request without replacing the open QQ dialog', async () => {
        failedTask('original', 'update');
        const args = inputs();
        vi.mocked(args.installDocker).mockResolvedValue({
            status: 'needSudoPassword',
            message: 'Docker 需要密码',
        });
        const { result } = renderHook(() => useDockerSudoOps(args));
        await act(async () => result.current.handleInstallDocker('remote:docker'));
        expect(result.current.sudoPrompt).toMatchObject({
            purpose: 'qq_action',
            hostId: 'remote:one',
        });
        await act(async () => result.current.handleSudoConfirm('qq-password', false));
        expect(result.current.sudoPrompt).toMatchObject({
            purpose: 'docker',
            hostId: 'remote:docker',
        });
    });
});
