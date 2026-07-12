// 可选启动/路由性能标记。默认关闭，避免生产噪音。
// 打开方式（任一）：
//   - localStorage.ncd_perf_marks = '1'
//   - 开发构建（import.meta.env.DEV）且 localStorage 未显式设为 '0'
// 仅写 performance.mark / measure + console.debug，不发 IPC。

const STORAGE_KEY = 'ncd_perf_marks';

// 进程内缓存开关，避免每次 mark 都读 localStorage。
let enabledCache: boolean | null = null;
const onceMarks = new Set<string>();

function marksEnabled(): boolean {
    if (enabledCache !== null) return enabledCache;
    if (typeof window === 'undefined' || typeof localStorage === 'undefined') {
        enabledCache = false;
        return false;
    }
    try {
        const flag = localStorage.getItem(STORAGE_KEY);
        if (flag === '0') {
            enabledCache = false;
            return false;
        }
        if (flag === '1') {
            enabledCache = true;
            return true;
        }
    } catch {
        enabledCache = false;
        return false;
    }
    enabledCache = Boolean(import.meta.env.DEV);
    return enabledCache;
}

export type PerfMarkOptions = {
    // 同名 mark 只记一次（防 StrictMode 双挂载 / 重复 effect）。
    once?: boolean;
};

export function perfMark(name: string, options?: PerfMarkOptions): void {
    if (options?.once) {
        if (onceMarks.has(name)) return;
        onceMarks.add(name);
    }
    if (!marksEnabled()) return;
    try {
        performance.mark(name);
        // eslint-disable-next-line no-console
        console.debug(`[perf] mark ${name} @ ${performance.now().toFixed(1)}ms`);
    } catch {
        /* noop */
    }
}

export function perfMeasure(name: string, startMark: string, endMark: string): void {
    if (!marksEnabled()) return;
    try {
        performance.measure(name, startMark, endMark);
        const entries = performance.getEntriesByName(name, 'measure');
        const last = entries[entries.length - 1];
        if (last) {
            // eslint-disable-next-line no-console
            console.debug(`[perf] measure ${name} = ${last.duration.toFixed(1)}ms`);
        }
    } catch {
        /* noop */
    }
}
