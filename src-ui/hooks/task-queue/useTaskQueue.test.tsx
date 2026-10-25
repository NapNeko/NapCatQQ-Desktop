// @vitest-environment jsdom

import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import type { DeploymentTaskSnapshot } from '../../core/ipc/types';
import { deploymentTaskStore } from './deploymentTaskStore';
import { useTaskQueueActiveCount } from './useTaskQueue';

function task(taskId: string, status: DeploymentTaskSnapshot['status']): DeploymentTaskSnapshot {
    return {
        taskId,
        status,
        kind: { kind: 'component_action', component_id: 'neobot', action: 'install' },
        hostId: 'local',
        title: '安装 NeoBot',
        resources: [],
        submittedAtMs: 1n,
        startedAtMs: 1n,
        cancellable: true,
        get progressEvents() {
            throw new Error('任务计数不应读取或重放进度历史');
        },
    };
}

beforeEach(() => deploymentTaskStore._reset());
afterEach(cleanup);

describe('常驻任务计数订阅', () => {
    it('按任务状态统计，不读取任何任务的进度历史', () => {
        deploymentTaskStore.applyList({
            tasks: [
                task('queued', 'queued'),
                task('running', 'running'),
                task('input', 'waiting_input'),
                task('success', 'success'),
                task('failed', 'failed'),
                task('cancelled', 'cancelled'),
            ],
        });
        const { result } = renderHook(() => useTaskQueueActiveCount());
        expect(result.current).toBe(3);
    });

    it('日志快照连续更新但活跃数不变时不重渲根订阅方', () => {
        deploymentTaskStore.applyTask(task('running', 'running'));
        let renders = 0;
        const { result } = renderHook(() => {
            renders += 1;
            return useTaskQueueActiveCount();
        });
        const initialRenders = renders;
        act(() => {
            for (let i = 0; i < 100; i += 1) {
                deploymentTaskStore.applyTask(task('running', 'running'));
            }
        });
        expect(result.current).toBe(1);
        expect(renders).toBe(initialRenders);
    });

    it('任务完成、删除和批量替换后保持计数一致', () => {
        const { result } = renderHook(() => useTaskQueueActiveCount());
        expect(result.current).toBe(0);
        act(() => {
            deploymentTaskStore.applyList({
                tasks: [task('first', 'queued'), task('second', 'running')],
            });
        });
        expect(result.current).toBe(2);
        act(() => deploymentTaskStore.applyTask(task('second', 'waiting_input')));
        expect(result.current).toBe(2);
        act(() => deploymentTaskStore.applyTask(task('second', 'success')));
        expect(result.current).toBe(1);
        act(() => deploymentTaskStore.removeTask('first'));
        expect(result.current).toBe(0);
        act(() => deploymentTaskStore.applyTask(task('third', 'running')));
        expect(result.current).toBe(1);
        act(() => {
            deploymentTaskStore.applyList({ tasks: [task('third', 'failed')] });
        });
        expect(result.current).toBe(0);
    });
});
