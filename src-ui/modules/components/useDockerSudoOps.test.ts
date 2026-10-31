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

    it('requests and retries local noVNC elevation without remembering the password', async () => {
        componentActionStore.started('local-novnc', 'novnc', 'local', 'ensure_installed');
        const args = inputs();
        const { result } = renderHook(() => useDockerSudoOps(args));
        act(() => {
            // 真实后台任务会在提权错误之后追加 finished；终态日志是「任务失败」。
            componentActionStore.applyProgress('local-novnc', {
                v: 1,
                timestamp_ms: 1n,
                kind: 'log',
                level: 'error',
                message:
                    "step 'single' failed: install step failed: sudo: elevation_required: noVNC 图形栈需要 sudo 密码",
            });
            componentActionStore.applyProgress('local-novnc', {
                v: 1,
                timestamp_ms: 2n,
                kind: 'finished',
                ok: false,
            });
        });
        expect(result.current.sudoPrompt).toMatchObject({ componentId: 'novnc', hostId: 'local' });
        await act(async () => result.current.handleSudoConfirm('local-password', false));
        expect(args.startAction).toHaveBeenCalledWith('novnc', 'local', 'ensure_installed', {
            sudoPassword: 'local-password',
            rememberSudo: false,
        });
        expect(result.current.sudoPrompt).toBeNull();
        expect(JSON.stringify(componentActionStore.getSnapshot())).not.toContain('local-password');
    });

    it('opens a local Docker password prompt and retries the same host', async () => {
        const args = inputs();
        vi.mocked(args.installDocker)
            .mockResolvedValueOnce({ status: 'needSudoPassword', message: '请输入 sudo 密码' })
            .mockResolvedValueOnce({ status: 'installed', message: '已安装' });
        const { result } = renderHook(() => useDockerSudoOps(args));
        await act(async () => result.current.handleInstallDocker('local'));
        expect(result.current.sudoPrompt).toMatchObject({ purpose: 'docker', hostId: 'local' });
        await act(async () => result.current.handleSudoConfirm('local-password', false));
        expect(args.installDocker).toHaveBeenLastCalledWith('local', {
            sudoPassword: 'local-password',
            rememberSudo: false,
        });
        expect(result.current.sudoPrompt).toBeNull();
    });

    it('retries a local SnowLuma install with its original package choice', async () => {
        componentActionStore.started('local-sl', 'snowluma', 'local', 'ensure_installed', 'lite');
        componentActionStore.failTask('local-sl', 'elevation_required: QQ 系统依赖需要 sudo 密码');
        const args = inputs();
        const { result } = renderHook(() => useDockerSudoOps(args));
        expect(result.current.sudoPrompt).toMatchObject({
            componentId: 'snowluma',
            hostId: 'local',
            snowlumaLinuxPackage: 'lite',
        });
        await act(async () => result.current.handleSudoConfirm('one-time-password', false));
        expect(args.startAction).toHaveBeenCalledWith('snowluma', 'local', 'ensure_installed', {
            sudoPassword: 'one-time-password',
            rememberSudo: false,
            snowlumaLinuxPackage: 'lite',
        });
    });

    it('retries the original local NapCat install after dependency elevation fails', async () => {
        componentActionStore.started('local-nc', 'napcat', 'local', 'ensure_installed');
        componentActionStore.failTask('local-nc', 'elevation_required: QQ 系统依赖需要 sudo 密码');
        const args = inputs();
        const { result } = renderHook(() => useDockerSudoOps(args));
        await act(async () => result.current.handleSudoConfirm('one-time-password', false));
        expect(args.startAction).toHaveBeenCalledWith('napcat', 'local', 'ensure_installed', {
            sudoPassword: 'one-time-password',
            rememberSudo: false,
        });
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
