import { describe, expect, it } from 'vitest';

import { initialActionProgress } from '../components/progress';
import { buildTaskQueueSnapshot } from './buildSnapshot';

const emptyMeta = {
    dockerDeployByTaskId: {},
    dockerInstallByHostId: {},
};

describe('buildTaskQueueSnapshot', () => {
    it('sorts running tasks before terminal, then by startedAt desc', () => {
        const runningProgress = {
            ...initialActionProgress,
            status: 'running' as const,
            message: '下载中',
            logs: [{ level: 'info' as const, message: 'start', timestamp_ms: 2000 }],
        };
        const successProgress = {
            ...initialActionProgress,
            status: 'success' as const,
            logs: [{ level: 'info' as const, message: 'done', timestamp_ms: 5000 }],
        };
        const pendingDeploy = {
            ...initialActionProgress,
            status: 'pending' as const,
            logs: [{ level: 'info' as const, message: 'deploy', timestamp_ms: 3000 }],
        };

        const snapshot = buildTaskQueueSnapshot({
            componentAction: {
                tasks: {
                    'task-done': successProgress,
                    'task-run': runningProgress,
                },
                activeByTarget: { 'qq::host-a': 'task-run' },
                taskTargets: {
                    'task-done': { componentId: 'napcat', hostId: 'host-b' },
                    'task-run': { componentId: 'qq', hostId: 'host-a' },
                },
            },
            dockerAction: {
                installingByHost: {},
                installHintByHost: {},
                installTaskIdByHost: {},
            },
            dockerDeployProgress: {
                tasks: { 'deploy-1': pendingDeploy },
            },
            dockerInstallProgress: { tasks: {} },
            meta: emptyMeta,
            hostLabels: { 'host-a': '本机', 'host-b': '远端' },
        });

        expect(snapshot.activeCount).toBe(2);
        expect(snapshot.items.map((i) => i.id)).toEqual([
            'task-run',
            'deploy-1',
            'task-done',
        ]);
        expect(snapshot.items[0].status).toBe('running');
        expect(snapshot.items[1].status).toBe('pending');
        expect(snapshot.items[2].status).toBe('success');
    });

    it('includes docker install row with hint and activeCount', () => {
        const snapshot = buildTaskQueueSnapshot({
            componentAction: {
                tasks: {},
                activeByTarget: {},
                taskTargets: {},
            },
            dockerAction: {
                installingByHost: { 'rh-1': true },
                installHintByHost: { 'rh-1': '正在安装 Docker…' },
                installTaskIdByHost: { 'rh-1': 'tid-1' },
            },
            dockerDeployProgress: { tasks: {} },
            dockerInstallProgress: { tasks: {} },
            meta: {
                dockerDeployByTaskId: {},
                dockerInstallByHostId: { 'rh-1': { hostId: 'rh-1', startedAt: 1000 } },
            },
            hostLabels: { 'rh-1': 'Linux 主机' },
        });

        expect(snapshot.activeCount).toBe(1);
        expect(snapshot.items).toHaveLength(1);
        expect(snapshot.items[0]).toMatchObject({
            id: 'docker_install::rh-1::tid-1',
            kind: 'docker_install',
            status: 'installing',
            logHint: '正在安装 Docker…',
            startedAt: 1000,
            hostLabel: 'Linux 主机',
        });
    });
});