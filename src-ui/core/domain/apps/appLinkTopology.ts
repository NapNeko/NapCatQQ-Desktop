// 对接拓扑：按 Bot runtime_target 与实例 host_id，不看 NapCat / SnowLuma。
// 正向（应用端连 Bot，如 MaiBot）听口在 Bot 侧，隧道方向和反向相反，但哪几种组合能连、要不要桌面端在线是一样的。

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

export function appLinkPairNote(botHostId: string | null, appHostId: string): string {
    if (!botHostId) return '';
    const topology = classifyAppLink(botHostId, appHostId);
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

export function appLinkPairEnabled(botHostId: string | null, appHostId: string): boolean {
    if (!botHostId) return true;
    return classifyAppLink(botHostId, appHostId) !== null;
}

// Docker 部署的 Bot 在容器里：它开的服务、连的 127.0.0.1 都是容器自己的，宿主机上的应用端和隧道口碰不到
export function isDockerBot(deploymentType: string | null | undefined): boolean {
    return deploymentType === 'docker';
}

export const DOCKER_BOT_NOTE = '（Docker 部署暂不支持）';

export function isDesktopSshLink(topology: AppLinkTopology | null): boolean {
    return topology === 'local_bot_remote_app' || topology === 'remote_bot_local_app';
}

export function isResidentLink(topology: AppLinkTopology | null): boolean {
    return topology === 'remote_bot_remote_app';
}
