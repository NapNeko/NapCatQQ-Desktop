// 响应面板的辅助：表格视图能不能用、字节数怎么写、哪些键值可以点、「复制回包」怎么写。

/** 表格最多显示这么多列；字段再多的对象数组看表格已经没意义，退回 JSON 树 */
const MAX_COLUMNS = 30;

const isPlainObject = (v: unknown): v is Record<string, unknown> =>
    typeof v === 'object' && v !== null && !Array.isArray(v);

/**
 * `data` 是「至少一个元素、且每个元素都是对象」的数组时给出表格数据，否则 null（这个视图不可用）。
 * 列是所有行的键并集，按第一次出现的先后排，最多 30 列。
 */
export function tableView(data: unknown): { columns: string[]; rows: Array<Record<string, unknown>> } | null {
    if (!Array.isArray(data) || data.length === 0) return null;
    const columns = new Set<string>();
    for (const row of data) {
        if (!isPlainObject(row)) return null;
        if (columns.size < MAX_COLUMNS) {
            for (const key of Object.keys(row)) {
                columns.add(key);
                if (columns.size >= MAX_COLUMNS) break;
            }
        }
    }
    return { columns: [...columns], rows: data as Array<Record<string, unknown>> };
}

const UNITS = ['B', 'KB', 'MB', 'GB', 'TB'];

/** 1024 进制，一位小数，整数不带 .0：`512 B`、`1.5 KB`、`256 KB`、`5 MB` */
export function formatBytes(n: number): string {
    if (!Number.isFinite(n) || n <= 0) return '0 B';
    let value = n;
    let unit = 0;
    while (value >= 1024 && unit < UNITS.length - 1) {
        value /= 1024;
        unit += 1;
    }
    if (unit === 0) return `${Math.round(value)} B`;
    const fixed = value.toFixed(1);
    return `${fixed.endsWith('.0') ? fixed.slice(0, -2) : fixed} ${UNITS[unit]}`;
}

/** 响应里点了能「填进请求」或「用它新开查询」的键 */
export function isClickableId(key: string): 'group_id' | 'user_id' | 'message_id' | null {
    return key === 'group_id' || key === 'user_id' || key === 'message_id' ? key : null;
}

/**
 * 「复制回包」按钮的名字和复制成功的提示：截断时复制的只是开头 256 KiB 的文本预览
 * （预览上限和后端 / mock 的 RAW_PREVIEW_LIMIT 一致），文案不能再说「完整回包」
 */
export function copyAllResponseCopy(truncated: boolean): { label: string; toast: string } {
    return truncated
        ? { label: '复制开头 256 KiB 的预览', toast: '已复制开头 256 KiB，完整内容请另存' }
        : { label: '复制完整回包', toast: '已复制完整回包' };
}

const two = (n: number) => String(n).padStart(2, '0');

/** 另存完整回包时的默认文件名：接口名 + 本地时间，存多份不会互相覆盖，一眼看得出是哪次 */
export function responseFileName(action: string, atMs: number): string {
    const d = new Date(atMs);
    const stamp = `${d.getFullYear()}${two(d.getMonth() + 1)}${two(d.getDate())}-${two(d.getHours())}${two(d.getMinutes())}${two(d.getSeconds())}`;
    // 文件名里不该有的字符（目录外的接口名是用户随手敲的）换成下划线
    const base = action.trim().replace(/[^\w.-]+/g, '_') || 'response';
    return `${base}-${stamp}.json`;
}
