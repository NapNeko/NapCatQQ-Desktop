// 组件页 update/uninstall 门禁：对应 host 上有活跃 Bot 时禁止改组件。
// host_id 与组件探测一致：`local` / `remote:{serverProfileId}`。

import type { BotActorSnapshot } from '../../ipc/generated/BotActorSnapshot';
import type { BotConfig } from '../../ipc/generated/domain/BotConfig';
import {
    isRuntimeTargetLocal,
    remoteHostIdFromRuntimeTarget,
} from '../bot/runtime-target';

/** Starting / Running / Stopping — 与 Rust BotActorState::is_active 对齐 */
export function isBotActorActive(state: string): boolean {
    return state === 'starting' || state === 'running' || state === 'stopping';
}

/** Bot 配置落在哪台组件主机上 */
export function componentHostIdFromRuntimeTarget(runtimeTarget: string): string | null {
    if (isRuntimeTargetLocal(runtimeTarget)) return 'local';
    return remoteHostIdFromRuntimeTarget(runtimeTarget);
}

export function countActiveBotsOnComponentHost(
    snapshots: readonly BotActorSnapshot[],
    configs: Readonly<Record<string, BotConfig | null | undefined>>,
    hostId: string,
): number {
    let n = 0;
    for (const snap of snapshots) {
        if (!isBotActorActive(snap.state)) continue;
        const cfg = configs[snap.bot_id];
        if (!cfg) continue;
        const botHost = componentHostIdFromRuntimeTarget(cfg.bot.runtime_target);
        if (botHost === hostId) n += 1;
    }
    return n;
}

/** 行按钮禁用：更新/卸载共用同一提示（不写死「更新」或「卸载」） */
export function componentLifecycleBlockedReason(
    snapshots: readonly BotActorSnapshot[],
    configs: Readonly<Record<string, BotConfig | null | undefined>>,
    hostId: string,
): string | null {
    const active = countActiveBotsOnComponentHost(snapshots, configs, hostId);
    if (active <= 0) return null;
    return `该机器上仍有 ${active} 个 Bot 处于启动中/运行中/停止中，请先全部停止后再更新或卸载组件`;
}

export function componentMutationBlockedReason(
    snapshots: readonly BotActorSnapshot[],
    configs: Readonly<Record<string, BotConfig | null | undefined>>,
    hostId: string,
    kind: 'update' | 'uninstall' | string,
): string | null {
    if (kind !== 'update' && kind !== 'uninstall') return null;
    const active = countActiveBotsOnComponentHost(snapshots, configs, hostId);
    if (active <= 0) return null;
    const action = kind === 'update' ? '更新' : '卸载';
    return `该机器上仍有 ${active} 个 Bot 处于启动中/运行中/停止中，请先全部停止后再${action}组件`;
}
