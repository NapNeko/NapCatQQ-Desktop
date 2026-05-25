// 把 ProgressEvent 流派生为 UI 渲染的 ActionProgressView。
// 纯函数 reducer：(prev, event) → next。

import type { LogLevel, ProgressEvent } from '../../ipc/types';

export type ActionStatus = 'pending' | 'running' | 'paused' | 'success' | 'failed' | 'cancelled';

export interface ActionLogLine {
    level: LogLevel;
    message: string;
    timestamp_ms: number;
}

export interface ActionProgressView {
    status: ActionStatus;
    /// 当前步骤号（1-based），未开始时 0
    currentStep: number;
    /// 总步骤数（未开始时 0）
    totalSteps: number;
    /// 当前 step 的百分比 0-100
    percent: number;
    /// 当前 step 的提示文字
    message: string;
    /// 当前下载步骤的瞬时速度（字节/秒），非下载步骤为 null
    speedBps: number | null;
    /// 累计 log（最多保留 50 条）
    logs: ActionLogLine[];
}

const MAX_LOGS = 50;

export const initialActionProgress: ActionProgressView = {
    status: 'pending',
    currentStep: 0,
    totalSteps: 0,
    percent: 0,
    message: '',
    speedBps: null,
    logs: [],
};

export function reduceActionProgress(
    prev: ActionProgressView,
    event: ProgressEvent,
): ActionProgressView {
    switch (event.kind) {
        case 'started':
            return {
                ...prev,
                status: 'running',
                totalSteps: event.total_steps,
                currentStep: 0,
                percent: 0,
                message: '准备中…',
                speedBps: null,
            };
        case 'step_begin':
            return {
                ...prev,
                status: 'running',
                currentStep: event.step,
                percent: 0,
                message: event.message,
                speedBps: null,
            };
        case 'step_progress':
            return {
                ...prev,
                status: 'running',
                currentStep: event.step,
                percent: event.percent,
                message: event.message,
                speedBps:
                    event.speed_bps == null ? null : Number(event.speed_bps),
            };
        case 'step_end':
            return {
                ...prev,
                percent: 100,
                speedBps: null,
            };
        case 'finished':
            return {
                ...prev,
                status: event.ok ? 'success' : 'failed',
                percent: 100,
                message: event.ok ? '完成' : '失败',
                speedBps: null,
            };
        case 'log': {
            // ts-rs 把 Rust u64 派生为 bigint；UI 侧统一用 number（Unix ms 在
            // Number 安全整数范围内，到公元 285 千年才溢出）。边界转换放
            // domain 层，参考 core/domain/release/normalize.ts。
            const next: ActionLogLine[] = [
                ...prev.logs,
                {
                    level: event.level,
                    message: event.message,
                    timestamp_ms: Number(event.timestamp_ms),
                },
            ];
            if (next.length > MAX_LOGS) next.splice(0, next.length - MAX_LOGS);
            return { ...prev, logs: next };
        }
        default:
            return prev;
    }
}
