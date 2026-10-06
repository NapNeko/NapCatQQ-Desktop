// 按行对比两段文字（最长公共子序列）。提示词几十上百行，O(n·m) 够用。

export type DiffLine = { kind: 'same' | 'add' | 'del'; text: string };

export function lineDiff(before: string, after: string): DiffLine[] {
    const a = before.split('\n');
    const b = after.split('\n');
    const n = a.length;
    const m = b.length;
    // lcs[i][j]：a[i..] 和 b[j..] 的公共行数
    const lcs: number[][] = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0));
    for (let i = n - 1; i >= 0; i -= 1) {
        for (let j = m - 1; j >= 0; j -= 1) {
            lcs[i]![j] =
                a[i] === b[j]
                    ? lcs[i + 1]![j + 1]! + 1
                    : Math.max(lcs[i + 1]![j]!, lcs[i]![j + 1]!);
        }
    }
    const out: DiffLine[] = [];
    let i = 0;
    let j = 0;
    while (i < n && j < m) {
        if (a[i] === b[j]) {
            out.push({ kind: 'same', text: a[i]! });
            i += 1;
            j += 1;
        } else if (lcs[i + 1]![j]! >= lcs[i]![j + 1]!) {
            out.push({ kind: 'del', text: a[i]! });
            i += 1;
        } else {
            out.push({ kind: 'add', text: b[j]! });
            j += 1;
        }
    }
    for (; i < n; i += 1) out.push({ kind: 'del', text: a[i]! });
    for (; j < m; j += 1) out.push({ kind: 'add', text: b[j]! });
    return out;
}

/** 改动的行数：加的和删的各算一行 */
export function changedLines(diff: readonly DiffLine[]): number {
    return diff.filter((d) => d.kind !== 'same').length;
}
