// componentActionStore 行为单测：任务状态迁移 + 终态 linger 清理。
//
// store 自带 _reset（清 linger 计时器 + 状态），配合全局 fake timers 保证用例间
// 没有残留计时器跨用例触发；清理偏好走 taskQueueCleanupPrefsStore 公开 API 设置。

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { componentActionStore, targetKey } from './componentActionStore';
import { taskQueueCleanupPrefsStore } from '../task-queue/taskQueueCleanupPrefsStore';
import { TASK_QUEUE_TERMINAL_RETENTION_MAX_WHEN_AUTO_OFF } from '../../core/domain/task-queue/cleanup';
import type { ProgressEvent } from '../../core/ipc/types';
import type { ProgressLogLevel } from '../../core/ipc/generated/domain/ProgressLogLevel';

const LINGER_MS = 5_000;

const startedEv = (totalSteps = 2): ProgressEvent => ({
    v: 1,
    timestamp_ms: 1n,
    kind: 'started',
    total_steps: totalSteps,
});
const finishedEv = (ok: boolean): ProgressEvent => ({
    v: 1,
    timestamp_ms: 2n,
    kind: 'finished',
    ok,
});
const logEv = (level: ProgressLogLevel, message: string): ProgressEvent => ({
    v: 1,
    timestamp_ms: 3n,
    kind: 'log',
    level,
    message,
});

const taskOf = (taskId: string) => componentActionStore.getSnapshot().tasks[taskId];

beforeEach(() => {
    vi.useFakeTimers();
    componentActionStore._reset();
    taskQueueCleanupPrefsStore._reset();
    // 显式短 linger，免得用例里推 600_000ms
    taskQueueCleanupPrefsStore.applyPrefs({
        taskQueueCleanupEnabled: true,
        taskQueueCleanupLingerMs: LINGER_MS,
    });
});

afterEach(() => {
    componentActionStore._reset();
    taskQueueCleanupPrefsStore._reset();
    vi.useRealTimers();
});

describe('注册与初始状态', () => {
    it('targetKey 拼 componentId::hostId', () => {
        expect(targetKey('napcat', 'local')).toBe('napcat::local');
    });

    it('空 store 三张表都是空', () => {
        expect(componentActionStore.getSnapshot()).toEqual({
            tasks: {},
            activeByTarget: {},
            taskTargets: {},
        });
    });

    it('started 建 pending 进度并登记 active 与 taskTargets', () => {
        componentActionStore.started('t1', 'napcat', 'host-a');
        const snap = componentActionStore.getSnapshot();
        expect(taskOf('t1').status).toBe('pending');
        expect(snap.activeByTarget[targetKey('napcat', 'host-a')]).toBe('t1');
        expect(snap.taskTargets.t1).toEqual({ componentId: 'napcat', hostId: 'host-a' });
    });

    it('ProgressEvent 早于 started 时先建孤立记录；后到的 started 不重置进度', () => {
        componentActionStore.applyProgress('t1', startedEv(3));
        let snap = componentActionStore.getSnapshot();
        expect(taskOf('t1').status).toBe('running');
        expect(taskOf('t1').totalSteps).toBe(3);
        // 孤立 task 只进 tasks，不进 activeByTarget
        expect(snap.activeByTarget).toEqual({});
        componentActionStore.started('t1', 'napcat', 'host-a');
        snap = componentActionStore.getSnapshot();
        expect(taskOf('t1').totalSteps).toBe(3);
        expect(snap.activeByTarget[targetKey('napcat', 'host-a')]).toBe('t1');
    });

    it('registerTarget 不把已终态的 task 覆盖回 pending', () => {
        componentActionStore.applyProgress('t1', finishedEv(true));
        expect(taskOf('t1').status).toBe('success');
        componentActionStore.registerTarget('t1', 'napcat', 'host-a');
        expect(taskOf('t1').status).toBe('success');
    });
});

