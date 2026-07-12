import { describe, expect, it } from 'vitest';
import {
    DEFAULT_TASK_QUEUE_CLEANUP_WHEN_ENABLED_MS,
    TASK_QUEUE_TERMINAL_RETENTION_MAX_WHEN_AUTO_OFF,
    taskQueueCleanupFromAppSettings,
    taskQueueCleanupToStoredFields,
    trimTerminalTasksInRecord,
} from './cleanup';

describe('taskQueueCleanupFromAppSettings', () => {
    it('defaults when fields missing', () => {
        expect(taskQueueCleanupFromAppSettings({})).toEqual({
            taskQueueCleanupEnabled: true,
            taskQueueCleanupLingerMs: DEFAULT_TASK_QUEUE_CLEANUP_WHEN_ENABLED_MS,
        });
    });

    it('disabled when enabled false', () => {
        expect(
            taskQueueCleanupFromAppSettings({
                taskQueueCleanupEnabled: false,
                taskQueueCleanupLingerMs: 0,
            }),
        ).toMatchObject({ taskQueueCleanupEnabled: false });
    });
});

describe('taskQueueCleanupToStoredFields', () => {
    it('writes zero linger when disabled', () => {
        expect(
            taskQueueCleanupToStoredFields({
                taskQueueCleanupEnabled: false,
                taskQueueCleanupLingerMs: 60_000,
            }),
        ).toEqual({
            taskQueueCleanupEnabled: false,
            taskQueueCleanupLingerMs: 0,
        });
    });
});

describe('trimTerminalTasksInRecord', () => {
    it('drops oldest terminal ids when over cap', () => {
        const max = TASK_QUEUE_TERMINAL_RETENTION_MAX_WHEN_AUTO_OFF;
        const tasks: Record<string, { status: string }> = {};
        // 终态必须超过 cap；另留若干 running 验证不会被误删。
        for (let i = 0; i < max + 5; i++) {
            const id = `t-${String(i).padStart(4, '0')}`;
            tasks[id] = { status: 'success' };
        }
        for (let i = 0; i < 3; i++) {
            const id = `r-${String(i).padStart(4, '0')}`;
            tasks[id] = { status: 'running' };
        }
        const terminalCount = Object.values(tasks).filter(
            (t) => t.status === 'success',
        ).length;
        expect(terminalCount).toBeGreaterThan(max);

        const { tasks: next, removedIds } = trimTerminalTasksInRecord(
            tasks,
            (t) => t.status === 'success',
            max,
        );
        const remainingTerminal = Object.values(next).filter(
            (t) => t.status === 'success',
        ).length;
        expect(remainingTerminal).toBe(max);
        expect(removedIds.length).toBe(terminalCount - max);
        expect(
            Object.keys(next).some((id) => tasks[id].status === 'running'),
        ).toBe(true);
    });
});