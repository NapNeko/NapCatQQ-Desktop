// 两份回包 JSON 的行级对比：LCS（最长公共子序列）行 diff，配成一行行左右对齐的对子——
// 左边删的、右边增的高亮，一样的两边并排。
//
// 回包可能很大（截断前最多 256 KiB），行数乘积超过上限时不做全量 LCS：掐头去尾，
// 共同前缀 / 后缀不动，中段全部算改，免得把主线程卡死。

export type DiffRowKind = 'same' | 'add' | 'remove';

export interface DiffRow {
    kind: DiffRowKind;
    /** 左（旧）侧的行；「增」时是 null */
    left: string | null;
    /** 右（新）侧的行；「删」时是 null */
    right: string | null;
}

/** LCS 全表允许的最大格子数（约 450×450 行）；超出就走掐头去尾 */
const MAX_CELLS = 200_000;

function splitLines(text: string): string[] {
    if (text === '') return [];
    const lines = text.split('\n');
    // split 在结尾换行后再补一个空串，那不是一行
    if (lines[lines.length - 1] === '') lines.pop();
    return lines;
}

function lcsDiff(a: string[], b: string[]): DiffRow[] {
    const n = a.length;
    const m = b.length;
    // dp[i][j] = a[i:] 和 b[j:] 的 LCS 长度，从右下角往回填
    const dp = new Uint32Array((n + 1) * (m + 1));
    const at = (i: number, j: number) => i * (m + 1) + j;
    for (let i = n - 1; i >= 0; i -= 1) {
        for (let j = m - 1; j >= 0; j -= 1) {
            dp[at(i, j)] =
                a[i] === b[j]
                    ? dp[at(i + 1, j + 1)]! + 1
                    : Math.max(dp[at(i + 1, j)]!, dp[at(i, j + 1)]!);
        }
    }

    const rows: DiffRow[] = [];
    let i = 0;
    let j = 0;
    while (i < n && j < m) {
        if (a[i] === b[j]) {
            rows.push({ kind: 'same', left: a[i]!, right: b[j]! });
            i += 1;
            j += 1;
        } else if (dp[at(i + 1, j)]! >= dp[at(i, j + 1)]!) {
            // 删优先：一块改动里先排左边删掉的行，再排右边新增的行
            rows.push({ kind: 'remove', left: a[i]!, right: null });
            i += 1;
        } else {
            rows.push({ kind: 'add', left: null, right: b[j]! });
            j += 1;
        }
    }
    while (i < n) rows.push({ kind: 'remove', left: a[i++]!, right: null });
    while (j < m) rows.push({ kind: 'add', left: null, right: b[j++]! });
    return rows;
}

/** 大行数的退化方案：共同头尾之外的中段全部按「先删后增」 */
function headTailDiff(a: string[], b: string[]): DiffRow[] {
    let head = 0;
    while (head < a.length && head < b.length && a[head] === b[head]) head += 1;
    let tail = 0;
    while (
        tail < a.length - head &&
        tail < b.length - head &&
        a[a.length - 1 - tail] === b[b.length - 1 - tail]
    )
        tail += 1;

    const rows: DiffRow[] = [];
    for (let k = 0; k < head; k += 1) rows.push({ kind: 'same', left: a[k]!, right: b[k]! });
    for (let k = head; k < a.length - tail; k += 1)
        rows.push({ kind: 'remove', left: a[k]!, right: null });
    for (let k = head; k < b.length - tail; k += 1)
        rows.push({ kind: 'add', left: null, right: b[k]! });
    for (let k = 0; k < tail; k += 1)
        rows.push({ kind: 'same', left: a[a.length - tail + k]!, right: b[b.length - tail + k]! });
    return rows;
}

/** before（较旧）和 after（较新）两份文本的行 diff 对子；空文本按 0 行算 */
export function diffLines(before: string, after: string): DiffRow[] {
    const a = splitLines(before);
    const b = splitLines(after);
    if (a.length === 0 && b.length === 0) return [];
    if (a.length * b.length > MAX_CELLS) return headTailDiff(a, b);
    // 一边为空时 LCS 也退化成全删 / 全增，不用特判
    return lcsDiff(a, b);
}

/** 是不是完全一样（一行改动都没有）；对比框据此可以不用翻来翻去找差异 */
export function diffAllSame(rows: readonly DiffRow[]): boolean {
    return rows.every((r) => r.kind === 'same');
}
