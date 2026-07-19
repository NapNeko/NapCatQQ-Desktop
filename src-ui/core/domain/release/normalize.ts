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
    /// 远端 ncd-watch 二进制（按机安装，不进本机 LocalVersionSnapshot）。
    ncdWatch: ReleaseInfoView | null;
    /// Linux QQ 宿主最新版本（组件页更新用）。
    qqLinux: ReleaseInfoView | null;
    /// Windows QQ 宿主最新版本。
    qqWindows: ReleaseInfoView | null;
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

export function normalizeReleaseSnapshot(
    snap: ReleaseSnapshot | null | undefined,
): ReleaseSnapshotView {
    if (!snap) {
        return {
            napcat: null,
            snowluma: null,
            desktop: null,
            ncdWatch: null,
            qqLinux: null,
            qqWindows: null,
            fetchedAt: null,
        };
    }
    return {
        napcat: toView(snap.napcat_latest),
        snowluma: toView(snap.snowluma_latest),
        desktop: toView(snap.desktop_latest),
        ncdWatch: toView(snap.ncd_watch_latest),
        qqLinux: toView(snap.qq_linux_latest),
        qqWindows: toView(snap.qq_windows_latest),
        fetchedAt:
            snap.fetched_at !== null && snap.fetched_at !== undefined
                ? Number(snap.fetched_at)
                : null,
    };
}

// ─── 版本比对 ────────────────────────────────────────────────────────────

/// 组件/二进制版本归一：clap 常输出 `ncd-watch 0.2.0`，release 可能是
/// `watch-v0.2.0` / `v0.2.0` / `0.2.0`。先取末段再剥前缀，便于 compareSemver。
export function normalizeComponentVersion(version: string): string {
    const trimmed = version.trim();
    if (!trimmed) return '';
    const token = trimmed.includes(' ')
        ? (trimmed.split(/\s+/).pop() ?? trimmed)
        : trimmed;
    return token.replace(/^watch-/i, '').replace(/^[vV]/, '');
}

/// QQ 版本常为 `3.2.25-45758` / `3.2.31` / `9.9.31`：先比主版本数字段，再比
/// `-` 后 build id（能解析成数字时）。用于组件页 hasUpdate。
export function compareComponentVersion(local: string, remote: string): number {
    const localNorm = normalizeComponentVersion(local);
    const remoteNorm = normalizeComponentVersion(remote);
    if (!localNorm || !remoteNorm) return 0;

    const [localCore, localBuildRaw] = splitVersionBuild(localNorm);
    const [remoteCore, remoteBuildRaw] = splitVersionBuild(remoteNorm);

    const coreCmp = compareNumericCore(localCore, remoteCore);
    if (coreCmp !== 0) return coreCmp;

    const localBuild = parseBuildId(localBuildRaw);
    const remoteBuild = parseBuildId(remoteBuildRaw);
    if (localBuild != null && remoteBuild != null && localBuild !== remoteBuild) {
        return remoteBuild - localBuild;
    }
    // 一边有 build、一边没有：无 build 视为官方简写主版本，不强制判更新
    return 0;
}

function splitVersionBuild(version: string): [string, string | null] {
    const idx = version.indexOf('-');
    if (idx < 0) return [version, null];
    return [version.slice(0, idx), version.slice(idx + 1)];
}

function parseBuildId(raw: string | null): number | null {
    if (!raw) return null;
    const m = raw.match(/^\d+/);
    if (!m) return null;
    const n = Number.parseInt(m[0], 10);
    return Number.isFinite(n) ? n : null;
}

function compareNumericCore(local: string, remote: string): number {
    const localParts = parseNumericParts(local);
    const remoteParts = parseNumericParts(remote);
    const len = Math.max(localParts.length, remoteParts.length);
    for (let i = 0; i < len; i++) {
        const a = localParts[i] ?? 0;
        const b = remoteParts[i] ?? 0;
        if (a !== b) return b - a;
    }
    return 0;
}

/// 简化版本比对：拆 `1.2.3` / `1.2.3-rc1` 这类 SemVer 数字段后逐位比较。
///
/// 返回：
///   - > 0  remote 比 local 新
///   - < 0  local 比 remote 新
///   - 0    一致 / 无法比较
///
/// QQ 宿主本地常是 `9.9.32-50969`，pcConfig 远端只有 `9.9.32`。数字 build
/// 走 compareComponentVersion（缺 build = 官方简写，等主版本即相等），不能
/// 再落到下面的 SemVer 预发布启发式，否则会误报可更新。
export function compareSemver(local: string, remote: string): number {
    if (local.includes('-') || remote.includes('-')) {
        const via = compareComponentVersion(local, remote);
        if (via !== 0) return via;

        const localNormEarly = normalizeComponentVersion(local);
        const remoteNormEarly = normalizeComponentVersion(remote);
        const [, localBuildRaw] = splitVersionBuild(localNormEarly);
        const [, remoteBuildRaw] = splitVersionBuild(remoteNormEarly);
        // 任一侧是 QQ 式数字 build：component 已判等则结束，勿当 pre-release
        if (parseBuildId(localBuildRaw) != null || parseBuildId(remoteBuildRaw) != null) {
            return 0;
        }
    }
    const localNorm = normalizeComponentVersion(local);
    const remoteNorm = normalizeComponentVersion(remote);
    const localParts = parseNumericParts(localNorm);
    const remoteParts = parseNumericParts(remoteNorm);
    const len = Math.max(localParts.length, remoteParts.length);
    for (let i = 0; i < len; i++) {
        const a = localParts[i] ?? 0;
        const b = remoteParts[i] ?? 0;
        if (a !== b) return b - a;
    }
    // 非数字后缀才按 SemVer 预发布：1.2.3-rc1 < 1.2.3
    const localHasPre = localNorm.includes('-');
    const remoteHasPre = remoteNorm.includes('-');
    if (localHasPre && !remoteHasPre) return 1;
    if (!localHasPre && remoteHasPre) return -1;
    return 0;
}

function parseNumericParts(version: string): number[] {
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
/// ncd-watch 按远端主机安装，不进本机 LocalVersionSnapshot，由组件页/设置页按机比对。
/// QQ 宿主按机安装，由组件页 latestVersionFor('qq') 驱动更新按钮。
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
        const cmp = local.snowluma
            ? compareSemver(local.snowluma, remote.snowluma.version)
            : Infinity;
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
