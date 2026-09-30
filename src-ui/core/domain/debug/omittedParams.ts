// 后端写历史和事件流前会给参数瘦身（ncd-runtime 的 params.rs）：
// - 超过 64 KiB 的字符串叶子换成占位文字 `<已省略 N 字节>`（N 是原字节数）；
// - 整体仍超过 256 KiB 的，顶层短标量留着、其余字段各换成占位；还超的直接收成
//   `{"_omitted": "<参数共 N 字节，已省略>"}`。
// 这些占位文字发出去必然失败（图裂、接口报参数错），而占位本身看不出哪错了，
// 所以重放 / 收藏 / 发送前都要先认出来。

const MARKER = /^<已省略 \d+ 字节>$/;

/** 整份被收成摘要的对象：只有一个 `_omitted` 键 */
export function isFullyOmitted(value: unknown): boolean {
    return typeof value === 'object' && value !== null && !Array.isArray(value) && '_omitted' in value;
}

/** 占位文字（含整体摘要里留着的那几条短标量也算不进去，只数真正被换掉的叶子） */
function countIn(value: unknown): number {
    if (typeof value === 'string') return MARKER.test(value) ? 1 : 0;
    if (Array.isArray(value)) {
        let n = 0;
        for (const v of value) n += countIn(v);
        return n;
    }
    if (value !== null && typeof value === 'object') {
        if (isFullyOmitted(value)) return 1;
        let n = 0;
        for (const v of Object.values(value as Record<string, unknown>)) n += countIn(v);
        return n;
    }
    return 0;
}

/** 参数（JSON 值）里有多少处被瘦身留下的占位；没有是 0 */
export function countOmittedInValue(value: unknown): number {
    return countIn(value);
}

/** 参数文本（tab 的 params_text）里有多少处占位；JSON 写坏了算 0（自有语法报错过） */
export function countOmittedParams(paramsText: string): number {
    // 大字符串每敲一个字都整段 JSON.parse 不划算；没有这两个子串直接 0
    if (!paramsText.includes('已省略') && !paramsText.includes('_omitted')) return 0;
    let value: unknown;
    try {
        value = JSON.parse(paramsText);
    } catch {
        return 0;
    }
    return countIn(value);
}

/** 发送拦住的提示文案；没拦住的事返回 null */
export function omittedBlocker(count: number): string | null {
    return count > 0 ? `有 ${count} 处超长参数在存盘时被省略，补上原文再发` : null;
}
