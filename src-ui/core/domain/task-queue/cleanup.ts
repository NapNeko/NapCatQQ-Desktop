// 任务队列：已完成 / 失败 / 已取消条目在内存中的保留时长（与各 progress store 的 linger 对齐）。

export const TASK_QUEUE_CLEANUP_LINGER_OFF = 0;

/** 开启自动清理时的滑块范围（毫秒）。 */
export const TASK_QUEUE_CLEANUP_SLIDER_MIN = 3_000;
export const TASK_QUEUE_CLEANUP_SLIDER_MAX = 3_600_000; // 1 小时
export const TASK_QUEUE_CLEANUP_SLIDER_STEP = 1_000;

/** 默认：开启自动清理，保留 10 分钟（与历史 Docker 部署 / 安装 store 一致）。 */
export const DEFAULT_TASK_QUEUE_CLEANUP_WHEN_ENABLED_MS = 600_000;

export type TaskQueueCleanupPrefs = {
    taskQueueCleanupEnabled: boolean;
    taskQueueCleanupLingerMs: number;
};

export const DEFAULT_TASK_QUEUE_CLEANUP: TaskQueueCleanupPrefs = {
    taskQueueCleanupEnabled: true,
    taskQueueCleanupLingerMs: DEFAULT_TASK_QUEUE_CLEANUP_WHEN_ENABLED_MS,
};

export type TaskQueueCleanupDraftSlice = TaskQueueCleanupPrefs;

export function clampTaskQueueCleanupSliderMs(raw: unknown): number {
    if (typeof raw !== 'number' || !Number.isFinite(raw)) {
        return DEFAULT_TASK_QUEUE_CLEANUP_WHEN_ENABLED_MS;
    }
    const stepped =
        Math.round(raw / TASK_QUEUE_CLEANUP_SLIDER_STEP) *
        TASK_QUEUE_CLEANUP_SLIDER_STEP;
    return Math.max(
        TASK_QUEUE_CLEANUP_SLIDER_MIN,
        Math.min(TASK_QUEUE_CLEANUP_SLIDER_MAX, stepped),
    );
}

/** 落盘：0 表示关闭自动清理（仅 enabled 为 false 时写入 0）。 */
export function clampTaskQueueCleanupStoredMs(raw: unknown): number {
    if (typeof raw !== 'number' || !Number.isFinite(raw)) {
        return TASK_QUEUE_CLEANUP_LINGER_OFF;
    }
    const n = Math.round(raw);
    if (n <= 0) return TASK_QUEUE_CLEANUP_LINGER_OFF;
    return clampTaskQueueCleanupSliderMs(n);
}

export function taskQueueCleanupFromAppSettings(slice: {
    taskQueueCleanupEnabled?: boolean;
    taskQueueCleanupLingerMs?: bigint | number;
}): TaskQueueCleanupPrefs {
    if (slice.taskQueueCleanupEnabled === false) {
        return {
            taskQueueCleanupEnabled: false,
            taskQueueCleanupLingerMs: DEFAULT_TASK_QUEUE_CLEANUP_WHEN_ENABLED_MS,
        };
    }
    const lingerRaw = slice.taskQueueCleanupLingerMs;
    const hasLinger =
        lingerRaw !== undefined && lingerRaw !== null;
    const storedMs = hasLinger
        ? (typeof lingerRaw === 'bigint'
              ? clampTaskQueueCleanupStoredMs(Number(lingerRaw))
              : clampTaskQueueCleanupStoredMs(lingerRaw))
        : DEFAULT_TASK_QUEUE_CLEANUP_WHEN_ENABLED_MS;
    if (!hasLinger || storedMs > 0) {
        return {
            taskQueueCleanupEnabled: true,
            taskQueueCleanupLingerMs: storedMs > 0
                ? clampTaskQueueCleanupSliderMs(storedMs)
                : DEFAULT_TASK_QUEUE_CLEANUP_WHEN_ENABLED_MS,
        };
    }
    return {
        taskQueueCleanupEnabled: false,
        taskQueueCleanupLingerMs: DEFAULT_TASK_QUEUE_CLEANUP_WHEN_ENABLED_MS,
    };
}

export function taskQueueCleanupDraftFromStored(
    prefs: TaskQueueCleanupPrefs,
): TaskQueueCleanupDraftSlice {
    return { ...prefs };
}

/** 写入 app-settings.json 的扁平字段。 */
export function taskQueueCleanupToStoredFields(
    draft: TaskQueueCleanupDraftSlice,
): { taskQueueCleanupEnabled: boolean; taskQueueCleanupLingerMs: number } {
    if (!draft.taskQueueCleanupEnabled) {
        return {
            taskQueueCleanupEnabled: false,
            taskQueueCleanupLingerMs: TASK_QUEUE_CLEANUP_LINGER_OFF,
        };
    }
    return {
        taskQueueCleanupEnabled: true,
        taskQueueCleanupLingerMs: clampTaskQueueCleanupSliderMs(
            draft.taskQueueCleanupLingerMs,
        ),
    };
}

/** 当前是否应对终态任务安排移除计时器。 */
export function shouldScheduleTaskQueueTerminalCleanup(
    prefs: TaskQueueCleanupPrefs,
): boolean {
    return prefs.taskQueueCleanupEnabled;
}

/** 终态保留毫秒；关闭自动清理时返回 null。 */
export function taskQueueTerminalLingerMs(
    prefs: TaskQueueCleanupPrefs,
): number | null {
    if (!prefs.taskQueueCleanupEnabled) return null;
    return clampTaskQueueCleanupSliderMs(prefs.taskQueueCleanupLingerMs);
}