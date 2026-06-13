import { describe, expect, it } from 'vitest';
import {
    DEFAULT_TASK_QUEUE_CLEANUP_WHEN_ENABLED_MS,
    taskQueueCleanupFromAppSettings,
    taskQueueCleanupToStoredFields,
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