describe('状态迁移与 linger 清理', () => {
    it('started→finished 成功：终态立即保留，linger 到点整条清除', () => {
        componentActionStore.started('t1', 'napcat', 'host-a');
        componentActionStore.applyProgress('t1', startedEv(2));
        expect(taskOf('t1').status).toBe('running');
        componentActionStore.applyProgress('t1', finishedEv(true));
        const snap = componentActionStore.getSnapshot();
        expect(snap.tasks.t1.status).toBe('success');
        expect(snap.activeByTarget[targetKey('napcat', 'host-a')]).toBe('t1');
        vi.advanceTimersByTime(LINGER_MS - 1);
        expect(taskOf('t1').status).toBe('success');
        vi.advanceTimersByTime(1);
        const cleared = componentActionStore.getSnapshot();
        expect(cleared.tasks.t1).toBeUndefined();
        expect(cleared.activeByTarget[targetKey('napcat', 'host-a')]).toBeUndefined();
        expect(cleared.taskTargets.t1).toBeUndefined();
    });

    it('finished 失败与 log 事件都照 reducer 走，failed 同样进 linger', () => {
        componentActionStore.started('t1', 'qq', 'local');
        componentActionStore.applyProgress('t1', logEv('warn', '等前置'));
        expect(taskOf('t1').message).toBe('等前置');
        componentActionStore.applyProgress('t1', finishedEv(false));
        expect(taskOf('t1').status).toBe('failed');
        vi.advanceTimersByTime(LINGER_MS);
        expect(componentActionStore.getSnapshot().tasks.t1).toBeUndefined();
    });

    it('failTask：Error / 字符串 / 其它兜底三种入参', () => {
        componentActionStore.started('e1', 'napcat', 'local');
        componentActionStore.failTask('e1', new Error('boom'));
        expect(taskOf('e1')).toMatchObject({ status: 'failed', message: 'boom' });
        expect(taskOf('e1').logs.at(-1)).toMatchObject({ level: 'error', message: 'boom' });

        componentActionStore.started('e2', 'napcat', 'local');
        componentActionStore.failTask('e2', '裸字符串');
        expect(taskOf('e2')).toMatchObject({ status: 'failed', message: '裸字符串' });

        componentActionStore.started('e3', 'napcat', 'local');
        componentActionStore.failTask('e3', { weird: true });
        expect(taskOf('e3').message).toBe('组件操作启动失败');
    });

    it('failTask 未注册过 target 的 task 也能标失败并进 linger', () => {
        componentActionStore.failTask('orphan', 'invoke 挂了');
        expect(taskOf('orphan')).toMatchObject({ status: 'failed', message: 'invoke 挂了' });
        expect(componentActionStore.getSnapshot().activeByTarget).toEqual({});
        vi.advanceTimersByTime(LINGER_MS);
        expect(componentActionStore.getSnapshot().tasks.orphan).toBeUndefined();
    });

    it('同一 target 换任务：旧任务的 linger 计时器被清掉，不再误清活跃标记', () => {
        componentActionStore.started('a', 'napcat', 'host-a');
        componentActionStore.applyProgress('a', finishedEv(true));
        vi.advanceTimersByTime(LINGER_MS - 1);
        componentActionStore.started('b', 'napcat', 'host-a');
        // 旧计时器已清：到点 a 不被移除
        vi.advanceTimersByTime(LINGER_MS);
        expect(taskOf('a').status).toBe('success');
        expect(
            componentActionStore.getSnapshot().activeByTarget[targetKey('napcat', 'host-a')],
        ).toBe('b');
        // b 自己终态后才走自己的 linger
        componentActionStore.applyProgress('b', finishedEv(false));
        vi.advanceTimersByTime(LINGER_MS);
        expect(taskOf('a')).toBeDefined();
        expect(componentActionStore.getSnapshot().tasks.b).toBeUndefined();
        expect(
            componentActionStore.getSnapshot().activeByTarget[targetKey('napcat', 'host-a')],
        ).toBeUndefined();
    });

    it('subscribe：状态变更通知，退订后不再通知', () => {
        const listener = vi.fn();
        const unsubscribe = componentActionStore.subscribe(listener);
        componentActionStore.started('t1', 'napcat', 'local');
        expect(listener).toHaveBeenCalledTimes(1);
        unsubscribe();
        componentActionStore.failTask('t1', 'x');
        expect(listener).toHaveBeenCalledTimes(1);
    });
});

