// 概览副列「一眼三项」与 Bot 态势的纯函数派生。

import type { BotActorSnapshot } from '../../ipc/types';
import type { BootstrapSnapshot } from '../../ipc/types';
import type { LocalVersionSnapshot } from '../../ipc/types';
import { isBotActive } from '../bot/status';
import { findUpdatesAvailable, type ReleaseSnapshotView } from '../release/normalize';

export interface BotFleetStats {
    total: number;
    running: number;
    active: number;
    crashed: number;
    pendingRestart: number;
    stopped: number;
}

export function computeBotFleetStats(snapshots: BotActorSnapshot[]): BotFleetStats {
    let running = 0;
    let active = 0;
    let crashed = 0;
    let pendingRestart = 0;
    let stopped = 0;
    for (const s of snapshots) {
        if (s.state === 'running') running += 1;
        if (isBotActive(s.state)) active += 1;
        if (s.state === 'crashed') crashed += 1;
        if (s.pending_restart) pendingRestart += 1;
        if (s.state === 'stopped') stopped += 1;
    }
    return {
        total: snapshots.length,
        running,
        active,
        crashed,
        pendingRestart,
        stopped,
    };
}

export interface ActionableBotItem {
    botId: string;
    title: string;
    detail: string;
}

export function listActionableBots(snapshots: BotActorSnapshot[]): ActionableBotItem[] {
    const out: ActionableBotItem[] = [];
    for (const s of snapshots) {
        if (s.state === 'crashed') {
            out.push({
                botId: s.bot_id,
                title: s.bot_id,
                detail: s.last_error?.trim() || '进程异常退出',
            });
            continue;
        }
        if (s.pending_restart) {
            out.push({
                botId: s.bot_id,
                title: s.bot_id,
                detail: '等待重启',
            });
            continue;
        }
        if (s.last_error?.trim()) {
            out.push({
                botId: s.bot_id,
                title: s.bot_id,
                detail: s.last_error.trim(),
            });
        }
    }
    return out.slice(0, 6);
}

export function countKernelUpdates(
    local: LocalVersionSnapshot,
    releases: ReleaseSnapshotView,
): number {
    return findUpdatesAvailable(local, releases).filter(
        (u) => u.project === 'napcat' || u.project === 'snowluma',
    ).length;
}

export interface SelfCheckGlance {
    /** 0 = 就绪且无告警；>0 表示需关注条数。 */
    issueCount: number;
    label: string;
    tone: 'success' | 'warning' | 'danger';
}

export function glanceSelfCheck(bootstrap: BootstrapSnapshot | null | undefined): SelfCheckGlance {
    if (!bootstrap) {
        return { issueCount: 0, label: '加载中', tone: 'success' };
    }
    const warnings = bootstrap.report?.warnings?.length ?? 0;
    const status = bootstrap.status;
    if (status === 'failed') {
        return { issueCount: 1, label: '自检未通过', tone: 'danger' };
    }
    if (status !== 'ready' || warnings > 0) {
        const n = warnings > 0 ? warnings : 1;
        return {
            issueCount: n,
            label: warnings > 0 ? `${warnings} 条迁移告警` : '需确认',
            tone: 'warning',
        };
    }
    return { issueCount: 0, label: '就绪', tone: 'success' };
}