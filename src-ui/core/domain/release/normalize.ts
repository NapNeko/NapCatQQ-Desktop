// ReleaseSnapshot 的 domain 派生层。
//
// 后端给前端的 `published_at` / `fetched_at` 是 bigint（ts-rs 把 Rust u64
// 派生成 bigint，避免 JS Number 范围溢出）。但 Unix epoch 秒在 JS Number
// 安全整数范围内（要到公元 285 千年才溢出），UI 用 number 更省心：
// `new Date(num * 1000)` 直接可用，bigint 不能和 number 做算术。
//
// 这一层负责：
//   1. bigint → number 转换（ReleaseInfoView / ReleaseSnapshotView）
//   2. 与本地版本对比，派生"有更新可用"语义
//
// 纯函数 + 无 IPC / 无 React。

import type {
    LocalVersionSnapshot,
    ReleaseInfo,
    ReleaseSnapshot,
} from '../../ipc/types';

/// UI 友好的 ReleaseInfo：bigint 已转 number。
export interface ReleaseInfoView {
    version: string;
    publishedAt: number;
    htmlUrl: string;
    releaseNotes: string;
}

/// UI 友好的 ReleaseSnapshot。
export interface ReleaseSnapshotView {
    napcat: ReleaseInfoView | null;
    snowluma: ReleaseInfoView | null;
    desktop: ReleaseInfoView | null;
    /// `null` 表示从未成功拉过。
    fetchedAt: number | null;
}

function toView(info: ReleaseInfo | null | undefined): ReleaseInfoView | null {
    if (!info) return null;
    return {
        version: info.version,
        publishedAt: Number(info.published_at),
        htmlUrl: info.html_url,
        releaseNotes: info.release_notes,
    };
}

export function normalizeReleaseSnapshot(snap: ReleaseSnapshot | null | undefined): ReleaseSnapshotView {
    if (!snap) {
        return { napcat: null, snowluma: null, desktop: null, fetchedAt: null };
    }
    return {
        napcat: toView(snap.napcat_latest),
        snowluma: toView(snap.snowluma_latest),
        desktop: toView(snap.desktop_latest),
        fetchedAt: snap.fetched_at !== null && snap.fetched_at !== undefined
            ? Number(snap.fetched_at)
            : null,
    };
}

// ─── 版本比对 ────────────────────────────────────────────────────────────

/// 简化版本比对：拆 `1.2.3` / `1.2.3-rc1` 这类 SemVer 数字段后逐位比较。
/// 拆不出数字段（比如 build metadata）直接走字典序兜底。
///
/// 返回：
///   - > 0  remote 比 local 新
///   - < 0  local 比 remote 新（理论上不该发生，但用户改了 napcat.mjs 也可能）
///   - 0    一致 / 无法比较
export function compareSemver(local: string, remote: string): number {
    const localParts = parseNumericParts(local);
    const remoteParts = parseNumericParts(remote);
    const len = Math.max(localParts.length, remoteParts.length);
    for (let i = 0; i < len; i++) {
        const a = localParts[i] ?? 0;
        const b = remoteParts[i] ?? 0;
        if (a !== b) return b - a;
    }
    // 数字段全相等：比 prerelease（有 prerelease 的版本算更老，遵循 SemVer 语义）
    const localHasPre = local.includes('-');
    const remoteHasPre = remote.includes('-');
    if (localHasPre && !remoteHasPre) return 1;
    if (!localHasPre && remoteHasPre) return -1;
    return 0;
}

function parseNumericParts(version: string): number[] {
    // 去掉 v 前缀，截到第一个 `-` / `+` 之前的数字段
    const stripped = version.replace(/^[vV]/, '');
    const core = stripped.split(/[-+]/)[0];
    return core.split('.').map((part) => {
        const n = Number.parseInt(part, 10);
        return Number.isFinite(n) ? n : 0;
    });
}

// ─── 派生：有没有更新可用 ─────────────────────────────────────────────────

export type UpdatableProject = 'napcat' | 'snowluma' | 'desktop';

export interface UpdateAvailableItem {
    project: UpdatableProject;
    /// 当前安装版本；未安装时是 null。
    localVersion: string | null;
    /// 远端最新版本。
    remoteVersion: string;
    htmlUrl: string;
    releaseNotes: string;
}

/// 比对本地 + 远端，得出"哪些 project 有更新可用"。
/// Desktop 当前版本暂不做派生（前端不知道自己哪个 build），只在 local
/// 完全空时把 desktop release 当"提示当前发布版"显示，由 UI 决定是否提醒。
///
/// 规则：
///   - 远端 release info 缺失 → 该 project 不在结果列表
///   - 本地版本缺失 → "未安装但有可用版本"，仍出 item（UI 改文案为"可安装"）
///   - 本地 >= 远端 → 不出 item
export function findUpdatesAvailable(
    local: LocalVersionSnapshot,
    remote: ReleaseSnapshotView,
): UpdateAvailableItem[] {
    const out: UpdateAvailableItem[] = [];

    if (remote.napcat) {
        const cmp = local.napcat ? compareSemver(local.napcat, remote.napcat.version) : Infinity;
        if (cmp > 0) {
            out.push({
                project: 'napcat',
                localVersion: local.napcat ?? null,
                remoteVersion: remote.napcat.version,
                htmlUrl: remote.napcat.htmlUrl,
                releaseNotes: remote.napcat.releaseNotes,
            });
        }
    }

    if (remote.snowluma) {
        const cmp = local.snowluma ? compareSemver(local.snowluma, remote.snowluma.version) : Infinity;
        if (cmp > 0) {
            out.push({
                project: 'snowluma',
                localVersion: local.snowluma ?? null,
                remoteVersion: remote.snowluma.version,
                htmlUrl: remote.snowluma.htmlUrl,
                releaseNotes: remote.snowluma.releaseNotes,
            });
        }
    }

    // Desktop 暂不在 BootstrapSnapshot 里给出本地版本，只在用户已显式查询过
    // release 时把"远端最新"作为参考资料展示，不做"有更新"派生
    // （UI 改 NoticeTimeline 时按需消费 remote.desktop）。

    return out;
}
