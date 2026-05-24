// Home 页 NoticeTimeline 的派生层。
//
// 把 4 个数据源合成一份按重要性排序的 NoticeItem 列表（取最近 N 条）：
//   - bootstrap.report.warnings：迁移诊断告警（静态，不变）
//   - bootstrap.local_versions vs releases：版本更新提示
//   - DomainEvent 流：登录失效 / 进程崩溃 / daemon 崩溃 等运行时通知
//   - 静态条件：未安装 NapCat / 暂无 Bot 实例（可选，目前先不做）
//
// 严守 frontend-layering：纯函数 + 类型化输入输出，零 React / 零 IPC。
//
// legacy 对照：`legacy-python/src/core/home/notice_service.py`
// HomeNoticeService 把通知分 reminder / runtime / update 三 section，
// 这里做了 v1 简化：合成单一 list，section 由 UI 层决定怎么分组展示。

import type {
    BootstrapSnapshot,
    BotActorSnapshot,
    DomainEvent,
} from '../../ipc/types';
import {
    findUpdatesAvailable,
    type ReleaseSnapshotView,
    type UpdatableProject,
} from '../release/normalize';

/// 通知严重等级，决定 NoticeTimeline 圆点颜色。
export type NoticeTone = 'info' | 'success' | 'warning' | 'danger';

export interface NoticeItem {
    /// 稳定 id，用于 React key + dismiss 持久化。
    id: string;
    title: string;
    detail: string;
    tone: NoticeTone;
    /// 可选时间戳（Unix ms），缺失时 UI 不显示日期。
    timestamp?: number;
    /// 可选：来源分类（迁移诊断 / 版本更新 / 运行时事件）。
    source: 'migration' | 'update' | 'runtime' | 'system';
    /// 可选：点击跳转 URL（version update 通常带）。
    url?: string;
}

export interface NoticeAggregatorInput {
    bootstrap: BootstrapSnapshot | null | undefined;
    releases: ReleaseSnapshotView | null | undefined;
    /// 来自 useEventStream 的最近事件，**最新在前**。
    /// 这里只看 payload，不依赖 UiEventRecord 的 timestamp 字符串。
    recentEvents: { payload: DomainEvent }[];
    /// 可选：当前 Bot 列表，目前未消费，预留给"暂无 Bot 实例"提醒。
    bots?: BotActorSnapshot[];
}

const MAX_NOTICES = 8;

/// 把多源数据聚合成 NoticeItem 列表，按 tone 优先级 + 来源排序。
/// 优先级（高 → 低）：danger > warning > info > success；
/// 同优先级内：runtime > migration > update > system。
export function buildNotices(input: NoticeAggregatorInput): NoticeItem[] {
    const items: NoticeItem[] = [];

    items.push(...collectMigrationNotices(input.bootstrap));
    items.push(...collectUpdateNotices(input.bootstrap, input.releases));
    items.push(...collectRuntimeNotices(input.recentEvents));

    return items.sort(comparePriority).slice(0, MAX_NOTICES);
}

// ─── source: migration ───────────────────────────────────────────────────

function collectMigrationNotices(snap: BootstrapSnapshot | null | undefined): NoticeItem[] {
    if (!snap?.report?.warnings?.length) return [];
    return snap.report.warnings.map((w) => ({
        id: `migration:${w.code}`,
        title: `迁移诊断 · ${w.code}`,
        detail: w.message,
        tone: 'warning' as const,
        source: 'migration' as const,
    }));
}

// ─── source: update ──────────────────────────────────────────────────────

const PROJECT_LABEL: Record<UpdatableProject, string> = {
    napcat: 'NapCat',
    snowluma: 'SnowLuma',
    desktop: 'NapCatQQ Desktop',
};