describe('关闭自动清理', () => {
    beforeEach(() => {
        taskQueueCleanupPrefsStore.applyPrefs({
            taskQueueCleanupEnabled: false,
            taskQueueCleanupLingerMs: 0,
        });
    });

    it('终态条目不排计时器、永远保留', () => {
        componentActionStore.started('t1', 'napcat', 'host-a');
        componentActionStore.applyProgress('t1', finishedEv(true));
        vi.advanceTimersByTime(10 * 60 * 1_000);
        const snap = componentActionStore.getSnapshot();
        expect(snap.tasks.t1.status).toBe('success');
        expect(snap.activeByTarget[targetKey('napcat', 'host-a')]).toBe('t1');
    });

    it('偏好切到关闭的那一刻就做一次修剪：终态超硬顶按字典序删最旧，非终态不动', () => {
        // 硬顶 + 1 个终态 + 1 个 pending；字典序 t000 最旧。
        // 造数阶段保持开启：关闭态下每个终态都即时修剪，测不到「切换那一刻」
        taskQueueCleanupPrefsStore.applyPrefs({
            taskQueueCleanupEnabled: true,
            taskQueueCleanupLingerMs: LINGER_MS,
        });
        for (let i = 0; i < TASK_QUEUE_TERMINAL_RETENTION_MAX_WHEN_AUTO_OFF + 1; i++) {
            const id = `t${i.toString().padStart(3, '0')}`;
            componentActionStore.applyProgress(id, finishedEv(true));
        }
        componentActionStore.started('running', 'napcat', 'host-a');
        // 关闭偏好的 applyPrefs 内部触发全 store 修剪
        taskQueueCleanupPrefsStore.applyPrefs({
            taskQueueCleanupEnabled: false,
            taskQueueCleanupLingerMs: 0,
        });
        const snap = componentActionStore.getSnapshot();
        expect(Object.keys(snap.tasks).length).toBe(
            TASK_QUEUE_TERMINAL_RETENTION_MAX_WHEN_AUTO_OFF + 1,
        );
        expect(snap.tasks.t000).toBeUndefined();
        expect(snap.tasks.t200).toBeDefined();
        expect(snap.tasks.running.status).toBe('pending');
    });

    it('修剪掉的终态任务连带清掉 active 与 taskTargets 映射', () => {
        // t000 先注册到 target（pending），再随其余 200 个一起转终态：
        // 超硬顶时字典序最旧的它被删，active/taskTargets 一起没了
        componentActionStore.registerTarget('t000', 'napcat', 'host-a');
        for (let i = 0; i < TASK_QUEUE_TERMINAL_RETENTION_MAX_WHEN_AUTO_OFF + 1; i++) {
            const id = `t${i.toString().padStart(3, '0')}`;
            componentActionStore.applyProgress(id, finishedEv(true));
        }
        componentActionStore.trimTerminalTasksWhenAutoCleanupOff();
        const snap = componentActionStore.getSnapshot();
        expect(Object.keys(snap.tasks).length).toBe(
            TASK_QUEUE_TERMINAL_RETENTION_MAX_WHEN_AUTO_OFF,
        );
        expect(snap.tasks.t000).toBeUndefined();
        expect(snap.taskTargets.t000).toBeUndefined();
        expect(snap.activeByTarget[targetKey('napcat', 'host-a')]).toBeUndefined();
    });
});
