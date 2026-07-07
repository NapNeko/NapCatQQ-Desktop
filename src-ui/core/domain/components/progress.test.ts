import { describe, expect, it } from 'vitest';

import type { ProgressEvent, ProgressKind } from '../../ipc/types';
import { initialActionProgress, reduceActionProgress } from './progress';

function event(
    timestampMs: number,
    body: ProgressKind,
): ProgressEvent {
    return {
        v: 1,
        timestamp_ms: BigInt(timestampMs),
        ...body,
    } as ProgressEvent;
}

describe('reduceActionProgress', () => {
    it('derives task-level progress across multiple steps', () => {
        let progress = initialActionProgress;

        progress = reduceActionProgress(progress, event(1000, {
            kind: 'started',
            total_steps: 4,
        }));
        progress = reduceActionProgress(progress, event(1100, {
            kind: 'step_begin',
            step: 1,
            message: 'download package',
        }));
        progress = reduceActionProgress(progress, event(1200, {
            kind: 'step_progress',
            step: 1,
            percent: 50,
            message: 'download 50%',
        }));

        expect(progress.percent).toBe(50);
        expect(progress.overallPercent).toBe(13);

        progress = reduceActionProgress(progress, event(1300, {
            kind: 'step_end',
            step: 1,
            ok: true,
        }));

        expect(progress.overallPercent).toBe(25);
        expect(progress.logs.map((line) => line.message)).toEqual([
            '开始：download package',
            '完成：download package',
        ]);
    });

    it('keeps package-manager step_progress events inside the active step', () => {
        let progress = initialActionProgress;

        progress = reduceActionProgress(progress, event(1000, {
            kind: 'started',
            total_steps: 1,
        }));
        progress = reduceActionProgress(progress, event(1100, {
            kind: 'step_begin',
            step: 1,
            message: 'install dependencies',
        }));
        progress = reduceActionProgress(progress, event(1200, {
            kind: 'step_progress',
            step: 0,
            percent: 40,
            message: 'install libx11',
        }));

        expect(progress.currentStep).toBe(1);
        expect(progress.percent).toBe(40);
        expect(progress.overallPercent).toBe(40);
    });
});
