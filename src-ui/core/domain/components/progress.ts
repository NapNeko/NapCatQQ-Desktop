// 把 ProgressEvent 流派生为 UI 渲染的 ActionProgressView。
// 纯函数 reducer：(prev, event) → next。
//
// 下载阶段会带额外字段（downloadedBytes / totalBytes / speedBps / downloadStage），
// 这些字段只在 step_progress 事件里有，且要在 step_end / finished 时清零。
// 不直接解析 message 字符串：所有数字字段从后端结构化字段拿。

import type { DockerPullLayerSnapshot, ProgressEvent, ProgressLogLevel } from '../../ipc/types';

export type ActionStatus = 'pending' | 'running' | 'paused' | 'success' | 'failed' | 'cancelled';

/// 后端 download_stage 字符串的强类型映射。未识别的字符串走 'unknown'
/// 让 UI fallback 到 message 文本。
export type DownloadStage =
    | 'racing'
    | 'streaming'
    | 'switching_mirror'
    | 'resuming'
    | 'unknown';

export interface ActionLogLine {
    level: ProgressLogLevel;
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
    /// 已下载字节，仅下载步骤为非 null
    downloadedBytes: number | null;
    /// 总字节数，仅下载且 Content-Length 已知时为非 null
    totalBytes: number | null;
    /// 下载阶段，仅下载步骤为非 null
    downloadStage: DownloadStage | null;
    /// docker pull 各层进度；仅拉镜像步骤有值
    dockerLayers: DockerPullLayerSnapshot[];
    /// 累计 log（最多保留 50 条）
    logs: ActionLogLine[];
}

const MAX_LOGS = 200;

export const initialActionProgress: ActionProgressView = {
    status: 'pending',
    currentStep: 0,
    totalSteps: 0,
    percent: 0,
    message: '',
    speedBps: null,
    downloadedBytes: null,
    totalBytes: null,
    downloadStage: null,
    dockerLayers: [],
    logs: [],
};

function clearDownloadFields<T extends ActionProgressView>(v: T): T {
    return {
        ...v,
        speedBps: null,
        downloadedBytes: null,
        totalBytes: null,
        downloadStage: null,
        dockerLayers: [],
    };
}

function toNumberOrNull(x: bigint | number | null | undefined): number | null {
    if (x == null) return null;
    return typeof x === 'bigint' ? Number(x) : x;
}

function toDownloadStage(s: string | null | undefined): DownloadStage | null {
    if (!s) return null;
    switch (s) {
        case 'racing':
        case 'streaming':
        case 'switching_mirror':
        case 'resuming':
            return s;
        default:
            return 'unknown';
    }
}

export function reduceActionProgress(
    prev: ActionProgressView,
    event: ProgressEvent,
): ActionProgressView {
    switch (event.kind) {
        case 'started':
            return clearDownloadFields({
                ...prev,
                status: 'running',
                totalSteps: event.total_steps,
                currentStep: 0,
                percent: 0,
                message: '准备中…',
            });
        case 'step_begin':
            return clearDownloadFields({
                ...prev,
                status: 'running',
                currentStep: event.step,
                percent: 0,
                message: event.message,
            });
        case 'step_progress':
            return {
                ...prev,
                status: 'running',
                currentStep: event.step,
                percent: event.percent,
                message: event.message,
                speedBps: toNumberOrNull(event.speed_bps),
                downloadedBytes: toNumberOrNull(event.downloaded_bytes),
                totalBytes: toNumberOrNull(event.total_bytes),
                downloadStage: toDownloadStage(event.download_stage),
                dockerLayers: event.docker_layers ?? [],
            };
        case 'step_end':
            return clearDownloadFields({ ...prev, percent: 100 });
        case 'finished':
            return clearDownloadFields({
                ...prev,
                status: event.ok ? 'success' : 'failed',
                percent: 100,
                message: event.ok ? '完成' : '失败',
            });
        case 'log': {
            const next: ActionLogLine[] = [
                ...prev.logs,
                {
                    level: event.level,
                    message: event.message,
                    timestamp_ms: Number(event.timestamp_ms),
                },
            ];
            if (next.length > MAX_LOGS) next.splice(0, next.length - MAX_LOGS);
            const hint =
                event.level === 'error' || event.level === 'warn'
                    ? event.message
                    : event.message.trim();
            const messagePatch =
                prev.status === 'running' && hint.length > 0
                    ? { message: hint }
                    : {};
            return { ...prev, logs: next, ...messagePatch };
        }
        default:
            return prev;
    }
}

// ─── 派生字段 / 格式化 ─────────────────────────────────────────────────────

/// ETA 秒数：剩余字节 / 速度。任一缺失或速度过慢（< 1 KB/s）返回 null
/// 避免显示 "ETA 999:99:99"。
export function deriveEtaSeconds(view: ActionProgressView): number | null {
    const { downloadedBytes, totalBytes, speedBps } = view;
    if (downloadedBytes == null || totalBytes == null || speedBps == null) return null;
    if (speedBps < 1024) return null;
    if (totalBytes <= downloadedBytes) return 0;
    const remain = totalBytes - downloadedBytes;
    return Math.ceil(remain / speedBps);
}

/// 进度条是否走 indeterminate（无确定百分比）。
/// race / 切镜像阶段没有总进度，UI 以无定值条纹动画呈现。
export function isIndeterminate(view: ActionProgressView): boolean {
    if (view.downloadStage === 'racing' || view.downloadStage === 'switching_mirror') {
        return true;
    }
    if (view.dockerLayers.length > 0) {
        return false;
    }
    if (view.percent <= 0 && view.status === 'running') return true;
    return false;
}

const KB = 1024;
const MB = 1024 * 1024;
const GB = 1024 * 1024 * 1024;

export function formatBytes(n: number | null): string {
    if (n == null) return '—';
    if (n >= GB) return `${(n / GB).toFixed(2)} GB`;
    if (n >= MB) return `${(n / MB).toFixed(1)} MB`;
    if (n >= KB) return `${(n / KB).toFixed(0)} KB`;
    return `${n} B`;
}

export function formatSpeed(bps: number | null): string {
    if (bps == null) return '—';
    if (bps >= MB) return `${(bps / MB).toFixed(1)} MB/s`;
    if (bps >= KB) return `${(bps / KB).toFixed(0)} KB/s`;
    return `${bps} B/s`;
}

export function formatEta(seconds: number | null): string {
    if (seconds == null) return '—';
    if (seconds < 1) return '< 1s';
    if (seconds < 60) return `${seconds}s`;
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    if (m < 60) return `${m}:${s.toString().padStart(2, '0')}`;
    const h = Math.floor(m / 60);
    const mm = m % 60;
    return `${h}:${mm.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
}

/// 阶段中文标签。UI 副标题用，如 "正在选择镜像 · 850 KB/s"。
export function downloadStageLabel(stage: DownloadStage | null): string | null {
    switch (stage) {
        case 'racing':
            return '选择镜像';
        case 'streaming':
            return '下载中';
        case 'switching_mirror':
            return '切换镜像';
        case 'resuming':
            return '续传中';
        default:
            return null;
    }
}
