// 对接拓扑：按 Bot runtime_target 与实例 host_id，不看 NapCat / SnowLuma。
// 正向（应用端连 Bot，如 MaiBot）听口在 Bot 侧，隧道方向和反向相反，后端目前只开同机。

import type { OneBotLinkMode } from '../../core/ipc/types';

export type AppLinkTopology =
    | 'same_host'
    | 'local_bot_remote_app'
    | 'remote_bot_local_app'
    | 'remote_bot_remote_app';

export function classifyAppLink(
    botHostId: string | null,
    appHostId: string,
): AppLinkTopology | null {
    if (!botHostId) return null;
    if (botHostId === appHostId) return 'same_host';
    if (botHostId === 'local' && appHostId.startsWith('remote:')) {
        return 'local_bot_remote_app';
    }
    if (botHostId.startsWith('remote:') && appHostId === 'local') {
        return 'remote_bot_local_app';
    }
    if (botHostId.startsWith('remote:') && appHostId.startsWith('remote:')) {
        return 'remote_bot_remote_app';
    }
    return null;
}

export function appLinkPairNote(
    botHostId: string | null,
    appHostId: string,
    mode: OneBotLinkMode = 'reverse_ws',
): string {
    if (!botHostId) return '';
    const topology = classifyAppLink(botHostId, appHostId);
    if (mode === 'forward_ws') return topology === 'same_host' ? '' : '（需同一台机器）';
    switch (topology) {
        case 'same_host':
            return '';
        case 'local_bot_remote_app':
        case 'remote_bot_local_app':
            return '（经 SSH 隧道）';
        case 'remote_bot_remote_app':
            return '（主机常驻隧道）';
        default:
            return '';
    }
}

export function appLinkPairEnabled(
    botHostId: string | null,
    appHostId: string,
    mode: OneBotLinkMode = 'reverse_ws',
): boolean {
    if (!botHostId) return true;
    const topology = classifyAppLink(botHostId, appHostId);
    if (mode === 'forward_ws') return topology === 'same_host';
    return topology !== null;
}

export function isDesktopSshLink(topology: AppLinkTopology | null): boolean {
    return topology === 'local_bot_remote_app' || topology === 'remote_bot_local_app';
}

export function isResidentLink(topology: AppLinkTopology | null): boolean {
    return topology === 'remote_bot_remote_app';
}
