import { describe, expect, it } from 'vitest';
import { appInstallTaskTarget, matchesAppInstallTask } from './instanceState';
import type { AppInstance, DeploymentTaskSnapshot } from '../../core/ipc/types';

const instance: AppInstance = {
    id: '6d4853b2',
    framework_id: 'nonebot2',
    display_name: 'NoneBot2',
    placement: 'local_native',
    host_id: 'local',
    install_dir: '/apps/nonebot2/6d4853b2',
    port: 8080,
    state: 'installing',
    created_at_ms: 1,
    install_renderer: true,
    origin: 'created',
};

function task(target: string, action = 'ensure_installed'): DeploymentTaskSnapshot {
    return {
        taskId: 't1',
        kind: { kind: 'component_action', component_id: 'nonebot2', action },
        status: 'success',
        hostId: 'local',
        title: 'NoneBot2 · 安装',
        resources: [{ kind: 'install_target', host_id: 'local', target }],
        progressEvents: [],
        submittedAtMs: 1n,
        cancellable: false,
    };
}

describe('matchesAppInstallTask', () => {
    it('matches nonebot2@instance_id', () => {
        expect(appInstallTaskTarget(instance)).toBe('nonebot2@6d4853b2');
        expect(matchesAppInstallTask(task('nonebot2@6d4853b2'), instance)).toBe(true);
    });

    it('ignores other components', () => {
        expect(matchesAppInstallTask(task('uv'), instance)).toBe(false);
    });
});

describe('latestAppInstallTask', () => {
    it('同一实例有多次安装时取最近提交的那次，别的实例的任务不算', async () => {
        const { latestAppInstallTask } = await import('../../hooks/apps/useAppInstallProgress');
        const old = { ...task('nonebot2@6d4853b2'), taskId: 'old', submittedAtMs: 1n };
        const recent = { ...task('nonebot2@6d4853b2'), taskId: 'recent', submittedAtMs: 5n };
        const other = { ...task('nonebot2@ffff0000'), taskId: 'other', submittedAtMs: 9n };
        expect(latestAppInstallTask({ old, recent, other }, instance)?.taskId).toBe('recent');
        expect(latestAppInstallTask({ other }, instance)).toBeUndefined();
    });
});
