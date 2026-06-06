// 概览副列「实例态势」与 Bot 舰队统计的纯函数派生。

import type { BotActorSnapshot } from '../../ipc/types';
import { isBotActive } from '../bot/status';

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