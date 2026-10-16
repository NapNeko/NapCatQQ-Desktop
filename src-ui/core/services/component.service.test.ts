import { afterEach, describe, expect, it, vi } from 'vitest';
import { componentService } from './component.service';
import { subscribeMockEvents } from '../ipc/mock/events.mock';

afterEach(() => vi.useRealTimers());

describe('component action preview', () => {
    it('uses the requested task ID and publishes the new QQ source before completion', async () => {
        vi.useFakeTimers();
        const finishedSources: string[] = [];
        const unsubscribe = subscribeMockEvents((event) => {
            if (
                event.kind === 'component_action_progress' &&
                event.task_id === 'qq-preview-install' &&
                event.event.kind === 'finished'
            ) {
                void componentService.localQqSource().then((source) => {
                    finishedSources.push(source);
                });
            }
        });
        try {
            const started = componentService.runComponentAction(
                'qq',
                'local',
                'force_install',
                'qq-preview-install',
            );
            await vi.advanceTimersByTimeAsync(50);
            expect(await started).toBe('qq-preview-install');
            await vi.advanceTimersByTimeAsync(3200);
            expect(finishedSources).toEqual(['managed']);
        } finally {
            unsubscribe();
        }
    });
});
