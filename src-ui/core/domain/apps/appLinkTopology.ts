// 对接拓扑：宿主决定隧道的生命周期，Docker 决定容器与宿主间的连接方式。

import type { OneBotLinkMode } from '../../ipc/generated/domain/OneBotLinkMode';

export type AppLinkTopology =
    'same_host' | 'local_bot_remote_app' | 'remote_bot_local_app' | 'remote_bot_remote_app';

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
    deploymentType?: string | null,
): string {
    if (!botHostId) return '';
    const topology = classifyAppLink(botHostId, appHostId);
    const docker = isDockerBot(deploymentType);
    switch (topology) {
        case 'same_host':
            return docker ? '（Docker 桥接）' : '';
        case 'local_bot_remote_app':
        case 'remote_bot_local_app':
            return docker ? '（Docker 桥接 · SSH 隧道）' : '（经 SSH 隧道）';
        case 'remote_bot_remote_app':
            return docker ? '（Docker 桥接 · 常驻 SSH 隧道）' : '（主机常驻隧道）';
        default:
            return '';
    }
}

export function appLinkPairEnabled(botHostId: string | null, appHostId: string): boolean {
    if (!botHostId) return true;
    return classifyAppLink(botHostId, appHostId) !== null;
}

export function isDockerBot(deploymentType: string | null | undefined): boolean {
    return deploymentType === 'docker';
}

export function isDesktopSshLink(topology: AppLinkTopology | null): boolean {
    return topology === 'local_bot_remote_app' || topology === 'remote_bot_local_app';
}

export function isResidentLink(topology: AppLinkTopology | null): boolean {
    return topology === 'remote_bot_remote_app';
}

export function appLinkTransportDescription(
    topology: AppLinkTopology | null,
    mode: OneBotLinkMode,
    docker: boolean,
): { route: string; availability: string } {
    let route = docker
        ? mode === 'forward_ws'
            ? '应用端通过宿主机发布端口连接容器中的 WS 服务，端口映射自动配置。'
            : 'Bot 通过仅绑定 Docker 网桥地址的专用 SSH 桥接连接应用端。'
        : '同一主机直接连接。';

    if (topology && topology !== 'same_host') {
        route = docker
            ? `${route} 跨主机部分自动建立 SSH 隧道。`
            : '跨主机自动建立 SSH 隧道，无需公开应用端监听端口。';
    }

    const availability = isDesktopSshLink(topology)
        ? '此连接经过本机桌面端，需保持桌面端在线；退出后隧道断开。'
        : isResidentLink(topology) || (docker && topology === 'same_host' && mode === 'reverse_ws')
          ? '隧道常驻远端主机，桌面端退出后仍保留。主机重启后需重新连接桌面端恢复隧道。'
          : topology === 'same_host'
            ? '两端运行时可直接通信，连接不依赖桌面端隧道。'
            : '';

    return { route, availability };
}
