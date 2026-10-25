import { describe, expect, it } from 'vitest';
import type { DeploymentTaskSnapshot } from '../../ipc/types';
import { initialActionProgress } from '../components/progress';
import { canCancelDeploymentTask, failureHint } from './display';
import type { TaskQueueItem } from './types';

function queueItem(overrides: Partial<TaskQueueItem> = {}): TaskQueueItem {
    return {
        id: 't1',
        kind: 'component_action',
        title: 'uv · 安装',
        hostId: 'remote:1',
        hostLabel: '修仙服务器',
        status: 'failed',
        startedAt: 0,
        progress: null,
        logHint: null,
        ...overrides,
    };
}

function failedProgress(
    logs: Array<{ level: 'info' | 'warn' | 'error'; message: string }>,
    message = '失败',
) {
    return {
        ...initialActionProgress,
        status: 'failed' as const,
        message,
        logs: logs.map((log, index) => ({ ...log, timestamp_ms: index })),
    };
}

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

describe('failureHint', () => {
    it('取最后一条带原因的 error 日志，跳过「失败」「任务失败」收尾文案', () => {
        const item = queueItem({
            progress: failedProgress([
                {
                    level: 'error',
                    message: '失败：直连 github.com · 下载中 · 763.00 KB / 17.72 MB · 0 B/s',
                },
                {
                    level: 'error',
                    message:
                        "step 'single' failed: download failed: https://github.com/astral-sh/uv/releases/download/0.12.8/uv-aarch64-unknown-linux-gnu.tar.gz: all mirrors failed: idle timeout: no bytes for 20s",
                },
                { level: 'error', message: '任务失败' },
            ]),
        });
        expect(failureHint(item)).toContain("step 'single' failed");
    });

    it('只有收尾文案时不把「失败」搬进顶部，给通用指引', () => {
        const item = queueItem({
            progress: failedProgress([{ level: 'error', message: '任务失败' }]),
        });
        expect(failureHint(item)).toBe('任务失败，展开步骤日志查看原因。');
    });

    it('没有进度快照的安装类任务回退到 logHint', () => {
        const item = queueItem({ kind: 'docker_install', logHint: '正在安装 Docker…' });
        expect(failureHint(item)).toBe('正在安装 Docker…');
    });

    it('非失败状态没有失败原因', () => {
        expect(failureHint(queueItem({ status: 'running' }))).toBeNull();
        expect(failureHint(queueItem({ status: 'success' }))).toBeNull();
    });
});
