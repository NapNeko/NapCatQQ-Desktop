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