function collectUpdateNotices(
    snap: BootstrapSnapshot | null | undefined,
    releases: ReleaseSnapshotView | null | undefined,
): NoticeItem[] {
    if (!snap || !releases) return [];
    const updates = findUpdatesAvailable(snap.local_versions, releases);
    return updates.map((u) => {
        const projectLabel = PROJECT_LABEL[u.project];
        const title = u.localVersion
            ? `${projectLabel} 有新版 ${u.remoteVersion}`
            : `${projectLabel} 可安装 ${u.remoteVersion}`;
        const detail = u.localVersion
            ? `当前 ${u.localVersion} → 最新 ${u.remoteVersion}`
            : `远端最新 ${u.remoteVersion}`;
        return {
            id: `update:${u.project}:${u.remoteVersion}`,
            title,
            detail,
            tone: 'info' as const,
            source: 'update' as const,
            url: u.htmlUrl,
        };
    });
}

// ─── source: runtime（DomainEvent 流） ──────────────────────────────────

/// 同一个 (kind, bot_id) 短时间内可能多次触发，去重保留最新一条。
function collectRuntimeNotices(events: { payload: DomainEvent }[]): NoticeItem[] {
    const seen = new Set<string>();
    const out: NoticeItem[] = [];
    for (const { payload } of events) {
        const item = describeRuntimeEvent(payload);
        if (!item) continue;
        // 同一 dedupKey 只保留第一条（list 是 newest first，第一条就是最新）
        const dedupKey = item.id;
        if (seen.has(dedupKey)) continue;
        seen.add(dedupKey);
        out.push(item);
    }
    return out;
}

function describeRuntimeEvent(event: DomainEvent): NoticeItem | null {
    switch (event.kind) {
        case 'napcat_login_invalidated': {
            const reasonText = event.reason === 'kicked' ? '账号被踢下线' : '已主动登出';
            return {
                id: `runtime:napcat_login_invalidated:${event.bot_id}`,
                title: `Bot ${event.bot_id} 登录失效`,
                detail: `${reasonText}，请重新扫码登录。`,
                tone: event.reason === 'kicked' ? 'warning' : 'info',
                source: 'runtime',
            };
        }
        case 'bot_process_exited': {
            const exit = event.exit_code;
            const isCrash = exit !== null && exit !== undefined && exit !== 0;
            if (!isCrash) return null;
            return {
                id: `runtime:bot_process_exited:${event.bot_id}`,
                title: `Bot ${event.bot_id} 进程异常退出`,
                detail: event.reason ?? `退出码 ${exit ?? 'unknown'}`,
                tone: 'danger',
                source: 'runtime',
            };
        }
        case 'bot_error': {
            return {
                id: `runtime:bot_error:${event.bot_id}`,
                title: `Bot ${event.bot_id} 异常`,
                detail: event.hint ? `${event.message}（${event.hint}）` : event.message,
                tone: 'danger',
                source: 'runtime',
            };
        }
        case 'snowluma_daemon_state_changed': {
            // 只关心 crashed；其它态变化是预期行为，不做通知
            if (event.state !== 'crashed') return null;
            return {
                id: `runtime:snowluma_daemon_crashed`,
                title: 'SnowLuma daemon 已崩溃',
                detail: event.reason ?? '所有 SnowLuma Bot 暂时不可用，请重启应用。',
                tone: 'danger',
                source: 'runtime',
            };
        }
        case 'napcat_login_qrcode': {
            return {
                id: `runtime:napcat_login_qrcode:${event.bot_id}`,
                title: `Bot ${event.bot_id} 等待扫码`,
                detail: '在 Bots 页面打开该实例并用 QQ 扫码登录。',
                tone: 'info',
                source: 'runtime',
            };
        }
        default:
            return null;
    }
}

// ─── 排序 ────────────────────────────────────────────────────────────────

const TONE_PRIORITY: Record<NoticeTone, number> = {
    danger: 0,
    warning: 1,
    info: 2,
    success: 3,
};

const SOURCE_PRIORITY: Record<NoticeItem['source'], number> = {
    runtime: 0,
    migration: 1,
    update: 2,
    system: 3,
};

function comparePriority(a: NoticeItem, b: NoticeItem): number {
    const t = TONE_PRIORITY[a.tone] - TONE_PRIORITY[b.tone];
    if (t !== 0) return t;
    return SOURCE_PRIORITY[a.source] - SOURCE_PRIORITY[b.source];
}
