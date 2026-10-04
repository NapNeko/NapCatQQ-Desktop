// 应用端框架的版本比较。**必须按 PEP 440，不能复用 release/normalize 的 compareSemver**：
//
// 那个是给 Node 系 / QQ 用的 SemVer 口径，它的预发布启发式只认连字符形式
// （`1.2.3-rc1`），而 Python 生态的版本是 PEP 440 拼写——`1.2.1a1` / `1.0.0rc1`
// 都没有连字符。把 `1.2.1a1` 丢给 parseNumericParts 会切成 [1,2,1]，与 `1.2.0`
// 比出「1.2.0 更新」，于是「可更新」提示方向相反。
//
// 与后端 crates/ncd-appframework/src/neobot/versions.rs 的规则保持一致：
// epoch > release 逐段（短补 0）> dev < a < b < rc < 正式 < post。

export type PreKind = 'a' | 'b' | 'rc';

export interface Pep440 {
    epoch: number;
    release: number[];
    pre: { kind: PreKind; num: number } | null;
    post: number | null;
    dev: number | null;
}

const PRE_RANK: Record<PreKind, number> = { a: 0, b: 1, rc: 2 };

/** 是否是预发布（PEP 440：有 pre 或 dev 都算） */
export function isPrerelease(v: Pep440): boolean {
    return v.pre !== null || v.dev !== null;
}

/**
 * 解析 PEP 440 的关键部分。认不出的拼写返回 null（上游历史上有过畸形版本号），
 * 调用方应把它当「无法比较」而不是当最低版本。
 */
export function parsePep440(raw: string): Pep440 | null {
    const s = raw.trim().toLowerCase();
    if (!s) return null;

    // epoch：`1!` 前缀
    let epoch = 0;
    let rest = s;
    const bang = s.indexOf('!');
    if (bang >= 0) {
        const head = s.slice(0, bang);
        if (!/^\d+$/.test(head)) return null;
        epoch = Number.parseInt(head, 10);
        rest = s.slice(bang + 1);
    }

    // release：开头连续的「数字.数字...」
    const relMatch = rest.match(/^(\d+(?:\.\d+)*)/);
    if (!relMatch) return null;
    const release = relMatch[1].split('.').map((p) => Number.parseInt(p, 10));
    let tail = rest.slice(relMatch[1].length);

    // 归一化分隔符：`-` / `_` 当 `.`
    tail = tail.replace(/[-_]/g, '.');

    let dev: number | null = null;
    let post: number | null = null;
    let pre: { kind: PreKind; num: number } | null = null;

    // 关键字和数字之间允许一个点：`1.0.0-alpha.23` 归一化后是 `.alpha.23`
    const takeNumber = (from: number): number => {
        const m = tail.slice(from).match(/^\.?(\d+)/);
        return m ? Number.parseInt(m[1], 10) : 0;
    };

    const devAt = tail.indexOf('dev');
    if (devAt >= 0) {
        dev = takeNumber(devAt + 3);
        tail = tail.slice(0, devAt);
    }
    const postAt = tail.indexOf('post');
    if (postAt >= 0) {
        post = takeNumber(postAt + 4);
        tail = tail.slice(0, postAt);
    }
    // pre 关键字长的优先，否则 `pre` 会先匹配到 `p`、`rc` 会被 `c` 抢走
    for (const [needle, kind] of [
        ['alpha', 'a'],
        ['beta', 'b'],
        ['preview', 'rc'],
        ['pre', 'rc'],
        ['rc', 'rc'],
        ['a', 'a'],
        ['b', 'b'],
        ['c', 'rc'],
    ] as [string, PreKind][]) {
        const at = tail.indexOf(needle);
        if (at >= 0) {
            pre = { kind, num: takeNumber(at + needle.length) };
            break;
        }
    }

    return { epoch, release, pre, post, dev };
}

/**
 * PEP 440 排序。返回数与 `compareSemver` 同极性：
 *   - **> 0  remote 比 local 新**（即「有更新」）
 *   - < 0  local 比 remote 新
 *   - 0    一致 / 任一侧解析不了（不猜）
 */
export function compareAppVersion(local: string, remote: string): number {
    const a = parsePep440(local);
    const b = parsePep440(remote);
    // 解析不了就不下结论：宁可漏报可更新，也不要报错方向
    if (!a || !b) return 0;

    if (a.epoch !== b.epoch) return b.epoch - a.epoch;

    const len = Math.max(a.release.length, b.release.length);
    for (let i = 0; i < len; i += 1) {
        const x = a.release[i] ?? 0;
        const y = b.release[i] ?? 0;
        if (x !== y) return y - x;
    }

    // rank：dev 最低(0) < 预发布(1+kind) < 正式(4) < post(5)
    const rank = (v: Pep440): number => {
        if (v.dev !== null && v.pre === null && v.post === null) return 0;
        if (v.pre) return 1 + PRE_RANK[v.pre.kind];
        if (v.post !== null) return 5;
        return 4;
    };
    const ra = rank(a);
    const rb = rank(b);
    if (ra !== rb) return rb - ra;

    const na = a.pre?.num ?? 0;
    const nb = b.pre?.num ?? 0;
    if (na !== nb) return nb - na;

    const pa = a.post ?? -1;
    const pb = b.post ?? -1;
    if (pa !== pb) return pb - pa;

    // dev 越大越新，但都低于非 dev；两个都非 dev 时上面已判等
    const da = a.dev ?? Number.MAX_SAFE_INTEGER;
    const db = b.dev ?? Number.MAX_SAFE_INTEGER;
    if (da !== db) return db - da;
    return 0;
}

/** 是否有更新：装了 local、上游最新正式版是 remote */
export function hasAppUpdate(installed: string | undefined | null, latest: string | null): boolean {
    if (!installed || !latest) return false;
    return compareAppVersion(installed, latest) > 0;
}

/** 版本列表里挑「最新正式版」，列表默认已由后端降序排好；兜底扫一遍 */
export function latestStable(versions: readonly string[]): string | null {
    for (const v of versions) {
        const p = parsePep440(v);
        if (p && !isPrerelease(p)) return v;
    }
    return null;
}
