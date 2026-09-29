import { describe, expect, it } from 'vitest';
import type { DeploymentTaskSnapshot } from '../../ipc/types';
import { canCancelDeploymentTask } from './display';

function task(
    status: DeploymentTaskSnapshot['status'],
    cancellable: boolean,
): DeploymentTaskSnapshot {
    return {
        taskId: 't1',
        kind: { kind: 'component_action', component_id: 'napcat', action: 'install' },
        status,
        hostId: 'local',
        title: 'NapCat install',
        resources: [],
        progressEvents: [],
        submittedAtMs: 0n,
        cancellable,
    } as DeploymentTaskSnapshot;
}

describe('canCancelDeploymentTask', () => {
    it('没有快照时照旧给取消', () => {
        expect(canCancelDeploymentTask(undefined)).toBe(true);
    });

    it('排队和等输入的任务不看 cancellable', () => {
        expect(canCancelDeploymentTask(task('queued', false))).toBe(true);
        expect(canCancelDeploymentTask(task('waiting_input', false))).toBe(true);
    });

    it('在跑的任务按后端给的 cancellable', () => {
        expect(canCancelDeploymentTask(task('running', false))).toBe(false);
        expect(canCancelDeploymentTask(task('running', true))).toBe(true);
    });

    it('终态不给取消', () => {
        expect(canCancelDeploymentTask(task('success', true))).toBe(false);
    });
});
