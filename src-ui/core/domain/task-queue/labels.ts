// 任务队列展示用文案；不依赖 ComponentRow 反查。

import type { ComponentId } from '../../ipc/types';

const COMPONENT_DISPLAY_NAME: Record<ComponentId, string> = {
    napcat: 'NapCat',
    snowluma: 'SnowLuma',
    qq: 'QQ',
    nodejs: 'Node.js',
    novnc: 'noVNC',
    desktop_self: 'Desktop',
};

const STEP_KIND_LABEL: Record<string, string> = {
    install: '安装',
    update: '更新',
    uninstall: '卸载',
};

export function componentDisplayName(componentId: ComponentId): string {
    return COMPONENT_DISPLAY_NAME[componentId] ?? componentId;
}

export function componentActionTitle(
    componentId: ComponentId,
    stepKind: string | undefined,
    message: string,
): string {
    const name = componentDisplayName(componentId);
    const step = stepKind ? STEP_KIND_LABEL[stepKind] : undefined;
    if (step) return `${name} · ${step}`;
    const msg = message?.trim();
    if (msg) return `${name} · ${msg}`;
    return `${name} · 完成`;
}

export function dockerInstallTitle(hostLabel: string): string {
    const label = hostLabel?.trim() || '远程主机';
    return `Docker · ${label}`;
}

export function dockerDeployTitle(hostLabel: string, container?: string): string {
    const label = hostLabel?.trim() || '远程主机';
    if (container?.trim()) return `Docker 部署 · ${container} · ${label}`;
    return `Docker 部署 · ${label}`;
}