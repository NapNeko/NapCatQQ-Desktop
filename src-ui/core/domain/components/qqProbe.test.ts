import { describe, expect, it } from 'vitest';
import { initialActionProgress } from './progress';
import { findQqSudoElevationTask, type QqSudoActionKind } from './qqProbe';

describe('QQ sudo retry target', () => {
    const failed = {
        ...initialActionProgress,
        status: 'failed' as const,
        logs: [
            {
                level: 'error' as const,
                message: "step 'single' failed: elevation_required: 请输入 sudo 密码",
                timestamp_ms: 1,
            },
        ],
    };

    it.each<QqSudoActionKind>([
        'update',
        'force_install',
        'ensure_installed',
        'ensure_dependencies',
    ])('preserves the original %s action', (kind) => {
        expect(
            findQqSudoElevationTask(
                { failed },
                { failed: { componentId: 'qq', hostId: 'remote:one', kind } },
                new Set(),
            ),
        ).toEqual({
            componentId: 'qq',
            taskId: 'failed',
            hostId: 'remote:one',
            kind,
            reason: '请输入 sudo 密码',
        });
    });

    it('allows a local QQ dependency failure to request a password', () => {
        expect(
            findQqSudoElevationTask(
                { failed },
                { failed: { componentId: 'qq', hostId: 'local', kind: 'ensure_dependencies' } },
                new Set(),
            ),
        ).toEqual({
            componentId: 'qq',
            taskId: 'failed',
            hostId: 'local',
            kind: 'ensure_dependencies',
            reason: '请输入 sudo 密码',
        });
    });

    it('ignores ordinary failures, missing actions and already shown prompts', () => {
        const targets = {
            failed: { componentId: 'qq' as const, hostId: 'remote:one', kind: 'update' as const },
        };
        expect(
            findQqSudoElevationTask({ failed: { ...failed, logs: [] } }, targets, new Set()),
        ).toBeNull();
        expect(findQqSudoElevationTask({ failed }, targets, new Set(['failed']))).toBeNull();

        expect(
            findQqSudoElevationTask(
                { failed },
                { failed: { ...targets.failed, kind: undefined } },
                new Set(),
            ),
        ).toBeNull();
        expect(
            findQqSudoElevationTask(
                { failed },
                { failed: { ...targets.failed, kind: 'uninstall' } },
                new Set(),
            ),
        ).toBeNull();
    });

    it('keeps the elevation reason when generic failure logs follow it', () => {
        expect(
            findQqSudoElevationTask(
                {
                    failed: {
                        ...failed,
                        message: '失败',
                        logs: [
                            ...failed.logs,
                            { level: 'error', message: '任务失败', timestamp_ms: 2 },
                        ],
                    },
                },
                { failed: { componentId: 'novnc', hostId: 'local', kind: 'ensure_installed' } },
                new Set(),
            ),
        ).toMatchObject({ componentId: 'novnc', reason: '请输入 sudo 密码' });
    });
});
